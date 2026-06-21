use std::{
    collections::HashMap,
    ffi::OsStr,
    fs, io,
    path::{Path, PathBuf},
};

use async_trait::async_trait;
use semver::Version;
use vantadeck_domain::{AppInstallation, AppState, DetectionEvidence};
use walkdir::WalkDir;

use crate::{DetectionSource, ScanRequest, infer_version};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacBundleMetadata {
    pub name: Option<String>,
    pub identifier: Option<String>,
    pub executable: String,
    pub version: Version,
}

pub fn parse_info_plist(input: &str) -> Option<MacBundleMetadata> {
    let executable = plist_string(input, "CFBundleExecutable")?;
    let version = plist_string(input, "CFBundleShortVersionString")
        .or_else(|| plist_string(input, "CFBundleVersion"))
        .and_then(|version| parse_loose_version(&version))
        .unwrap_or_else(unknown_version);
    Some(MacBundleMetadata {
        name: plist_string(input, "CFBundleDisplayName")
            .or_else(|| plist_string(input, "CFBundleName")),
        identifier: plist_string(input, "CFBundleIdentifier"),
        executable,
        version,
    })
}

pub struct MacAppBundleDetectionSource {
    roots: Vec<PathBuf>,
    max_depth: usize,
}

impl MacAppBundleDetectionSource {
    pub fn new(roots: Vec<PathBuf>, max_depth: usize) -> Self {
        Self { roots, max_depth }
    }

    #[cfg(target_os = "macos")]
    pub fn system() -> Self {
        let mut roots = vec![
            PathBuf::from("/Applications"),
            PathBuf::from("/System/Applications"),
        ];
        if let Some(home) = home_directory() {
            roots.push(home.join("Applications"));
        }
        Self::new(roots, 3)
    }
}

#[async_trait]
impl DetectionSource for MacAppBundleDetectionSource {
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
                .filter_map(Result::ok)
                .filter(|entry| entry.file_type().is_dir() && has_extension(entry.path(), "app"))
            {
                let bundle = entry.path();
                let plist_path = bundle.join("Contents/Info.plist");
                let metadata = fs::read_to_string(&plist_path)
                    .ok()
                    .and_then(|input| parse_info_plist(&input));
                let bundle_name = bundle.file_stem().and_then(OsStr::to_str);
                if !metadata
                    .as_ref()
                    .and_then(|metadata| metadata.name.as_deref())
                    .or(bundle_name)
                    .is_some_and(|name| request_name_matches(name, request))
                {
                    continue;
                }
                let declared = metadata
                    .as_ref()
                    .map(|metadata| metadata.executable.as_str());
                let executable_name = declared
                    .filter(|name| {
                        request
                            .executables
                            .iter()
                            .any(|candidate| candidate == name)
                    })
                    .or_else(|| request.executables.first().map(String::as_str));
                let Some(executable) = executable_name
                    .map(|name| bundle.join("Contents/MacOS").join(name))
                    .filter(|path| path.is_file())
                else {
                    continue;
                };
                let version = metadata
                    .map(|metadata| metadata.version)
                    .or_else(|| infer_version(bundle))
                    .unwrap_or_else(unknown_version);
                found.push(installation(
                    version,
                    executable,
                    "macos-app-bundle",
                    plist_path.display().to_string(),
                    95,
                ));
            }
        }
        Ok(found)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopEntry {
    pub name: Option<String>,
    pub exec: Vec<String>,
    pub try_exec: Option<String>,
    pub flatpak_id: Option<String>,
    pub snap_instance: Option<String>,
}

pub fn parse_desktop_entry(input: &str) -> Option<DesktopEntry> {
    let fields = parse_ini_section(input, "Desktop Entry");
    let exec = tokenize_exec(fields.get("Exec")?)
        .into_iter()
        .filter(|argument| !is_desktop_field_code(argument))
        .collect::<Vec<_>>();
    if exec.is_empty() {
        return None;
    }
    Some(DesktopEntry {
        name: fields.get("Name").cloned(),
        exec,
        try_exec: fields.get("TryExec").cloned(),
        flatpak_id: fields.get("X-Flatpak").cloned(),
        snap_instance: fields.get("X-SnapInstanceName").cloned(),
    })
}

pub struct DesktopEntryDetectionSource {
    roots: Vec<PathBuf>,
    search_paths: Vec<PathBuf>,
}

impl DesktopEntryDetectionSource {
    pub fn new(roots: Vec<PathBuf>, search_paths: Vec<PathBuf>) -> Self {
        Self {
            roots,
            search_paths,
        }
    }

