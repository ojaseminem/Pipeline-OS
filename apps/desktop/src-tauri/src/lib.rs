use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;
use tauri::{Emitter, Manager, State};
use tauri_plugin_updater::UpdaterExt;
use vantadeck_application::{
    AppSummary, ApplicationService, EngineChoice, HealthSummary, LaunchResult, ProjectSummary,
};
use vantadeck_domain::{
    AppCategory, AppInstallation, DetectedApplication, HealthIssue, ProjectConfig,
};
use vantadeck_launcher::LaunchSpec;
use vantadeck_manifests::{AppManifest, ToolManifest};
use vantadeck_storage::{RegisteredProject, Storage};
use vantadeck_vcs::GitProvider;
use vantadeck_vcs::VcsOperationResult;

/// Categories that represent launchable creative applications. Version-control
/// tooling is detected for project workflows but is never launched directly.
fn is_launchable_category(category: &str) -> bool {
    matches!(category, "game-engine" | "dcc" | "art" | "code")
}

fn category_str(category: &AppCategory) -> &'static str {
    match category {
        AppCategory::GameEngine => "game-engine",
        AppCategory::Dcc => "dcc",
        AppCategory::Art => "art",
        AppCategory::Code => "code",
        AppCategory::VersionControl => "version-control",
        AppCategory::Utility => "utility",
    }
}

/// The application catalog, derived from the bundled manifests so any manifest
/// added under `manifests/apps/` automatically appears in the app — no parallel
/// hard-coded list to maintain. Returns (id, display name, category).
fn manifest_catalog(manifest_dir: &Path) -> Vec<(String, String, String)> {
    let mut catalog = Vec::new();
    let Ok(entries) = std::fs::read_dir(manifest_dir) else {
        return catalog;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(manifest) = AppManifest::from_json(&text) else {
            continue;
        };
        catalog.push((
            manifest.id,
            manifest.name,
            category_str(&manifest.category).to_string(),
        ));
    }
    catalog.sort_by_key(|(_, name, _)| name.to_lowercase());
    catalog
}
const TOOL_INDEX_SOURCE: &str = "https://tools.vantadeck.org/v1/index.json";

