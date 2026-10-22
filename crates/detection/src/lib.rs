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
                    || !is_executable_file(entry.path())
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
    // macOS app bundles are detected from their Info.plist by the bundle
    // source; files inside other apps' bundles (a "gimp" palette folder in
    // some editor's Resources) are never installations of their own.
    if cfg!(target_os = "macos") && entry.depth() > 0 && has_app_extension(entry.path()) {
        return true;
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

fn has_app_extension(path: &std::path::Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("app"))
}

/// A regular file the OS can execute. On Unix that means an execute bit;
/// Windows decides by extension, which the manifest's names already encode.
pub fn is_executable_file(path: &std::path::Path) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

/// Turns what a user picked into something launchable: a macOS `.app` bundle
/// resolves to its declared executable (with the bundle's version); an
/// executable file is used as is, with a version from `--version` or its path.
pub fn resolve_launch_target(path: &std::path::Path) -> Option<(PathBuf, Option<Version>)> {
    if path.is_dir() && has_app_extension(path) {
        let metadata = unix_sources::read_info_plist(&path.join("Contents/Info.plist"))?;
        let executable = path.join("Contents/MacOS").join(&metadata.executable);
        let version = (metadata.version != Version::new(0, 0, 0)).then_some(metadata.version);
        return is_executable_file(&executable).then_some((executable, version));
    }
    if !is_executable_file(path) {
        return None;
    }
    let version = unix_sources::cli_version(path).or_else(|| infer_version(path));
    Some((path.to_path_buf(), version))
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
        .or_else(|| unreal_build_version(path))
        .or_else(|| windows_product_version(path))
}

/// Unreal Engine installs carry `Engine/Build/Build.version` (JSON with
/// MajorVersion/MinorVersion/PatchVersion), whatever their folder is called.
fn unreal_build_version(executable: &std::path::Path) -> Option<Version> {
    let engine = executable
        .ancestors()
        .find(|ancestor| ancestor.file_name().is_some_and(|name| name == "Engine"))?;
    let raw = std::fs::read_to_string(engine.join("Build/Build.version")).ok()?;
    let json: serde_json::Value = serde_json::from_str(&raw).ok()?;
    let part = |key: &str| json.get(key).and_then(serde_json::Value::as_u64);
    Some(Version::new(
        part("MajorVersion")?,
        part("MinorVersion")?,
        part("PatchVersion").unwrap_or(0),
    ))
}

/// The ProductVersion (or FileVersion) string from a Windows executable's
/// version resource. Resources sit near the end of the file, so only the
/// tail is read.
pub fn windows_product_version(executable: &std::path::Path) -> Option<Version> {
    use std::io::{Read, Seek, SeekFrom};
    if !executable
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
    {
        return None;
    }
    let mut file = std::fs::File::open(executable).ok()?;
    let length = file.metadata().ok()?.len();
    let window = length.min(8 * 1024 * 1024);
    file.seek(SeekFrom::Start(length - window)).ok()?;
    let mut bytes = Vec::with_capacity(window as usize);
    file.take(window).read_to_end(&mut bytes).ok()?;
    ["ProductVersion", "FileVersion"]
        .into_iter()
        .find_map(|key| version_resource_value(&bytes, key))
}

/// Finds `key` (UTF-16LE, NUL-terminated) in a VS_VERSIONINFO block and
/// parses the string value that follows it.
fn version_resource_value(bytes: &[u8], key: &str) -> Option<Version> {
    let needle: Vec<u8> = key
        .encode_utf16()
        .chain(std::iter::once(0))
        .flat_map(u16::to_le_bytes)
        .collect();
    let mut start = 0;
    while let Some(offset) = bytes[start..]
        .windows(needle.len())
        .position(|window| window == needle)
    {
        let mut index = start + offset + needle.len();
        // Values are 32-bit aligned; skip padding.
        while index + 1 < bytes.len() && bytes[index] == 0 && bytes[index + 1] == 0 {
            index += 2;
        }
        let mut units = Vec::new();
        while index + 1 < bytes.len() && units.len() < 64 {
            let unit = u16::from_le_bytes([bytes[index], bytes[index + 1]]);
            if unit == 0 {
                break;
            }
            units.push(unit);
            index += 2;
        }
        let text = String::from_utf16_lossy(&units);
        let cleaned: String = text.trim().replace(", ", ".").replace(',', ".");
        if let Some(version) = cleaned
            .split(|c: char| !c.is_ascii_digit() && c != '.')
            .find(|candidate| candidate.contains('.'))
            .and_then(|candidate| {
                let parts: Vec<&str> = candidate.trim_matches('.').split('.').collect();
                let number = |i: usize| parts.get(i).and_then(|p| p.parse::<u64>().ok());
                Some(Version::new(
                    number(0)?,
                    number(1).unwrap_or(0),
                    number(2).unwrap_or(0),
                ))
            })
            .filter(|version| *version != Version::new(0, 0, 0))
        {
            return Some(version);
        }
        start += offset + needle.len();
    }
    None
}

#[cfg(test)]
mod version_tests {
    use super::*;

    #[test]
    fn reads_a_version_resource_value() {
        let mut bytes = b"MZ....".to_vec();
        for unit in "ProductVersion".encode_utf16().chain([0, 0]) {
            bytes.extend(unit.to_le_bytes());
        }
        for unit in "6000.0.23f1".encode_utf16().chain([0]) {
            bytes.extend(unit.to_le_bytes());
        }
        assert_eq!(
            version_resource_value(&bytes, "ProductVersion"),
            Some(Version::new(6000, 0, 23))
        );
    }

    #[test]
    fn reads_unreal_build_version() {
        let root = tempfile::tempdir().expect("root");
        let engine = root.path().join("UE/Engine");
        std::fs::create_dir_all(engine.join("Build")).expect("build");
        std::fs::create_dir_all(engine.join("Binaries/Win64")).expect("bin");
        std::fs::write(
            engine.join("Build/Build.version"),
            r#"{"MajorVersion":5,"MinorVersion":4,"PatchVersion":2}"#,
        )
        .expect("version");
        assert_eq!(
            unreal_build_version(&engine.join("Binaries/Win64/UnrealEditor.exe")),
            Some(Version::new(5, 4, 2))
        );
    }
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