    #[cfg(target_os = "linux")]
    pub fn system() -> Self {
        Self::new(linux_desktop_roots(), executable_search_paths())
    }
}

#[async_trait]
impl DetectionSource for DesktopEntryDetectionSource {
    async fn scan(&self, request: &ScanRequest) -> io::Result<Vec<AppInstallation>> {
        scan_desktop_roots(
            &self.roots,
            &self.search_paths,
            request,
            "linux-desktop-entry",
            80,
            |_| true,
        )
    }
}

pub struct ExecutablePathDetectionSource {
    search_paths: Vec<PathBuf>,
}

impl ExecutablePathDetectionSource {
    pub fn new(search_paths: Vec<PathBuf>) -> Self {
        Self { search_paths }
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    pub fn system() -> Self {
        let mut paths = executable_search_paths();
        for common in ["/usr/local/bin", "/usr/bin", "/bin"] {
            paths.push(common.into());
        }
        paths.sort();
        paths.dedup();
        Self::new(paths)
    }
}

#[async_trait]
impl DetectionSource for ExecutablePathDetectionSource {
    async fn scan(&self, request: &ScanRequest) -> io::Result<Vec<AppInstallation>> {
        let mut found = Vec::new();
        for directory in &self.search_paths {
            for executable_name in &request.executables {
                let executable = directory.join(executable_name);
                if executable.is_file() {
                    found.push(installation(
                        infer_version(&executable).unwrap_or_else(unknown_version),
                        executable,
                        "executable-path",
                        directory.display().to_string(),
                        85,
                    ));
                }
            }
        }
        Ok(found)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlatpakMetadata {
    pub app_id: String,
    pub command: String,
    pub branch: Option<String>,
}

pub fn parse_flatpak_metadata(input: &str) -> Option<FlatpakMetadata> {
    let application = parse_ini_section(input, "Application");
    let instance = parse_ini_section(input, "Instance");
    Some(FlatpakMetadata {
        app_id: application.get("name")?.clone(),
        command: application.get("command")?.clone(),
        branch: instance.get("branch").cloned(),
    })
}

pub struct FlatpakDetectionSource {
    app_roots: Vec<PathBuf>,
    export_roots: Vec<PathBuf>,
    search_paths: Vec<PathBuf>,
}

impl FlatpakDetectionSource {
    pub fn new(app_roots: Vec<PathBuf>, export_roots: Vec<PathBuf>) -> Self {
        Self {
            app_roots,
            export_roots,
            search_paths: Vec::new(),
        }
    }

    pub fn with_search_paths(mut self, search_paths: Vec<PathBuf>) -> Self {
        self.search_paths = search_paths;
        self
    }

    #[cfg(target_os = "linux")]
    pub fn system() -> Self {
        let mut app_roots = vec![PathBuf::from("/var/lib/flatpak/app")];
        let mut export_roots = vec![PathBuf::from("/var/lib/flatpak/exports/share/applications")];
        if let Some(home) = home_directory() {
            let flatpak = home.join(".local/share/flatpak");
            app_roots.push(flatpak.join("app"));
            export_roots.push(flatpak.join("exports/share/applications"));
        }
        Self::new(app_roots, export_roots).with_search_paths(executable_search_paths())
    }
}

#[async_trait]
impl DetectionSource for FlatpakDetectionSource {
    async fn scan(&self, request: &ScanRequest) -> io::Result<Vec<AppInstallation>> {
        let mut found = Vec::new();
        for root in &self.app_roots {
            if !root.is_dir() {
                continue;
            }
            for entry in WalkDir::new(root)
                .max_depth(8)
                .follow_links(false)
                .into_iter()
                .filter_map(Result::ok)
                .filter(|entry| entry.file_type().is_file() && entry.file_name() == "metadata")
            {
                let Ok(input) = fs::read_to_string(entry.path()) else {
                    continue;
                };
                let Some(metadata) = parse_flatpak_metadata(&input) else {
                    continue;
                };
                if !request_name_matches(&metadata.app_id, request) {
                    continue;
                }
                let Some(deployment) = entry.path().parent() else {
                    continue;
                };
                let executable = deployment.join("files/bin").join(&metadata.command);
                if executable.is_file()
                    && request
                        .executables
                        .iter()
                        .any(|name| file_name_matches(&executable, name))
                {
                    found.push(installation(
                        infer_version(deployment).unwrap_or_else(unknown_version),
                        executable,
                        "flatpak-metadata",
                        entry.path().display().to_string(),
                        90,
                    ));
                }
            }
        }
        found.extend(scan_desktop_roots(
            &self.export_roots,
            &self.search_paths,
            request,
            "flatpak-export",
            85,
            |entry| entry.flatpak_id.is_some(),
        )?);
        Ok(found)
    }
}

pub struct SnapDetectionSource {
    desktop_roots: Vec<PathBuf>,
    search_paths: Vec<PathBuf>,
}

impl SnapDetectionSource {
    pub fn new(desktop_roots: Vec<PathBuf>, search_paths: Vec<PathBuf>) -> Self {
        Self {
            desktop_roots,
            search_paths,
        }
    }

    #[cfg(target_os = "linux")]
    pub fn system() -> Self {
        let mut paths = executable_search_paths();
        paths.push(PathBuf::from("/snap/bin"));
        Self::new(
            vec![PathBuf::from("/var/lib/snapd/desktop/applications")],
            paths,
        )
    }
}

#[async_trait]
impl DetectionSource for SnapDetectionSource {
    async fn scan(&self, request: &ScanRequest) -> io::Result<Vec<AppInstallation>> {
        scan_desktop_roots(
            &self.desktop_roots,
            &self.search_paths,
            request,
            "snap-desktop-entry",
            85,
            |entry| entry.snap_instance.is_some(),
        )
    }
}

pub struct AppImageDetectionSource {
    roots: Vec<PathBuf>,
    max_depth: usize,
}

impl AppImageDetectionSource {
    pub fn new(roots: Vec<PathBuf>, max_depth: usize) -> Self {
        Self { roots, max_depth }
    }

    #[cfg(target_os = "linux")]
    pub fn system() -> Self {
        let mut roots = executable_search_paths();
        if let Some(home) = home_directory() {
            roots.push(home.join("Applications"));
            roots.push(home.join(".local/bin"));
        }
        Self::new(roots, 2)
    }
}

#[async_trait]
impl DetectionSource for AppImageDetectionSource {
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
                .filter_map(Result::ok)
                .filter(|entry| {
                    entry.file_type().is_file() && has_extension(entry.path(), "appimage")
                })
            {
                let executable = entry.into_path();
                let Some(name) = executable.file_stem().and_then(OsStr::to_str) else {
                    continue;
                };
                if !request_name_matches(name, request)
                    && !request
                        .executables
                        .iter()
                        .any(|candidate| contains_ignore_ascii_case(name, candidate))
                {
                    continue;
                }
                found.push(installation(
                    infer_version(&executable).unwrap_or_else(unknown_version),
                    executable.clone(),
                    "appimage",
                    executable.display().to_string(),
                    75,
                ));
            }
        }
        Ok(found)
    }
}

fn scan_desktop_roots(
    roots: &[PathBuf],
    search_paths: &[PathBuf],
    request: &ScanRequest,
    source: &str,
    confidence: u8,
    accepts: impl Fn(&DesktopEntry) -> bool,
) -> io::Result<Vec<AppInstallation>> {
    let mut found = Vec::new();
    for root in roots {
        if !root.is_dir() {
            continue;
        }
        for entry in WalkDir::new(root)
            .max_depth(3)
            .follow_links(false)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_file() && has_extension(entry.path(), "desktop"))
        {
            let Ok(input) = fs::read_to_string(entry.path()) else {
                continue;
            };
            let Some(desktop) = parse_desktop_entry(&input) else {
                continue;
            };
            if !accepts(&desktop)
                || !desktop
                    .name
                    .as_deref()
                    .is_some_and(|name| request_name_matches(name, request))
                    && !desktop
                        .flatpak_id
                        .as_deref()
                        .is_some_and(|name| request_name_matches(name, request))
                    && !desktop
                        .snap_instance
                        .as_deref()
                        .is_some_and(|name| request_name_matches(name, request))
            {
                continue;
            }
            let Some(executable) = resolve_desktop_executable(&desktop, search_paths) else {
                continue;
            };
            if !request
                .executables
                .iter()
                .any(|name| file_name_matches(&executable, name))
                && source != "flatpak-export"
            {
                continue;
            }
            found.push(installation(
                infer_version(&executable).unwrap_or_else(unknown_version),
                executable,
                source,
                entry.path().display().to_string(),
                confidence,
            ));
        }
    }
    Ok(found)
}

