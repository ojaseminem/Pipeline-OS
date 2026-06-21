use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::{Manager, State};
use vantadeck_application::{
    AppSummary, ApplicationService, HealthSummary, LaunchResult, ProjectSummary,
};
use vantadeck_domain::{AppInstallation, DetectedApplication, HealthIssue, ProjectConfig};
use vantadeck_launcher::LaunchSpec;
use vantadeck_manifests::ToolManifest;
use vantadeck_storage::Storage;
use vantadeck_vcs::GitProvider;
use vantadeck_vcs::VcsOperationResult;

const APP_CATALOG: &[(&str, &str)] = &[
    ("blender", "Blender"),
    ("git", "Git"),
    ("git-lfs", "Git LFS"),
    ("godot", "Godot"),
    ("perforce", "Perforce CLI"),
    ("rider", "JetBrains Rider"),
    ("unity", "Unity"),
    ("unreal-engine", "Unreal Engine"),
    ("vscode", "Visual Studio Code"),
];
const TOOL_INDEX_SOURCE: &str = "https://tools.vantadeck.org/v1/index.json";

struct DesktopState {
    service: ApplicationService,
    manifest_dir: PathBuf,
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
struct DesktopApp {
    id: String,
    name: String,
    installations: Vec<AppInstallation>,
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

async fn apps(service: &ApplicationService) -> Result<Vec<DesktopApp>, String> {
    let mut result = Vec::with_capacity(APP_CATALOG.len());
    for (id, name) in APP_CATALOG {
        result.push(DesktopApp {
            id: (*id).into(),
            name: (*name).into(),
            installations: service
                .detected_installations(id)
                .await
                .map_err(|e| e.to_string())?,
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
    let detected = apps(&state.service).await?;
    let health = if let Some(project) = projects.first() {
        state
            .service
            .project_health(&project.root)
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
            .filter(|app| !app.installations.is_empty())
            .map(|app| AppSummary {
                id: app.id,
                name: app.name,
                versions: app
                    .installations
                    .into_iter()
                    .map(|i| i.version.to_string())
                    .collect(),
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
    apps(&state.service).await
}

#[tauri::command]
async fn scan_apps(
    roots: Vec<String>,
    state: State<'_, DesktopState>,
) -> Result<Vec<DetectedApplication>, String> {
    let roots = roots.into_iter().map(PathBuf::from).collect::<Vec<_>>();
    state
        .service
        .scan_apps(&state.manifest_dir, &roots)
        .await
        .map_err(|e| e.to_string())
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

#[tauri::command(rename_all = "camelCase")]
async fn launch_app(
    app_id: String,
    executable: String,
    state: State<'_, DesktopState>,
) -> Result<(), String> {
    let executable = PathBuf::from(executable);
    let installations = state
        .service
        .detected_installations(&app_id)
        .await
        .map_err(|e| e.to_string())?;
    if !launch_is_allowed(&installations, &executable) || !executable.is_file() {
        return Err("launch denied: executable is not a detected installation".into());
    }
    let working_directory = executable
        .parent()
        .ok_or("launch denied: executable has no parent directory")?
        .to_path_buf();
    LaunchSpec::new(executable, vec![], working_directory)
        .map_err(|e| e.to_string())?
        .command()
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
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
            launch_app,
            list_tools,
            set_project_pinned,
            launch_project_profile,
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
