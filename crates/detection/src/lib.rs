use std::{io, path::PathBuf};

use async_trait::async_trait;
use semver::Version;
use vantadeck_domain::{AppCategory, DetectedApplication};
use vantadeck_domain::{AppInstallation, AppState, DetectionEvidence};
use walkdir::WalkDir;

mod unix_sources;
mod windows_sources;

pub use unix_sources::{
    AppImageDetectionSource, DesktopEntry, DesktopEntryDetectionSource,
    ExecutablePathDetectionSource, FlatpakDetectionSource, FlatpakMetadata,
    MacAppBundleDetectionSource, MacBundleMetadata, SnapDetectionSource, parse_desktop_entry,
    parse_flatpak_metadata, parse_info_plist, read_info_plist,
};

pub use windows_sources::{
    EpicLauncherDetectionSource, EpicManifestRecord, RegistryDetectionSource,
    ShortcutDetectionSource, SteamAppRecord, SteamDetectionSource, UninstallRegistryRecord,
    UnityEditorRecord, UnityHubDetectionSource, parse_epic_manifest, parse_registry_query,
    parse_shortcut_target, parse_steam_app_manifest, parse_steam_library_folders,
    parse_unity_hub_editors,
};

#[derive(Debug, Clone)]
pub struct ScanRequest {
    pub app_id: String,
    pub display_name: String,
    pub executables: Vec<String>,
    /// Glob patterns of well-known install locations from the app manifest. May
    /// contain `*` wildcards and a `{user}` placeholder.
    pub known_paths: Vec<String>,
    /// macOS bundle identifier prefixes (e.g. `com.unity3d.UnityEditor`).
    /// When present, macOS bundles are matched by identifier only.
    pub mac_bundle_ids: Vec<String>,
}

impl ScanRequest {
    pub fn new(
        app_id: impl Into<String>,
        display_name: impl Into<String>,
        executables: Vec<impl Into<String>>,
    ) -> Self {
        Self {
            app_id: app_id.into(),
            display_name: display_name.into(),
            executables: executables.into_iter().map(Into::into).collect(),
            known_paths: Vec::new(),
            mac_bundle_ids: Vec::new(),
        }
    }

    pub fn with_known_paths(mut self, known_paths: Vec<impl Into<String>>) -> Self {
        self.known_paths = known_paths.into_iter().map(Into::into).collect();
        self
    }

    pub fn with_mac_bundle_ids(mut self, ids: Vec<impl Into<String>>) -> Self {
        self.mac_bundle_ids = ids.into_iter().map(Into::into).collect();
        self
    }
}

#[async_trait]
pub trait DetectionSource: Send + Sync {
    async fn scan(&self, request: &ScanRequest) -> io::Result<Vec<AppInstallation>>;
}

/// Detects installations from the manifest's declared well-known paths,
/// expanding `*` globs and the `{user}` placeholder. This finds editors that
/// install into versioned hub directories (Unity Hub, Epic UE_*, JetBrains) on
/// any drive, which a bounded directory walk would otherwise miss.
pub struct KnownPathDetectionSource {
    source_id: String,
    confidence: u8,
}

impl KnownPathDetectionSource {
    pub fn new(source_id: impl Into<String>, confidence: u8) -> Self {
        Self {
            source_id: source_id.into(),
            confidence: confidence.min(100),
        }
    }
}

#[async_trait]
impl DetectionSource for KnownPathDetectionSource {
    async fn scan(&self, request: &ScanRequest) -> io::Result<Vec<AppInstallation>> {
        let mut found = Vec::new();
        let user = std::env::var("USERNAME")
            .or_else(|_| std::env::var("USER"))
            .unwrap_or_default();
        for pattern in &request.known_paths {
            let expanded = pattern.replace("{user}", &user);
            let Ok(entries) = glob::glob(&expanded) else {
                continue;
            };
            for entry in entries.flatten() {
                // A known path may name a macOS bundle (".../Unity.app"):
                // resolve it to the executable inside.
                let entry = if entry.is_dir()
                    && entry.extension().and_then(|e| e.to_str()) == Some("app")
                {
                    let metadata =
                        unix_sources::read_info_plist(&entry.join("Contents/Info.plist"));
                    match unix_sources::bundle_executable(&entry, metadata.as_ref(), request) {
                        Some(executable) => executable,
                        None => continue,
                    }
                } else {
                    entry
                };
                if entry.is_file() {
                    found.push(AppInstallation {
                        version: infer_version(&entry).unwrap_or_else(|| Version::new(0, 0, 0)),
                        executable: entry.clone(),
                        state: AppState::NewDetected,
                        evidence: vec![DetectionEvidence {
                            source: self.source_id.clone(),
                            detail: entry.display().to_string(),
                            confidence: self.confidence,
                        }],
                    });
                }
            }
        }
        Ok(found)
    }
}

pub struct PathDetectionSource {
    source_id: String,
    confidence: u8,
    roots: Vec<PathBuf>,
}

impl PathDetectionSource {
    pub fn new(source_id: impl Into<String>, confidence: u8, roots: Vec<PathBuf>) -> Self {
        Self {
            source_id: source_id.into(),
            confidence: confidence.min(100),
            roots,
        }
    }
}

#[async_trait]
impl DetectionSource for PathDetectionSource {
    async fn scan(&self, request: &ScanRequest) -> io::Result<Vec<AppInstallation>> {
        let mut found = Vec::new();
        for root in &self.roots {
            for executable_name in &request.executables {
                let executable = root.join(executable_name);
                if executable.is_file() {
                    found.push(AppInstallation {
                        version: Version::new(0, 0, 0),
                        executable: executable.clone(),
                        state: AppState::NewDetected,
                        evidence: vec![DetectionEvidence {
                            source: self.source_id.clone(),
                            detail: executable.display().to_string(),
                            confidence: self.confidence,
                        }],
                    });
                }
            }
        }
        Ok(found)
    }
}

