use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;
use tauri::{Emitter, Manager, State};
use tauri_plugin_updater::UpdaterExt;
use vantadeck_application::{
    AppSummary, ApplicationService, HealthSummary, LaunchResult, ProjectSummary,
};
use vantadeck_domain::{
    AppCategory, AppInstallation, DetectedApplication, HealthIssue, ProjectConfig,
};
use vantadeck_launcher::LaunchSpec;
use vantadeck_manifests::{AppManifest, ToolManifest};
use vantadeck_storage::Storage;
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
    changed_files: Vec<DesktopChangedFile>,
}

#[derive(Serialize)]
struct DesktopChangedFile {
    path: String,
    status: String,
}

fn project_summary(name: String, root: &Path) -> ProjectSummary {
    ProjectSummary {
        name,
        path: root.display().to_string(),
        engine: "Registered project".into(),
        version: "Local".into(),
        branch: "-".into(),
        last_opened: "Registered".into(),
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
    let summaries = projects
        .iter()
        .map(|p| project_summary(p.name.clone(), &p.root))
        .collect::<Vec<_>>();
    let detected = apps(&state.service, &state.manifest_dir).await?;
    let health = if let Some(project) = projects.first() {
        state
            .service
            .project_health_quick(&project.root)
            .await
            .into_iter()
            .map(|issue| HealthSummary {
                code: issue.code,
                title: issue.title,
                detail: issue.detail,
                severity: format!("{:?}", issue.severity).to_lowercase(),
            })
            .collect()
    } else {
        Vec::new()
    };
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
    Ok(state
        .service
        .registered_projects()
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|p| DesktopProject {
            name: p.name,
            path: p.root.display().to_string(),
            pinned: p.pinned,
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
    let updater = app.updater().map_err(|e| e.to_string())?;
    match updater.check().await.map_err(|e| e.to_string())? {
        Some(update) => Ok(UpdateInfo {
            available: true,
            current_version,
            version: update.version.clone(),
            notes: update.body.clone(),
        }),
        None => Ok(UpdateInfo {
            available: false,
            version: current_version.clone(),
            current_version,
            notes: None,
        }),
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
                service: ApplicationService::new(storage, GitProvider::new("git")),
                manifest_dir,
                scan_cancel: Arc::new(AtomicBool::new(false)),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            dashboard_snapshot,
            list_projects,
            import_project,
            project_health,
            git_status,
            list_apps,
            scan_apps,
            set_manual_override,
            cancel_scan,
            launch_app,
            app_icon,
            list_tools,
            set_project_pinned,
            launch_project_profile,
            check_for_update,
            install_update,
            path_info,
            project_config,
            recent_files,
            open_path,
            launch_executable,
            git_sync,
            git_commit,
            git_push,
            git_switch
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