fn resolve_desktop_executable(entry: &DesktopEntry, search_paths: &[PathBuf]) -> Option<PathBuf> {
    let command = desktop_command(&entry.exec).or(entry.try_exec.as_deref())?;
    let path = PathBuf::from(command);
    if path.is_absolute() {
        return path.is_file().then_some(path);
    }
    search_paths
        .iter()
        .map(|directory| directory.join(command))
        .find(|candidate| candidate.is_file())
}

fn desktop_command(arguments: &[String]) -> Option<&str> {
    let mut index = usize::from(arguments.first().is_some_and(|value| value == "env"));
    while arguments
        .get(index)
        .is_some_and(|value| value.contains('=') && !value.starts_with('/'))
    {
        index += 1;
    }
    arguments.get(index).map(String::as_str)
}

fn tokenize_exec(input: &str) -> Vec<String> {
    let mut arguments = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    let mut escaped = false;
    for character in input.chars() {
        if escaped {
            current.push(character);
            escaped = false;
            continue;
        }
        if character == '\\' {
            escaped = true;
            continue;
        }
        if let Some(expected) = quote {
            if character == expected {
                quote = None;
            } else {
                current.push(character);
            }
            continue;
        }
        if character == '"' || character == '\'' {
            quote = Some(character);
        } else if character.is_whitespace() {
            if !current.is_empty() {
                arguments.push(std::mem::take(&mut current));
            }
        } else {
            current.push(character);
        }
    }
    if escaped {
        current.push('\\');
    }
    if !current.is_empty() {
        arguments.push(current);
    }
    arguments
}