pub struct FilesystemDetectionSource {
    source_id: String,
    confidence: u8,
    roots: Vec<PathBuf>,
    max_depth: usize,
}

impl FilesystemDetectionSource {
    pub fn new(
        source_id: impl Into<String>,
        confidence: u8,
        roots: Vec<PathBuf>,
        max_depth: usize,
    ) -> Self {
        Self {
            source_id: source_id.into(),
            confidence: confidence.min(100),
            roots,
            max_depth,
        }
    }
}

#[async_trait]
impl DetectionSource for FilesystemDetectionSource {
    async fn scan(&self, request: &ScanRequest) -> io::Result<Vec<AppInstallation>> {
        let mut found = Vec::new();
        for root in &self.roots {
            if !root.is_dir() {
                continue;
            }
            for entry in WalkDir::new(root)
                .max_depth(self.max_depth)
                .follow_links(false)
                .into_iter()
                .filter_entry(|entry| !is_pruned_directory(entry))
                .filter_map(Result::ok)
                .filter(|entry| entry.file_type().is_file())
            {
                let name = entry.file_name().to_string_lossy();
                if !request
                    .executables
                    .iter()
                    .any(|candidate| name.eq_ignore_ascii_case(candidate))
                {
                    continue;
                }
                let executable = entry.into_path();
                found.push(AppInstallation {
                    version: infer_version(&executable).unwrap_or_else(|| Version::new(0, 0, 0)),
                    executable: executable.clone(),
                    state: AppState::NewDetected,
                    evidence: vec![DetectionEvidence {
                        source: self.source_id.clone(),
                        detail: executable.display().to_string(),
                        confidence: self.confidence,
                    }],
                });
            }
        }
        Ok(found)
    }
}

/// Transient, system, or dependency directories that never contain real
/// creative-app installations and would otherwise produce phantom detections
/// (e.g. test fixtures unpacked in %TEMP%) or slow the scan dramatically.
fn is_pruned_directory(entry: &walkdir::DirEntry) -> bool {
    if !entry.file_type().is_dir() {
        return false;
    }
    const PRUNED: &[&str] = &[
        "Temp",
        "tmp",
        "Cache",
        "Caches",
        "$Recycle.Bin",
        "node_modules",
        ".git",
        "WinSxS",
        "Windows.old",
        "Installer",
    ];
    let name = entry.file_name().to_string_lossy();
    PRUNED.iter().any(|skip| name.eq_ignore_ascii_case(skip))
}

#[derive(Debug, Clone)]
pub struct ManualOverride {
    pub version: Version,
    pub executable: PathBuf,
}

pub struct DetectionEngine {
    sources: Vec<Box<dyn DetectionSource>>,
}

impl DetectionEngine {
    pub fn new(sources: Vec<Box<dyn DetectionSource>>) -> Self {
        Self { sources }
    }

    pub async fn scan_app(
        &self,
        request: &ScanRequest,
        category: AppCategory,
        manual_override: Option<ManualOverride>,
    ) -> io::Result<DetectedApplication> {
        let mut application =
            DetectedApplication::new(&request.app_id, &request.display_name, category);
        for source in &self.sources {
            for installation in source.scan(request).await? {
                application.add_installation(installation);
            }
        }
        if let Some(manual_override) = manual_override {
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
        Ok(application)
    }
}

pub(crate) fn infer_version(path: &std::path::Path) -> Option<Version> {
    // Inside a macOS bundle the Info.plist is authoritative ("Blender.app"
    // carries no version in its path, so path parsing would give 0.0.0).
    if let Some(version) = mac_bundle_version(path) {
        return Some(version);
    }
    path.ancestors()
        .filter_map(|ancestor| ancestor.file_name())
        .filter_map(|name| {
            name.to_string_lossy()
                .split(|character: char| !character.is_ascii_digit() && character != '.')
                .filter(|candidate| !candidate.is_empty())
                .filter_map(parse_version_candidate)
                .max()
        })
        .max()
}

/// Version from the Info.plist of the nearest enclosing `.app` bundle.
fn mac_bundle_version(path: &std::path::Path) -> Option<Version> {
    // Follow symlinks (Homebrew's /opt/homebrew/bin/code points into the app).
    let resolved = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let bundle = resolved.ancestors().find(|ancestor| {
        ancestor
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("app"))
    })?;
    let metadata = unix_sources::read_info_plist(&bundle.join("Contents/Info.plist"))?;
    (metadata.version != Version::new(0, 0, 0)).then_some(metadata.version)
}

fn parse_version_candidate(candidate: &str) -> Option<Version> {
    let candidate = candidate.trim_matches('.');
    let dot_count = candidate.bytes().filter(|byte| *byte == b'.').count();
    match dot_count {
        // A bare integer is only treated as a version when it's a plausible
        // year-style release (Maya 2024, 3ds Max 2025, ZBrush 2024…). This keeps
        // stray numbers like the "86" in "Program Files (x86)" from matching.
        0 => candidate
            .parse::<u64>()
            .ok()
            .filter(|year| (1990..=2100).contains(year))
            .map(|year| Version::new(year, 0, 0)),
        1 => Version::parse(&format!("{candidate}.0")).ok(),
        2 => Version::parse(candidate).ok(),
        _ => None,
    }
}
