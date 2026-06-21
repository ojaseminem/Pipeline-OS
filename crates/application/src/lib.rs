use std::{
    fs, io,
    path::{Path, PathBuf},
};

use chrono::Utc;
use semver::Version;
use serde::{Deserialize, Serialize};
use thiserror::Error;
#[cfg(target_os = "macos")]
use vantadeck_detection::MacAppBundleDetectionSource;
#[cfg(target_os = "linux")]
use vantadeck_detection::{
    AppImageDetectionSource, DesktopEntryDetectionSource, ExecutablePathDetectionSource,
    FlatpakDetectionSource, SnapDetectionSource,
};
use vantadeck_detection::{
    DetectionEngine, DetectionSource, FilesystemDetectionSource, ScanRequest,
};
#[cfg(windows)]
use vantadeck_detection::{
    EpicLauncherDetectionSource, RegistryDetectionSource, ShortcutDetectionSource,
    SteamDetectionSource, UnityHubDetectionSource,
};
use vantadeck_domain::{
    AppInstallation, AppState, DetectedApplication, DetectionEvidence, HealthIssue, HealthSeverity,
    ProjectConfig,
};
use vantadeck_health::{HealthCheck, ProjectPathCheck};
use vantadeck_launcher::{LaunchError, LaunchSpec, resolve_launch_profile};
use vantadeck_manifests::{AppManifest, ManifestError, ToolManifest};
use vantadeck_projects::{ProjectError, import_project, load_project};
use vantadeck_storage::{
    ActivityRecord, ManualOverrideRecord, RegisteredProject, Storage, StorageError,
};
use vantadeck_vcs::{
    GitProvider, VcsError, VcsOperationResult, VcsStatus, VersionControlProvider,
    evaluate_lfs_health,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardSnapshot {
    pub network_enabled: bool,
    pub continue_project: ProjectSummary,
    pub pinned_projects: Vec<ProjectSummary>,
    pub recent_projects: Vec<ProjectSummary>,
    pub apps: Vec<AppSummary>,
    pub health: Vec<HealthSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSummary {
    pub name: String,
    pub path: String,
    pub engine: String,
    pub version: String,
    pub branch: String,
    pub last_opened: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSummary {
    pub id: String,
    pub name: String,
    pub category: String,
    pub versions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthSummary {
    pub code: String,
    pub title: String,
    pub detail: String,
    pub severity: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchResult {
    pub process_id: Option<u32>,
    pub executable: String,
}

impl DashboardSnapshot {
    pub fn demo() -> Self {
        let project = |name: &str, engine: &str, version: &str, branch: &str| ProjectSummary {
            name: name.into(),
            path: format!("D:/Dev/Projects/{name}"),
            engine: engine.into(),
            version: version.into(),
            branch: branch.into(),
            last_opened: "Today".into(),
        };
        Self {
            network_enabled: false,
            continue_project: project("Voidline", "Unity", "2022.3.18f1", "main"),
            pinned_projects: vec![
                project("Emberfall", "Unreal Engine", "5.3.2", "develop"),
                project("Project Helix", "Unity", "2022.3.18f1", "main"),
                project("Mech Bay", "Unreal Engine", "5.2.1", "main"),
                project("Northbreak", "Unity", "2020.3.48f1", "release"),
                project("Riftbound", "Unreal Engine", "5.3", "main"),
            ],
            recent_projects: vec![project("Atlas Prototype", "Godot", "4.3", "prototype")],
            apps: vec![
                AppSummary {
                    id: "blender".into(),
                    name: "Blender".into(),
                    category: "dcc".into(),
                    versions: vec!["4.1.1 LTS".into(), "3.6.5 LTS".into()],
                },
                AppSummary {
                    id: "unreal-engine".into(),
                    name: "Unreal Engine".into(),
                    category: "game-engine".into(),
                    versions: vec!["5.3.2".into(), "5.2.1".into()],
                },
                AppSummary {
                    id: "unity".into(),
                    name: "Unity".into(),
                    category: "game-engine".into(),
                    versions: vec!["2022.3.18f1".into()],
                },
                AppSummary {
                    id: "vscode".into(),
                    name: "Visual Studio Code".into(),
                    category: "code".into(),
                    versions: vec!["1.87.2".into()],
                },
            ],
            health: vec![HealthSummary {
                code: "APP_VERSION_MISSING".into(),
                title: "Preferred version missing".into(),
                detail: "Install the preferred version or select a fallback.".into(),
                severity: "warning".into(),
            }],
        }
    }
}

pub struct ApplicationService {
    storage: Storage,
    git: GitProvider,
}

#[derive(Debug, Error)]
pub enum ApplicationError {
    #[error("application storage failed: {0}")]
    Storage(#[from] StorageError),
    #[error("manifest operation failed: {0}")]
    Manifest(#[from] ManifestError),
    #[error("project operation failed: {0}")]
    Project(#[from] ProjectError),
    #[error("version-control operation failed: {0}")]
    Vcs(#[from] VcsError),
    #[error("file operation failed: {0}")]
    Io(#[from] io::Error),
    #[error("JSON operation failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("manual override executable does not exist: {0}")]
    OverrideExecutableMissing(PathBuf),
    #[error("launch profile `{0}` does not exist")]
    LaunchProfileMissing(String),
    #[error("launch profile could not be resolved: {0}")]
    Launch(#[from] LaunchError),
    #[error("explicit confirmation is required for {0}")]
    ConfirmationRequired(&'static str),
}

impl ApplicationService {
    pub fn new(storage: Storage, git: GitProvider) -> Self {
        Self { storage, git }
    }

    pub async fn set_manual_override(
        &self,
        app_id: &str,
        version: Version,
        executable: &Path,
    ) -> Result<(), ApplicationError> {
        if !executable.is_file() {
            return Err(ApplicationError::OverrideExecutableMissing(
                executable.to_path_buf(),
            ));
        }
        self.storage
            .set_manual_override(&ManualOverrideRecord {
                app_id: app_id.into(),
                version,
                executable: executable.to_path_buf(),
            })
            .await?;
        self.storage
            .record_activity("manual-override", &format!("Updated override for {app_id}"))
            .await?;
        Ok(())
    }

    pub async fn scan_apps(
        &self,
        manifest_dir: &Path,
        roots: &[PathBuf],
    ) -> Result<Vec<DetectedApplication>, ApplicationError> {
        let mut manifest_paths = fs::read_dir(manifest_dir)?
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("json"))
            .collect::<Vec<_>>();
        manifest_paths.sort();
        let mut applications = Vec::new();
        for path in manifest_paths {
            let manifest = AppManifest::from_json(&fs::read_to_string(path)?)?;
            let engine = system_detection_engine(roots);
            let mut application = engine
                .scan_app(
                    &ScanRequest::new(
                        &manifest.id,
                        &manifest.name,
                        manifest.executables.iter().map(String::as_str).collect(),
                    ),
                    manifest.category,
                    None,
                )
                .await?;
            for manual_override in self.storage.manual_overrides(&manifest.id).await? {
                application
                    .installations
                    .retain(|installation| installation.version != manual_override.version);
                application.add_installation(AppInstallation {
                    version: manual_override.version,
                    executable: manual_override.executable.clone(),
                    state: AppState::ManuallyOverridden,
                    evidence: vec![DetectionEvidence {
                        source: "manual-override".into(),
                        detail: manual_override.executable.display().to_string(),
                        confidence: 100,
                    }],
                });
            }
            self.storage
                .replace_detected_installations(&application.id, &application.installations)
                .await?;
            if !application.installations.is_empty() {
                applications.push(application);
            }
        }
        self.storage
            .record_activity(
                "app-scan",
                &format!("Detected {} application families", applications.len()),
            )
            .await?;
        Ok(applications)
    }

    pub async fn detected_installations(
        &self,
        app_id: &str,
    ) -> Result<Vec<AppInstallation>, ApplicationError> {
        Ok(self.storage.detected_installations(app_id).await?)
    }

    pub async fn import_project(
        &self,
        root: &Path,
        name: Option<&str>,
    ) -> Result<ProjectConfig, ApplicationError> {
        let project = import_project(root, name)?;
        self.storage
            .register_project(&RegisteredProject {
                root: root.to_path_buf(),
                name: project.name.clone(),
                pinned: false,
            })
            .await?;
        self.storage
            .record_activity("project-import", &format!("Imported {}", project.name))
            .await?;
        Ok(project)
    }

    pub async fn registered_projects(&self) -> Result<Vec<RegisteredProject>, ApplicationError> {
        Ok(self.storage.registered_projects().await?)
    }

    pub async fn search_projects(
        &self,
        query: &str,
        limit: u32,
    ) -> Result<Vec<RegisteredProject>, ApplicationError> {
        Ok(self.storage.search_projects(query, limit).await?)
    }

    pub async fn set_project_pinned(
        &self,
        root: &Path,
        pinned: bool,
    ) -> Result<(), ApplicationError> {
        self.storage.set_project_pinned(root, pinned).await?;
        self.storage
            .record_activity(
                "project-pin",
                &format!(
                    "{} {}",
                    if pinned { "Pinned" } else { "Unpinned" },
                    root.display()
                ),
            )
            .await?;
        Ok(())
    }

    pub async fn resolve_project_launch(
        &self,
        root: &Path,
        profile_id: &str,
    ) -> Result<LaunchSpec, ApplicationError> {
        let config = load_project(root)?;
        let profile = config
            .launch_profiles
            .iter()
            .find(|profile| profile.id == profile_id)
            .ok_or_else(|| ApplicationError::LaunchProfileMissing(profile_id.into()))?;
        let mut installations = self.storage.detected_installations(&profile.app_id).await?;
        for manual_override in self.storage.manual_overrides(&profile.app_id).await? {
            installations.retain(|item| item.version != manual_override.version);
            installations.push(AppInstallation {
                version: manual_override.version,
                executable: manual_override.executable,
                state: AppState::ManuallyOverridden,
                evidence: vec![DetectionEvidence {
                    source: "manual-override".into(),
                    detail: "Machine-local project launch override".into(),
                    confidence: 100,
                }],
            });
        }
        Ok(resolve_launch_profile(root, profile, &installations)?)
    }

    pub async fn launch_project_profile(
        &self,
        root: &Path,
        profile_id: &str,
    ) -> Result<LaunchResult, ApplicationError> {
        let spec = self.resolve_project_launch(root, profile_id).await?;
        let executable = spec.executable.display().to_string();
        let child = spec.command().spawn()?;
        self.storage
            .record_activity(
                "project-launch",
                &format!("Launched profile {profile_id} for {}", root.display()),
            )
            .await?;
        Ok(LaunchResult {
            process_id: child.id(),
            executable,
        })
    }

    pub async fn recent_activity(
        &self,
        limit: u32,
    ) -> Result<Vec<ActivityRecord>, ApplicationError> {
        Ok(self.storage.recent_activity(limit).await?)
    }

    pub async fn cache_tool_index(
        &self,
        source_url: &str,
        content: &str,
        etag: Option<&str>,
    ) -> Result<Vec<ToolManifest>, ApplicationError> {
        let entries: Vec<serde_json::Value> = serde_json::from_str(content)?;
        let tools = entries
            .into_iter()
            .map(|entry| ToolManifest::from_json(&entry.to_string()))
            .collect::<Result<Vec<_>, _>>()?;
        self.storage
            .cache_tool_index(source_url, content, etag)
            .await?;
        self.storage
            .record_activity(
                "tool-index-refresh",
                &format!("Cached {} validated tool entries", tools.len()),
            )
            .await?;
        Ok(tools)
    }

    pub async fn cached_tools(
        &self,
        source_url: &str,
    ) -> Result<Vec<ToolManifest>, ApplicationError> {
        let Some(cache) = self.storage.cached_tool_index(source_url).await? else {
            return Ok(Vec::new());
        };
        let entries: Vec<serde_json::Value> = serde_json::from_str(&cache.content)?;
        entries
            .into_iter()
            .map(|entry| {
                ToolManifest::from_json(&entry.to_string()).map_err(ApplicationError::from)
            })
            .collect()
    }

    pub async fn vcs_status(&self, root: &Path) -> Result<VcsStatus, ApplicationError> {
        Ok(self.git.status(root).await?)
    }

    pub async fn vcs_sync(
        &self,
        root: &Path,
        confirmed: bool,
    ) -> Result<VcsOperationResult, ApplicationError> {
        require_confirmation("Git sync", confirmed)?;
        Ok(self.git.sync(root).await?)
    }

    pub async fn vcs_commit(
        &self,
        root: &Path,
        message: &str,
        confirmed: bool,
    ) -> Result<VcsOperationResult, ApplicationError> {
        require_confirmation("Git commit", confirmed)?;
        Ok(self.git.commit_all(root, message).await?)
    }

    pub async fn vcs_push(
        &self,
        root: &Path,
        confirmed: bool,
    ) -> Result<VcsOperationResult, ApplicationError> {
        require_confirmation("Git push", confirmed)?;
        Ok(self.git.push(root).await?)
    }

    pub async fn vcs_switch_branch(
        &self,
        root: &Path,
        branch: &str,
        confirmed: bool,
    ) -> Result<VcsOperationResult, ApplicationError> {
        require_confirmation("Git branch switch", confirmed)?;
        Ok(self.git.switch_branch(root, branch).await?)
    }

    pub async fn project_health(&self, root: &Path) -> Vec<HealthIssue> {
        let mut issues = ProjectPathCheck.run(root).await;
        if !issues.is_empty() {
            return issues;
        }
        match load_project(root) {
            Ok(config) => {
                for linked_app in &config.linked_apps {
                    match self.local_installations(&linked_app.app_id).await {
                        Ok(installations) if installations.is_empty() => issues.push(HealthIssue {
                            code: "APP_NOT_INSTALLED".into(),
                            severity: HealthSeverity::Error,
                            title: format!("{} is not installed", linked_app.app_id),
                            detail: "No detected installation or manual override is available on this machine."
                                .into(),
                            remediation: Some(
                                "Scan applications or configure a machine-local override.".into(),
                            ),
                            checked_at: Utc::now(),
                        }),
                        Err(error) => issues.push(HealthIssue {
                            code: "APP_CHECK_FAILED".into(),
                            severity: HealthSeverity::Error,
                            title: format!("Could not inspect {}", linked_app.app_id),
                            detail: error.to_string(),
                            remediation: Some(
                                "Check the local Vantadeck database and retry.".into(),
                            ),
                            checked_at: Utc::now(),
                        }),
                        Ok(_) => {}
                    }
                }
                for profile in &config.launch_profiles {
                    if let Err(error) = self.resolve_project_launch(root, &profile.id).await {
                        issues.push(HealthIssue {
                            code: "LAUNCH_PROFILE_BROKEN".into(),
                            severity: HealthSeverity::Error,
                            title: format!("Launch profile '{}' is unavailable", profile.name),
                            detail: error.to_string(),
                            remediation: Some(
                                "Correct the profile path or configure a compatible application version."
                                    .into(),
                            ),
                            checked_at: Utc::now(),
                        });
                    }
                }
            }
            Err(error) => issues.push(HealthIssue {
                code: "PROJECT_CONFIG_INVALID".into(),
                severity: HealthSeverity::Error,
                title: "Project metadata is invalid".into(),
                detail: error.to_string(),
                remediation: Some(
                    "Repair .vantadeck/project.toml or restore it from source control.".into(),
                ),
                checked_at: Utc::now(),
            }),
        }
        if !self.git.detect(root).await {
            return issues;
        }
        match self.git.status(root).await {
            Ok(status) if !status.changed_files.is_empty() => issues.push(HealthIssue {
                code: "REPO_UNCOMMITTED_CHANGES".into(),
                severity: HealthSeverity::Warning,
                title: "Repository has uncommitted changes".into(),
                detail: format!("{} files have local changes", status.changed_files.len()),
                remediation: Some("Review and save or discard the local changes.".into()),
                checked_at: Utc::now(),
            }),
            Ok(_) => {}
            Err(error) => issues.push(HealthIssue {
                code: "GIT_STATUS_FAILED".into(),
                severity: HealthSeverity::Error,
                title: "Git status failed".into(),
                detail: error.to_string(),
                remediation: Some("Check Git installation and repository permissions.".into()),
                checked_at: Utc::now(),
            }),
        }
        let probe = self.git.lfs_probe(root, 50 * 1024 * 1024).await;
        issues.extend(evaluate_lfs_health(&probe));
        issues
    }

    async fn local_installations(
        &self,
        app_id: &str,
    ) -> Result<Vec<AppInstallation>, ApplicationError> {
        let mut installations = self.storage.detected_installations(app_id).await?;
        for manual_override in self.storage.manual_overrides(app_id).await? {
            installations.retain(|item| item.version != manual_override.version);
            installations.push(AppInstallation {
                version: manual_override.version,
                executable: manual_override.executable,
                state: AppState::ManuallyOverridden,
                evidence: vec![DetectionEvidence {
                    source: "manual-override".into(),
                    detail: "Machine-local override".into(),
                    confidence: 100,
                }],
            });
        }
        Ok(installations)
    }
}

fn require_confirmation(operation: &'static str, confirmed: bool) -> Result<(), ApplicationError> {
    confirmed
        .then_some(())
        .ok_or(ApplicationError::ConfirmationRequired(operation))
}

fn system_detection_engine(roots: &[PathBuf]) -> DetectionEngine {
    let include_system_sources = roots.is_empty();
    let filesystem_roots = if include_system_sources {
        default_detection_roots()
    } else {
        roots.to_vec()
    };
    let mut sources: Vec<Box<dyn DetectionSource>> = vec![Box::new(
        FilesystemDetectionSource::new("filesystem", 80, filesystem_roots, 6),
    )];
    #[cfg(windows)]
    if include_system_sources {
        sources.push(Box::new(RegistryDetectionSource::system()));
        if let Some(source) = UnityHubDetectionSource::system() {
            sources.push(Box::new(source));
        }
        if let Some(source) = EpicLauncherDetectionSource::system() {
            sources.push(Box::new(source));
        }
        if let Some(source) = SteamDetectionSource::system() {
            sources.push(Box::new(source));
        }
        sources.push(Box::new(ShortcutDetectionSource::system()));
    }
    #[cfg(target_os = "macos")]
    if include_system_sources {
        sources.push(Box::new(MacAppBundleDetectionSource::system()));
    }
    #[cfg(target_os = "linux")]
    if include_system_sources {
        sources.push(Box::new(DesktopEntryDetectionSource::system()));
        sources.push(Box::new(ExecutablePathDetectionSource::system()));
        sources.push(Box::new(FlatpakDetectionSource::system()));
        sources.push(Box::new(SnapDetectionSource::system()));
        sources.push(Box::new(AppImageDetectionSource::system()));
    }
    DetectionEngine::new(sources)
}

fn default_detection_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    // Standard, environment-provided locations (these may live on any drive).
    for variable in [
        "ProgramFiles",
        "ProgramFiles(x86)",
        "ProgramW6432",
        "LOCALAPPDATA",
        "APPDATA",
    ] {
        if let Some(value) = std::env::var_os(variable) {
            roots.push(PathBuf::from(value));
        }
    }
    // PATH entries frequently contain command-line tools (git, p4, etc.).
    if let Some(path) = std::env::var_os("PATH") {
        roots.extend(std::env::split_paths(&path));
    }
    roots.extend(platform_detection_roots());
    roots.sort();
    roots.dedup();
    roots.retain(|root| root.is_dir());
    roots
}

/// Curated application install folders on every available drive. Creative tools
/// are routinely installed off the system drive, so we probe well-known
/// subdirectories on each fixed drive rather than walking entire volumes.
#[cfg(windows)]
fn platform_detection_roots() -> Vec<PathBuf> {
    const COMMON_SUBDIRS: &[&str] = &[
        "Program Files",
        "Program Files (x86)",
        "Apps",
        "Applications",
        "Tools",
        "Games",
        "Epic Games",
        "Steam/steamapps/common",
        "Program Files/Epic Games",
        "Unity",
        "Unity/Hub/Editor",
        "Program Files/Unity/Hub/Editor",
    ];
    let mut roots = Vec::new();
    for letter in b'A'..=b'Z' {
        let drive = PathBuf::from(format!("{}:\\", letter as char));
        if !drive.is_dir() {
            continue;
        }
        for subdir in COMMON_SUBDIRS {
            roots.push(drive.join(subdir));
        }
    }
    roots
}

#[cfg(not(windows))]
fn platform_detection_roots() -> Vec<PathBuf> {
    let mut roots = vec![
        PathBuf::from("/opt"),
        PathBuf::from("/usr/local"),
        PathBuf::from("/Applications"),
    ];
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        roots.push(home.join("Applications"));
        roots.push(home.join(".local/bin"));
        roots.push(home.join(".local/share/applications"));
    }
    roots
}