fn is_desktop_field_code(argument: &str) -> bool {
    argument.len() == 2
        && argument.starts_with('%')
        && argument
            .chars()
            .nth(1)
            .is_some_and(|character| character.is_ascii_alphabetic())
}

fn parse_ini_section(input: &str, requested_section: &str) -> HashMap<String, String> {
    let mut fields = HashMap::new();
    let mut active = false;
    for line in input.lines() {
        let line = line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            active = &line[1..line.len() - 1] == requested_section;
            continue;
        }
        if !active || line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            fields.insert(key.trim().to_owned(), value.trim().to_owned());
        }
    }
    fields
}

fn plist_string(input: &str, requested_key: &str) -> Option<String> {
    let key_marker = format!("<key>{requested_key}</key>");
    let after_key = input.split_once(&key_marker)?.1;
    let after_open = after_key.split_once("<string>")?.1;
    let value = after_open.split_once("</string>")?.0.trim();
    Some(unescape_xml(value))
}

fn unescape_xml(value: &str) -> String {
    value
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

fn parse_loose_version(input: &str) -> Option<Version> {
    for candidate in input.split(|character: char| !character.is_ascii_digit() && character != '.')
    {
        let candidate = candidate.trim_matches('.');
        if candidate.is_empty() {
            continue;
        }
        let normalized = match candidate.split('.').count() {
            1 => format!("{candidate}.0.0"),
            2 => format!("{candidate}.0"),
            _ => candidate.to_owned(),
        };
        if let Ok(version) = Version::parse(&normalized) {
            return Some(version);
        }
    }
    None
}

fn request_name_matches(value: &str, request: &ScanRequest) -> bool {
    contains_ignore_ascii_case(value, &request.display_name)
        || contains_ignore_ascii_case(value, &request.app_id)
}

fn contains_ignore_ascii_case(value: &str, needle: &str) -> bool {
    !needle.is_empty()
        && value
            .to_ascii_lowercase()
            .contains(&needle.to_ascii_lowercase())
}

fn file_name_matches(path: &Path, expected: &str) -> bool {
    path.file_name()
        .and_then(OsStr::to_str)
        .is_some_and(|name| name.eq_ignore_ascii_case(expected))
}

fn has_extension(path: &Path, expected: &str) -> bool {
    path.extension()
        .and_then(OsStr::to_str)
        .is_some_and(|extension| extension.eq_ignore_ascii_case(expected))
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn executable_search_paths() -> Vec<PathBuf> {
    std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect())
        .unwrap_or_default()
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn home_directory() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

#[cfg(target_os = "linux")]
fn linux_desktop_roots() -> Vec<PathBuf> {
    let mut roots = vec![
        PathBuf::from("/usr/share/applications"),
        PathBuf::from("/usr/local/share/applications"),
    ];
    if let Some(data_home) = std::env::var_os("XDG_DATA_HOME") {
        roots.push(PathBuf::from(data_home).join("applications"));
    } else if let Some(home) = home_directory() {
        roots.push(home.join(".local/share/applications"));
    }
    roots
}

fn unknown_version() -> Version {
    Version::new(0, 0, 0)
}

fn installation(
    version: Version,
    executable: PathBuf,
    source: &str,
    detail: String,
    confidence: u8,
) -> AppInstallation {
    AppInstallation {
        version,
        executable,
        state: AppState::NewDetected,
        evidence: vec![DetectionEvidence {
            source: source.into(),
            detail,
            confidence,
        }],
    }
}