struct DesktopState {
    service: ApplicationService,
    manifest_dir: PathBuf,
    scan_cancel: Arc<AtomicBool>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DesktopProject {
    name: String,
    path: String,
    pinned: bool,
    tags: Vec<String>,
    category: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DesktopActivity {
    id: i64,
    kind: String,
    message: String,
    created_at: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DesktopInstallation {
    version: String,
    executable: String,
    state: String,
    /// Whether this binary's architecture can run on the current machine.
    runnable: bool,
}

impl From<AppInstallation> for DesktopInstallation {
    fn from(installation: AppInstallation) -> Self {
        let runnable = vantadeck_launcher::ensure_runnable(&installation.executable).is_ok();
        DesktopInstallation {
            version: installation.version.to_string(),
            executable: installation.executable.display().to_string(),
            state: format!("{:?}", installation.state).to_lowercase(),
            runnable,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DesktopApp {
    id: String,
    name: String,
    category: String,
    launchable: bool,
    installations: Vec<DesktopInstallation>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DesktopDashboard {
    network_enabled: bool,
    continue_project: Option<ProjectSummary>,
    pinned_projects: Vec<ProjectSummary>,
    recent_projects: Vec<ProjectSummary>,
    apps: Vec<AppSummary>,
    health: Vec<HealthSummary>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DesktopGitStatus {
    branch: Option<String>,
    ahead: u32,
    behind: u32,
    changed_files: Vec<DesktopChangedFile>,
}

#[derive(Serialize)]
struct DesktopChangedFile {
    path: String,
    status: String,
}

/// Known creative engines/tools in priority order, mapping catalog id to a
/// friendly display name. Used to label a project by its real engine (Unity,
/// Unreal, …) rather than the generic project type.
const ENGINE_LABELS: &[(&str, &str)] = &[
    ("unity", "Unity"),
    ("unreal-engine", "Unreal Engine"),
    ("godot", "Godot"),
    ("blender", "Blender"),
    ("maya", "Maya"),
];

/// Derives a project category from its explicit setting or its linked apps.
fn derive_category(config: &ProjectConfig) -> String {
    if let Some(category) = config.category.as_deref().filter(|value| !value.is_empty()) {
        return category.to_string();
    }
    let has = |id: &str| config.linked_apps.iter().any(|app| app.app_id == id);
    if has("unity") || has("unreal-engine") || has("godot") {
        "Game Dev".into()
    } else if has("zbrush") {
        "Sculpting".into()
    } else if has("substance-painter") || has("substance-designer") {
        "Texturing".into()
    } else if has("maya") || has("blender") || has("3dsmax") || has("houdini") || has("cinema4d") {
        "3D".into()
    } else if has("photoshop") || has("krita") || has("aseprite") {
        "2D / Concept".into()
    } else if config.project_type == "game-development" {
        "Game Dev".into()
    } else if !config.linked_apps.is_empty() {
        "3D".into()
    } else {
        "Other".into()
    }
}

/// Derives (display name, catalog id, preferred version) for a project's
/// primary engine from its linked apps, falling back to the project type.
fn derive_engine(config: &ProjectConfig) -> (String, String, String) {
    for (id, label) in ENGINE_LABELS {
        if let Some(app) = config.linked_apps.iter().find(|app| app.app_id == *id) {
            return (
                (*label).to_string(),
                (*id).to_string(),
                app.preferred_version.clone().unwrap_or_default(),
            );
        }
    }
    if let Some(app) = config.linked_apps.first() {
        return (
            app.app_id.clone(),
            app.app_id.clone(),
            app.preferred_version.clone().unwrap_or_default(),
        );
    }
    // No linked engine: present the project type as a readable label.
    let label = config
        .project_type
        .split(['-', '_'])
        .map(|word| {
            let mut chars = word.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().chain(chars).collect::<String>())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ");
    (label, String::new(), String::new())
}

/// Builds a rich project summary (engine name/icon/version, branch, last-opened,
/// thumbnail) for the dashboard. Reads the portable config, the engine's
/// detected executable (for its real icon), and a cheap current branch.
async fn build_summary(
    service: &ApplicationService,
    project: &RegisteredProject,
) -> ProjectSummary {
    let config = service.project_config(&project.root).await.ok();
    let (engine, engine_id, version) = config
        .as_ref()
        .map(derive_engine)
        .unwrap_or_else(|| ("Project".into(), String::new(), String::new()));
    let engine_executable = if engine_id.is_empty() {
        None
    } else {
        service
            .detected_installations(&engine_id)
            .await
            .ok()
            .and_then(|installations| {
                installations
                    .into_iter()
                    .map(|installation| installation.executable.display().to_string())
                    .next()
            })
    };
    let thumbnail = config.as_ref().and_then(|config| config.thumbnail.clone());
    let category = config
        .as_ref()
        .map(derive_category)
        .unwrap_or_else(|| "Other".into());
    let branch = service
        .vcs_current_branch(&project.root)
        .await
        .unwrap_or_default();
    ProjectSummary {
        name: project.name.clone(),
        path: project.root.display().to_string(),
        engine,
        engine_id,
        engine_executable,
        version,
        branch,
        last_opened: project.last_opened.clone().unwrap_or_default(),
        thumbnail,
        category,
    }
}

async fn apps(
    service: &ApplicationService,
    manifest_dir: &Path,
) -> Result<Vec<DesktopApp>, String> {
    let catalog = manifest_catalog(manifest_dir);
    let mut result = Vec::with_capacity(catalog.len());
    for (id, name, category) in catalog {
        result.push(DesktopApp {
            launchable: is_launchable_category(&category),
            installations: service
                .detected_installations(&id)
                .await
                .map_err(|e| e.to_string())?
                .into_iter()
                .map(DesktopInstallation::from)
                .collect(),
            id,
            name,
            category,
        });
    }
    Ok(result)
}

#[tauri::command]
async fn dashboard_snapshot(state: State<'_, DesktopState>) -> Result<DesktopDashboard, String> {
    let projects = state
        .service
        .registered_projects()
        .await
        .map_err(|e| e.to_string())?;
    let mut summaries = Vec::with_capacity(projects.len());
    for project in &projects {
        summaries.push(build_summary(&state.service, project).await);
    }
    let detected = apps(&state.service, &state.manifest_dir).await?;
    // Cached-first: the dashboard returns the last cached health instantly with
    // no recompute, so opening is fast. The UI then refreshes via `refresh_health`
    // (a quick pass on focus, a full scan on launch/recheck).
    let mut health = Vec::new();
    for project in &projects {
        if let Some((issues, _)) = state.service.cached_project_health(&project.root).await {
            for issue in issues {
                health.push(HealthSummary {
                    code: issue.code,
                    title: issue.title,
                    detail: issue.detail,
                    severity: format!("{:?}", issue.severity).to_lowercase(),
                    project: project.name.clone(),
                });
            }
        }
    }
    Ok(DesktopDashboard {
        network_enabled: false,
        continue_project: summaries.first().cloned(),
        pinned_projects: projects
            .iter()
            .zip(&summaries)
            .filter(|(p, _)| p.pinned)
            .map(|(_, s)| s.clone())
            .collect(),
        recent_projects: summaries.clone(),
        apps: detected
            .into_iter()
            .filter(|app| app.launchable && !app.installations.is_empty())
            .map(|app| {
                let primary = app.installations.iter().find(|i| i.runnable);
                AppSummary {
                    executable: primary.map(|i| i.executable.clone()),
                    versions: app
                        .installations
                        .iter()
                        .map(|i| i.version.clone())
                        .collect(),
                    id: app.id,
                    name: app.name,
                    category: app.category,
                }
            })
            .collect(),
        health,
    })
}

#[tauri::command]
async fn list_projects(state: State<'_, DesktopState>) -> Result<Vec<DesktopProject>, String> {
    let projects = state
        .service
        .registered_projects()
        .await
        .map_err(|e| e.to_string())?;
    let mut result = Vec::with_capacity(projects.len());
    for project in projects {
        let config = state.service.project_config(&project.root).await.ok();
        let tags = config
            .as_ref()
            .map(|config| config.tags.clone())
            .unwrap_or_default();
        let category = config
            .as_ref()
            .map(derive_category)
            .unwrap_or_else(|| "Other".into());
        result.push(DesktopProject {
            name: project.name,
            path: project.root.display().to_string(),
            pinned: project.pinned,
            tags,
            category,
        });
    }
    Ok(result)
}

/// Sets a project's portable category (written to project.toml; empty clears it).
#[tauri::command(rename_all = "camelCase")]
async fn set_project_category(
    root: String,
    category: Option<String>,
    state: State<'_, DesktopState>,
) -> Result<(), String> {
    state
        .service
        .set_project_category(Path::new(&root), category.as_deref())
        .await
        .map_err(|e| e.to_string())
}

/// Unregisters a project (does not delete its files).
#[tauri::command(rename_all = "camelCase")]
async fn remove_project(root: String, state: State<'_, DesktopState>) -> Result<(), String> {
    state
        .service
        .remove_project(Path::new(&root))
        .await
        .map_err(|e| e.to_string())
}

/// Sets a project's portable tags (written to project.toml).
#[tauri::command(rename_all = "camelCase")]
async fn set_project_tags(
    root: String,
    tags: Vec<String>,
    state: State<'_, DesktopState>,
) -> Result<(), String> {
    state
        .service
        .set_project_tags(Path::new(&root), &tags)
        .await
        .map_err(|e| e.to_string())
}

/// Reads the portable project workspace document (notes/to-dos/references JSON).
#[tauri::command(rename_all = "camelCase")]
async fn read_project_workspace(
    root: String,
    state: State<'_, DesktopState>,
) -> Result<Option<String>, String> {
    state
        .service
        .read_project_workspace(Path::new(&root))
        .await
        .map_err(|e| e.to_string())
}

/// Writes the portable project workspace document.
#[tauri::command(rename_all = "camelCase")]
async fn save_project_workspace(
    root: String,
    contents: String,
    state: State<'_, DesktopState>,
) -> Result<(), String> {
    state
        .service
        .save_project_workspace(Path::new(&root), &contents)
        .await
        .map_err(|e| e.to_string())
}

/// Recent durable activity (imports, scans, launches, commits, …).
#[tauri::command]
async fn recent_activity(
    limit: u32,
    state: State<'_, DesktopState>,
) -> Result<Vec<DesktopActivity>, String> {
    Ok(state
        .service
        .recent_activity(limit)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|record| DesktopActivity {
            id: record.id,
            kind: record.kind,
            message: record.message,
            created_at: record.created_at,
        })
        .collect())
}

#[tauri::command(rename_all = "camelCase")]
async fn import_project(
    root: String,
    name: Option<String>,
    state: State<'_, DesktopState>,
) -> Result<ProjectConfig, String> {
    state
        .service
        .import_project(Path::new(&root), name.as_deref())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn project_health(
    root: String,
    state: State<'_, DesktopState>,
) -> Result<Vec<HealthIssue>, String> {
    Ok(state.service.project_health(Path::new(&root)).await)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CachedHealth {
    issues: Vec<HealthIssue>,
    checked_at: String,
}

/// Returns the cached health result for a project (no recompute), if any.
#[tauri::command(rename_all = "camelCase")]
async fn cached_health(
    root: String,
    state: State<'_, DesktopState>,
) -> Result<Option<CachedHealth>, String> {
    Ok(state
        .service
        .cached_project_health(Path::new(&root))
        .await
        .map(|(issues, checked_at)| CachedHealth { issues, checked_at }))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectHealthOverview {
    name: String,
    path: String,
    checked_at: Option<String>,
    issues: Vec<HealthIssue>,
}

/// Per-project cached health for the Health screen's initial render (so it is
/// never blank). Projects with no cached result are returned with no issues.
#[tauri::command]
async fn health_overview(
    state: State<'_, DesktopState>,
) -> Result<Vec<ProjectHealthOverview>, String> {
    let projects = state
        .service
        .registered_projects()
        .await
        .map_err(|e| e.to_string())?;
    let mut overview = Vec::with_capacity(projects.len());
    for project in &projects {
        let (issues, checked_at) = match state.service.cached_project_health(&project.root).await {
            Some((issues, checked_at)) => (issues, Some(checked_at)),
            None => (Vec::new(), None),
        };
        overview.push(ProjectHealthOverview {
            name: project.name.clone(),
            path: project.root.display().to_string(),
            checked_at,
            issues,
        });
    }
    Ok(overview)
}

/// Refreshes health for every project and returns the aggregated rollup.
/// `full` runs the complete check (engine, profiles, Git, LFS); otherwise a
/// lightweight pass (no Git subprocesses) is used. Both update the cache.
#[tauri::command]
async fn refresh_health(
    full: bool,
    state: State<'_, DesktopState>,
) -> Result<Vec<HealthSummary>, String> {
    let projects = state
        .service
        .registered_projects()
        .await
        .map_err(|e| e.to_string())?;
    let mut health = Vec::new();
    for project in &projects {
        let issues = if full {
            state.service.project_health(&project.root).await
        } else {
            state
                .service
                .refresh_project_health_quick(&project.root)
                .await
        };
        for issue in issues {
            health.push(HealthSummary {
                code: issue.code,
                title: issue.title,
                detail: issue.detail,
                severity: format!("{:?}", issue.severity).to_lowercase(),
                project: project.name.clone(),
            });
        }
    }
    Ok(health)
}

/// Renames a project (updates project.toml and the local registry).
#[tauri::command(rename_all = "camelCase")]
async fn rename_project(
    root: String,
    name: String,
    state: State<'_, DesktopState>,
) -> Result<(), String> {
    state
        .service
        .rename_project(Path::new(&root), &name)
        .await
        .map_err(|e| e.to_string())
}

/// Records that a project was opened (updates its last-opened timestamp).
#[tauri::command(rename_all = "camelCase")]
async fn record_project_opened(root: String, state: State<'_, DesktopState>) -> Result<(), String> {
    state
        .service
        .touch_project_opened(Path::new(&root))
        .await
        .map_err(|e| e.to_string())
}

/// Sets a portable, team-shared project thumbnail from a chosen source image.
/// Returns the project-relative path stored in `project.toml`.
#[tauri::command(rename_all = "camelCase")]
async fn set_project_thumbnail(
    root: String,
    source: String,
    state: State<'_, DesktopState>,
) -> Result<String, String> {
    state
        .service
        .set_project_thumbnail(Path::new(&root), Path::new(&source))
        .await
        .map_err(|e| e.to_string())
}

/// Clears a project's thumbnail (removes the file and the config entry).
#[tauri::command(rename_all = "camelCase")]
async fn clear_project_thumbnail(
    root: String,
    state: State<'_, DesktopState>,
) -> Result<(), String> {
    state
        .service
        .clear_project_thumbnail(Path::new(&root))
        .await
        .map_err(|e| e.to_string())
}

/// Files changed by a single commit (for the source-control history view).
#[tauri::command(rename_all = "camelCase")]
async fn git_commit_files(
    root: String,
    hash: String,
    state: State<'_, DesktopState>,
) -> Result<Vec<DesktopChangedFile>, String> {
    Ok(state
        .service
        .vcs_commit_files(Path::new(&root), &hash)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|file| DesktopChangedFile {
            path: file.path,
            status: file.status,
        })
        .collect())
}

#[tauri::command(rename_all = "camelCase")]
async fn git_status(
    root: String,
    state: State<'_, DesktopState>,
) -> Result<DesktopGitStatus, String> {
    let status = state
        .service
        .vcs_status(Path::new(&root))
        .await
        .map_err(|e| e.to_string())?;
    Ok(DesktopGitStatus {
        branch: status.branch,
        ahead: status.ahead,
        behind: status.behind,
        changed_files: status
            .changed_files
            .into_iter()
            .map(|file| DesktopChangedFile {
                path: file.path,
                status: file.status,
            })
            .collect(),
    })
}

#[tauri::command(rename_all = "camelCase")]
async fn git_log(
    root: String,
    limit: u32,
    state: State<'_, DesktopState>,
) -> Result<Vec<vantadeck_vcs::GitCommit>, String> {
    state
        .service
        .vcs_log(Path::new(&root), limit)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn git_diff(
    root: String,
    path: String,
    state: State<'_, DesktopState>,
) -> Result<String, String> {
    state
        .service
        .vcs_diff(Path::new(&root), &path)
        .await
        .map_err(|e| e.to_string())
}

/// Discards local changes to a single file (reverts to HEAD, or deletes if
/// untracked). Destructive — requires confirmation.
#[tauri::command(rename_all = "camelCase")]
async fn git_discard(
    root: String,
    path: String,
    confirmed: bool,
    state: State<'_, DesktopState>,
) -> Result<VcsOperationResult, String> {
    require_confirmation(confirmed)?;
    state
        .service
        .vcs_discard_path(Path::new(&root), &path, confirmed)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn git_commit_paths(
    root: String,
    message: String,
    paths: Vec<String>,
    confirmed: bool,
    state: State<'_, DesktopState>,
) -> Result<VcsOperationResult, String> {
    require_confirmation(confirmed)?;
    state
        .service
        .vcs_commit_paths(Path::new(&root), &message, &paths, confirmed)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn list_apps(state: State<'_, DesktopState>) -> Result<Vec<DesktopApp>, String> {
    apps(&state.service, &state.manifest_dir).await
}

#[tauri::command]
async fn scan_apps(
    app: tauri::AppHandle,
    roots: Vec<String>,
    state: State<'_, DesktopState>,
) -> Result<Vec<DetectedApplication>, String> {
    let roots = roots.into_iter().map(PathBuf::from).collect::<Vec<_>>();
    state.scan_cancel.store(false, Ordering::SeqCst);
    let cancel = state.scan_cancel.clone();
    state
        .service
        .scan_apps_with_progress(
            &state.manifest_dir,
            &roots,
            |progress| {
                let _ = app.emit("scan://progress", progress);
            },
            || cancel.load(Ordering::SeqCst),
        )
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn cancel_scan(state: State<'_, DesktopState>) -> Result<(), String> {
    state.scan_cancel.store(true, Ordering::SeqCst);
    Ok(())
}

#[tauri::command(rename_all = "camelCase")]
async fn set_manual_override(
    app_id: String,
    version: String,
    executable: String,
    state: State<'_, DesktopState>,
) -> Result<(), String> {
    let version = version.parse().map_err(|e: semver::Error| e.to_string())?;
    state
        .service
        .set_manual_override(&app_id, version, Path::new(&executable))
        .await
        .map_err(|e| e.to_string())
}

fn launch_is_allowed(installations: &[AppInstallation], executable: &Path) -> bool {
    installations
        .iter()
        .any(|candidate| candidate.executable == executable)
}

fn catalog_category(manifest_dir: &Path, app_id: &str) -> Option<String> {
    manifest_catalog(manifest_dir)
        .into_iter()
        .find(|(id, _, _)| id == app_id)
        .map(|(_, _, category)| category)
}

#[tauri::command(rename_all = "camelCase")]
async fn launch_app(
    app_id: String,
    executable: String,
    state: State<'_, DesktopState>,
) -> Result<(), String> {
    match catalog_category(&state.manifest_dir, &app_id).as_deref() {
        Some(category) if !is_launchable_category(category) => {
            return Err(format!(
                "{app_id} is a version-control tool and is used for project source control, not launched directly."
            ));
        }
        _ => {}
    }
    let executable = PathBuf::from(executable);
    let installations = state
        .service
        .detected_installations(&app_id)
        .await
        .map_err(|e| e.to_string())?;
    if !launch_is_allowed(&installations, &executable) || !executable.is_file() {
        return Err(
            "This executable is not a detected installation; rescan applications and try again."
                .into(),
        );
    }
    // Reject architecture-incompatible binaries before spawning so Windows never
    // raises a blocking "machine type mismatch" dialog the user cannot dismiss.
    vantadeck_launcher::ensure_runnable(&executable).map_err(|e| e.to_string())?;
    let working_directory = executable
        .parent()
        .ok_or("launch denied: executable has no parent directory")?
        .to_path_buf();
    LaunchSpec::new(executable, vec![], working_directory)
        .map_err(|e| e.to_string())?
        .command()
        .spawn()
        .map_err(|e| format!("Could not start the application: {e}"))?;
    Ok(())
}

/// Extracts the embedded icon from a Windows PE executable as `.ico` bytes.
fn extract_executable_icon(path: &Path) -> Option<Vec<u8>> {
    use pelite::pe64::{Pe, PeFile};
    let map = pelite::FileMap::open(path).ok()?;
    let bytes = map.as_ref();
    // 64-bit images are the common case; fall back to the 32-bit parser.
    let resources = match PeFile::from_bytes(bytes) {
        Ok(pe) => pe.resources().ok()?,
        Err(_) => {
            use pelite::pe32::{Pe as Pe32, PeFile as PeFile32};
            PeFile32::from_bytes(bytes).ok()?.resources().ok()?
        }
    };
    for (_name, icon) in resources.icons().filter_map(Result::ok) {
        let mut buffer = Vec::new();
        if icon.write(&mut buffer).is_ok() && !buffer.is_empty() {
            return Some(buffer);
        }
    }
    None
}

/// Returns an icon data URL for the native icon of an executable, or `None`
/// when no icon can be extracted. Used by the UI to show real application icons.
#[tauri::command(rename_all = "camelCase")]
async fn app_icon(executable: String) -> Result<Option<String>, String> {
    let path = PathBuf::from(&executable);
    if !path.is_file() {
        return Ok(None);
    }
    let icon = tauri::async_runtime::spawn_blocking(move || extract_executable_icon(&path))
        .await
        .map_err(|e| e.to_string())?;
    Ok(icon.map(|bytes| {
        use base64::Engine;
        format!(
            "data:image/x-icon;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        )
    }))
}

#[tauri::command]
async fn list_tools(state: State<'_, DesktopState>) -> Result<Vec<ToolManifest>, String> {
    state
        .service
        .cached_tools(TOOL_INDEX_SOURCE)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn set_project_pinned(
    root: String,
    pinned: bool,
    state: State<'_, DesktopState>,
) -> Result<(), String> {
    state
        .service
        .set_project_pinned(Path::new(&root), pinned)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn launch_project_profile(
    root: String,
    profile_id: String,
    state: State<'_, DesktopState>,
) -> Result<LaunchResult, String> {
    state
        .service
        .launch_project_profile(Path::new(&root), &profile_id)
        .await
        .map_err(|error| error.to_string())
}

/// Opens a project directly in its connected engine (no launch profile needed).
/// `appId` targets a specific linked app; omit it for the primary engine.
#[tauri::command(rename_all = "camelCase")]
async fn open_in_engine(
    root: String,
    app_id: Option<String>,
    state: State<'_, DesktopState>,
) -> Result<LaunchResult, String> {
    state
        .service
        .open_project_in_engine(Path::new(&root), app_id.as_deref())
        .await
        .map_err(|error| error.to_string())
}

/// Version choices for opening a project in its (primary or given) engine.
#[tauri::command(rename_all = "camelCase")]
async fn engine_options(
    root: String,
    app_id: Option<String>,
    state: State<'_, DesktopState>,
) -> Result<Option<EngineChoice>, String> {
    state
        .service
        .engine_options(Path::new(&root), app_id.as_deref())
        .await
        .map_err(|error| error.to_string())
}

/// Saves a linked app's preferred version into the portable project config.
#[tauri::command(rename_all = "camelCase")]
async fn set_project_engine_version(
    root: String,
    app_id: String,
    version: String,
    state: State<'_, DesktopState>,
) -> Result<(), String> {
    state
        .service
        .set_project_engine_version(Path::new(&root), &app_id, &version)
        .await
        .map_err(|error| error.to_string())
}

fn require_confirmation(confirmed: bool) -> Result<(), String> {
    confirmed
        .then_some(())
        .ok_or_else(|| "confirmation is required for this repository mutation".into())
}

#[tauri::command(rename_all = "camelCase")]
async fn git_sync(
    root: String,
    confirmed: bool,
    state: State<'_, DesktopState>,
) -> Result<VcsOperationResult, String> {
    require_confirmation(confirmed)?;
    state
        .service
        .vcs_sync(Path::new(&root), confirmed)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn git_commit(
    root: String,
    message: String,
    confirmed: bool,
    state: State<'_, DesktopState>,
) -> Result<VcsOperationResult, String> {
    require_confirmation(confirmed)?;
    state
        .service
        .vcs_commit(Path::new(&root), &message, confirmed)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn git_push(
    root: String,
    confirmed: bool,
    state: State<'_, DesktopState>,
) -> Result<VcsOperationResult, String> {
    require_confirmation(confirmed)?;
    state
        .service
        .vcs_push(Path::new(&root), confirmed)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn git_switch(
    root: String,
    branch: String,
    confirmed: bool,
    state: State<'_, DesktopState>,
) -> Result<VcsOperationResult, String> {
    require_confirmation(confirmed)?;
    state
        .service
        .vcs_switch_branch(Path::new(&root), &branch, confirmed)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn git_branches(root: String, state: State<'_, DesktopState>) -> Result<Vec<String>, String> {
    state
        .service
        .vcs_branches(Path::new(&root))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
async fn git_create_branch(
    root: String,
    branch: String,
    confirmed: bool,
    state: State<'_, DesktopState>,
) -> Result<VcsOperationResult, String> {
    require_confirmation(confirmed)?;
    state
        .service
        .vcs_create_branch(Path::new(&root), &branch, confirmed)
        .await
        .map_err(|e| e.to_string())
}

/// The running application's version (from tauri.conf.json), shown in the UI.
#[tauri::command]
async fn app_version(app: tauri::AppHandle) -> Result<String, String> {
    Ok(app.package_info().version.to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateInfo {
    available: bool,
    current_version: String,
    version: String,
    notes: Option<String>,
}

/// Checks the configured release feed for a newer signed build. Returns update
/// metadata when one is available so the UI can inform the user.
#[tauri::command]
async fn check_for_update(app: tauri::AppHandle) -> Result<UpdateInfo, String> {
    let current_version = app.package_info().version.to_string();
    let up_to_date = UpdateInfo {
        available: false,
        version: current_version.clone(),
        current_version: current_version.clone(),
        notes: None,
    };
    // No published release feed yet (or it's unreachable) is a normal state, not
    // an error — treat it as "you're on the latest version" rather than surfacing
    // a scary "could not fetch release JSON" message.
    let Ok(updater) = app.updater() else {
        return Ok(up_to_date);
    };
    match updater.check().await {
        Ok(Some(update)) => Ok(UpdateInfo {
            available: true,
            current_version,
            version: update.version.clone(),
            notes: update.body.clone(),
        }),
        Ok(None) | Err(_) => Ok(up_to_date),
    }
}

/// Downloads and installs the available update, then restarts the application.
#[tauri::command]
async fn install_update(app: tauri::AppHandle) -> Result<(), String> {
    let updater = app.updater().map_err(|e| e.to_string())?;
    let Some(update) = updater.check().await.map_err(|e| e.to_string())? else {
        return Err("No update is available.".into());
    };
    update
        .download_and_install(|_downloaded, _total| {}, || {})
        .await
        .map_err(|e| e.to_string())?;
    app.restart();
}

/// Prevents Windows from raising blocking hard-error dialogs (such as
/// "machine type mismatch") for child processes the launcher starts.
#[cfg(windows)]
fn suppress_windows_error_dialogs() {
    unsafe extern "system" {
        fn SetErrorMode(mode: u32) -> u32;
    }
    // SEM_FAILCRITICALERRORS | SEM_NOGPFAULTERRORBOX | SEM_NOOPENFILEERRORBOX
    const FLAGS: u32 = 0x0001 | 0x0002 | 0x8000;
    unsafe {
        let previous = SetErrorMode(FLAGS);
        SetErrorMode(previous | FLAGS);
    }
}

#[cfg(not(windows))]
fn suppress_windows_error_dialogs() {}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PathInfo {
    exists: bool,
    is_dir: bool,
    is_file: bool,
}

/// Reports whether a (pasted) path exists and whether it is a directory or file,
/// so the UI can validate manually entered paths.
#[tauri::command(rename_all = "camelCase")]
async fn path_info(path: String) -> Result<PathInfo, String> {
    let path = PathBuf::from(path);
    Ok(PathInfo {
        exists: path.exists(),
        is_dir: path.is_dir(),
        is_file: path.is_file(),
    })
}

#[tauri::command(rename_all = "camelCase")]
async fn project_config(
    root: String,
    state: State<'_, DesktopState>,
) -> Result<ProjectConfig, String> {
    state
        .service
        .project_config(Path::new(&root))
        .await
        .map_err(|e| e.to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RecentFile {
    path: String,
    name: String,
    modified: u64,
}

// Directories that never hold meaningful "recent files" for creative projects
// (engine caches, build output, dependencies) — pruned for signal and speed.
fn is_pruned_project_dir(name: &str) -> bool {
    matches!(
        name,
        ".git"
            | "node_modules"
            | "Library"
            | "Temp"
            | "Logs"
            | "obj"
            | "bin"
            | ".vs"
            | "Intermediate"
            | "Saved"
            | "Binaries"
            | "DerivedDataCache"
            | "__pycache__"
    )
}

fn collect_recent(dir: &Path, depth: usize, out: &mut Vec<(PathBuf, std::time::SystemTime)>) {
    if depth == 0 || out.len() > 5000 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if file_type.is_dir() && !is_pruned_project_dir(name.as_ref()) {
            collect_recent(&entry.path(), depth - 1, out);
        } else if file_type.is_file()
            && let Ok(modified) = entry.metadata().and_then(|meta| meta.modified())
        {
            out.push((entry.path(), modified));
        }
    }
}

/// Most-recently-modified files in a project, generic across engines.
#[tauri::command(rename_all = "camelCase")]
async fn recent_files(root: String, limit: usize) -> Result<Vec<RecentFile>, String> {
    let root = PathBuf::from(root);
    if !root.is_dir() {
        return Ok(Vec::new());
    }
    let files = tauri::async_runtime::spawn_blocking(move || {
        let mut out = Vec::new();
        collect_recent(&root, 4, &mut out);
        out.sort_by_key(|(_, modified)| std::cmp::Reverse(*modified));
        out.into_iter()
            .take(limit.clamp(1, 100))
            .map(|(path, modified)| RecentFile {
                name: path
                    .file_name()
                    .map(|value| value.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                modified: modified
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0),
                path: path.display().to_string(),
            })
            .collect::<Vec<_>>()
    })
    .await
    .map_err(|e| e.to_string())?;
    Ok(files)
}

/// Reads an image file as a data URL (for custom project thumbnails).
#[tauri::command(rename_all = "camelCase")]
async fn read_image(path: String) -> Result<Option<String>, String> {
    let file = PathBuf::from(&path);
    if !file.is_file() {
        return Ok(None);
    }
    let mime = match file
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("bmp") => "image/bmp",
        _ => "image/png",
    };
    let bytes = tauri::async_runtime::spawn_blocking(move || std::fs::read(&file).ok())
        .await
        .map_err(|e| e.to_string())?;
    let Some(bytes) = bytes else { return Ok(None) };
    if bytes.len() > 16 * 1024 * 1024 {
        return Err("Image is too large (max 16 MB).".into());
    }
    use base64::Engine;
    Ok(Some(format!(
        "data:{};base64,{}",
        mime,
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )))
}

/// Reads and validates tool manifests from a local folder (a user tool source).
#[tauri::command(rename_all = "camelCase")]
async fn read_tools_from_dir(path: String) -> Result<Vec<ToolManifest>, String> {
    let dir = PathBuf::from(path);
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut tools = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let file = entry.path();
            if file.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            if let Ok(text) = std::fs::read_to_string(&file)
                && let Ok(tool) = ToolManifest::from_json(&text)
            {
                tools.push(tool);
            }
        }
    }
    Ok(tools)
}

fn open_with_os(path: &str) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("cmd")
            .args(["/C", "start", "", path])
            .creation_flags(0x0800_0000)
            .spawn()?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(path).spawn()?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open").arg(path).spawn()?;
    }
    Ok(())
}

/// Opens a file or folder with the operating system's default handler.
#[tauri::command(rename_all = "camelCase")]
async fn open_path(path: String) -> Result<(), String> {
    if !Path::new(&path).exists() {
        return Err("Path does not exist.".into());
    }
    open_with_os(&path).map_err(|e| e.to_string())
}

/// Resolves the `git` executable: prefers a standard Git-for-Windows install
/// location (so a freshly winget-installed Git works without restarting), then
/// falls back to whatever is on `PATH`.
fn resolve_git_binary() -> PathBuf {
    #[cfg(windows)]
    for candidate in [
        r"C:\Program Files\Git\cmd\git.exe",
        r"C:\Program Files\Git\bin\git.exe",
        r"C:\Program Files (x86)\Git\cmd\git.exe",
    ] {
        if Path::new(candidate).is_file() {
            return PathBuf::from(candidate);
        }
    }
    PathBuf::from("git")
}

/// Whether Git is available on this machine.
#[tauri::command]
async fn git_available(state: State<'_, DesktopState>) -> Result<bool, String> {
    Ok(state.service.git_available().await)
}

/// Initializes a Git repository for a project, optionally adding `remote` and
/// pushing. Remote auth is handled by the user's Git credential helper.
#[tauri::command(rename_all = "camelCase")]
async fn git_init_repo(
    root: String,
    remote: Option<String>,
    confirmed: bool,
    state: State<'_, DesktopState>,
) -> Result<VcsOperationResult, String> {
    require_confirmation(confirmed)?;
    state
        .service
        .git_init_repo(Path::new(&root), remote.as_deref(), confirmed)
        .await
        .map_err(|e| e.to_string())
}

/// Installs Git via winget (Windows). Returns a message; the app must be
/// restarted afterwards so the new install is picked up reliably.
#[tauri::command]
async fn install_git() -> Result<String, String> {
    #[cfg(windows)]
    {
        let output = tauri::async_runtime::spawn_blocking(|| {
            use std::os::windows::process::CommandExt;
            std::process::Command::new("winget")
                .args([
                    "install",
                    "--id",
                    "Git.Git",
                    "-e",
                    "--accept-source-agreements",
                    "--accept-package-agreements",
                ])
                .creation_flags(0x0800_0000)
                .output()
        })
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| {
            format!("Could not run winget: {e}. Install Git from https://git-scm.com/download/win")
        })?;
        if output.status.success() {
            Ok("Git installed. Restart Pipeline OS, then continue setting up Git.".into())
        } else {
            Err(format!(
                "winget could not install Git: {}. You can install it from https://git-scm.com/download/win",
                String::from_utf8_lossy(&output.stderr).trim()
            ))
        }
    }
    #[cfg(not(windows))]
    {
        Err("Automatic install is Windows-only. Install Git with your package manager.".into())
    }
}

/// Opens an http(s) URL in the user's default browser. The WebView's
/// `window.open` is unreliable, so external links route through the OS opener.
#[tauri::command(rename_all = "camelCase")]
async fn open_url(url: String) -> Result<(), String> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err("Only http(s) links can be opened.".into());
    }
    open_with_os(&url).map_err(|e| e.to_string())
}

/// Launches an arbitrary user-provided executable (custom apps not in the
/// bundled catalog), with the same architecture safety as catalog launches.
#[tauri::command(rename_all = "camelCase")]
async fn launch_executable(executable: String) -> Result<(), String> {
    let exe = PathBuf::from(&executable);
    if !exe.is_file() {
        return Err("Executable not found.".into());
    }
    vantadeck_launcher::ensure_runnable(&exe).map_err(|e| e.to_string())?;
    let working_directory = exe
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    LaunchSpec::new(exe, vec![], working_directory)
        .map_err(|e| e.to_string())?
        .command()
        .spawn()
        .map_err(|e| format!("Could not start the application: {e}"))?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    suppress_windows_error_dialogs();
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::Builder::new().build())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            let storage = tauri::async_runtime::block_on(Storage::connect_path(
                &data_dir.join("vantadeck.sqlite3"),
            ))?;
            let resource_manifest_dir = app.path().resource_dir()?.join("manifests/apps");
            let manifest_dir = if resource_manifest_dir.is_dir() {
                resource_manifest_dir
            } else {
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../manifests/apps")
            };
            app.manage(DesktopState {
                service: ApplicationService::new(storage, GitProvider::new(resolve_git_binary())),
                manifest_dir,
                scan_cancel: Arc::new(AtomicBool::new(false)),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            dashboard_snapshot,
            list_projects,
            import_project,
            remove_project,
            set_project_tags,
            set_project_category,
            read_project_workspace,
            save_project_workspace,
            recent_activity,
            project_health,
            cached_health,
            health_overview,
            refresh_health,
            rename_project,
            record_project_opened,
            set_project_thumbnail,
            clear_project_thumbnail,
            git_commit_files,
            git_status,
            git_available,
            git_init_repo,
            install_git,
            list_apps,
            scan_apps,
            set_manual_override,
            cancel_scan,
            launch_app,
            app_icon,
            list_tools,
            set_project_pinned,
            launch_project_profile,
            open_in_engine,
            engine_options,
            set_project_engine_version,
            check_for_update,
            install_update,
            path_info,
            project_config,
            recent_files,
            read_image,
            read_tools_from_dir,
            open_path,
            open_url,
            launch_executable,
            git_sync,
            git_commit,
            git_push,
            git_switch,
            git_branches,
            git_create_branch,
            git_log,
            git_diff,
            git_discard,
            git_commit_paths,
            app_version
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Vantadeck desktop application");
}

#[cfg(test)]
mod tests {
    use super::*;
    use vantadeck_domain::AppState;

    #[test]
    fn launch_requires_an_exact_detected_executable() {
        let installations = vec![AppInstallation {
            version: "1.2.3".parse().unwrap(),
            executable: PathBuf::from("C:/Tools/editor.exe"),
            state: AppState::Installed,
            evidence: vec![],
        }];
        assert!(launch_is_allowed(
            &installations,
            Path::new("C:/Tools/editor.exe")
        ));
        assert!(!launch_is_allowed(
            &installations,
            Path::new("C:/Other/editor.exe")
        ));
    }
}
