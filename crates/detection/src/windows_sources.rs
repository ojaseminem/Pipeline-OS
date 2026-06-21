use std::{
    fs, io,
    path::{Path, PathBuf},
};

use async_trait::async_trait;
use semver::{Prerelease, Version};
use serde_json::Value;
use vantadeck_domain::{AppInstallation, AppState, DetectionEvidence};
use walkdir::WalkDir;

use crate::{DetectionSource, ScanRequest, infer_version};

const UNKNOWN_VERSION: fn() -> Version = || Version::new(0, 0, 0);

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UninstallRegistryRecord {
    pub key: String,
    pub display_name: Option<String>,
    pub display_version: Option<String>,
    pub install_location: Option<PathBuf>,
    pub display_icon: Option<String>,
}

pub fn parse_registry_query(input: &str) -> Vec<UninstallRegistryRecord> {
    let mut records = Vec::new();
    let mut current: Option<UninstallRegistryRecord> = None;
    for line in input.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("HKEY_") {
            if let Some(record) = current.take() {
                records.push(record);
            }
            current = Some(UninstallRegistryRecord {
                key: trimmed.to_owned(),
                ..Default::default()
            });
            continue;
        }
        let Some(record) = current.as_mut() else {
            continue;
        };
        let mut fields = trimmed.split_whitespace();
        let Some(name) = fields.next() else { continue };
        let Some(kind) = fields.next() else { continue };
        if !kind.starts_with("REG_") {
            continue;
        }
        let value = fields.collect::<Vec<_>>().join(" ");
        if value.is_empty() {
            continue;
        }
        match name {
            "DisplayName" => record.display_name = Some(value),
            "DisplayVersion" => record.display_version = Some(value),
            "InstallLocation" => record.install_location = Some(value.into()),
            "DisplayIcon" => record.display_icon = Some(value),
            _ => {}
        }
    }
    if let Some(record) = current {
        records.push(record);
    }
    records
}

pub struct RegistryDetectionSource {
    records: Option<Vec<UninstallRegistryRecord>>,
}

impl RegistryDetectionSource {
    pub fn from_records(records: Vec<UninstallRegistryRecord>) -> Self {
        Self {
            records: Some(records),
        }
    }

    #[cfg(windows)]
    pub fn system() -> Self {
        // Query the registry once at construction and cache the result, so a
        // multi-app scan does not re-run `reg query` for every manifest.
        Self {
            records: Some(query_windows_uninstall_registry().unwrap_or_default()),
        }
    }

    fn records(&self) -> io::Result<Vec<UninstallRegistryRecord>> {
        if let Some(records) = &self.records {
            return Ok(records.clone());
        }
        #[cfg(windows)]
        {
            query_windows_uninstall_registry()
        }
        #[cfg(not(windows))]
        Ok(Vec::new())
    }
}

#[cfg(windows)]
fn query_windows_uninstall_registry() -> io::Result<Vec<UninstallRegistryRecord>> {
    use std::os::windows::process::CommandExt;
    use std::process::Command;
    // Run the helper without flashing a console window.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let reg_executable = std::env::var_os("WINDIR")
        .map(PathBuf::from)
        .map(|windows| windows.join("System32").join("reg.exe"))
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows\System32\reg.exe"));
    let keys = [
        r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
        r"HKCU\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
    ];
    let mut records = Vec::new();
    for key in keys {
        for view in ["/reg:64", "/reg:32"] {
            let output = Command::new(&reg_executable)
                .args(["query", key, "/s", view])
                .creation_flags(CREATE_NO_WINDOW)
                .output()?;
            if output.status.success() {
                records.extend(parse_registry_query(&String::from_utf8_lossy(
                    &output.stdout,
                )));
            }
        }
    }
    Ok(records)
}

#[async_trait]
impl DetectionSource for RegistryDetectionSource {
    async fn scan(&self, request: &ScanRequest) -> io::Result<Vec<AppInstallation>> {
        let mut found = Vec::new();
        for record in self.records()? {
            if !name_matches(record.display_name.as_deref(), request) {
                continue;
            }
            let executable = registry_executable(&record, request);
            let Some(executable) = executable.filter(|path| path.is_file()) else {
                continue;
            };
            found.push(installation(
                parse_loose_version(record.display_version.as_deref().unwrap_or(""))
                    .or_else(|| infer_version(&executable))
                    .unwrap_or_else(UNKNOWN_VERSION),
                executable,
                "windows-uninstall-registry",
                record.key,
                90,
            ));
        }
        Ok(found)
    }
}

fn registry_executable(record: &UninstallRegistryRecord, request: &ScanRequest) -> Option<PathBuf> {
    if let Some(icon) = &record.display_icon {
        let icon = icon
            .split(',')
            .next()
            .unwrap_or(icon)
            .trim()
            .trim_matches('"');
        let path = PathBuf::from(icon);
        if request
            .executables
            .iter()
            .any(|name| file_name_eq(&path, name))
        {
            return Some(path);
        }
    }
    record.install_location.as_ref().and_then(|location| {
        request
            .executables
            .iter()
            .map(|name| location.join(name))
            .find(|path| path.is_file())
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnityEditorRecord {
    pub version: Version,
    pub location: PathBuf,
}

pub fn parse_unity_hub_editors(input: &str) -> Result<Vec<UnityEditorRecord>, serde_json::Error> {
    let root: Value = serde_json::from_str(input)?;
    let mut records = Vec::new();
    let values: Vec<(&str, &Value)> = if let Some(data) = root.get("data").and_then(Value::as_array)
    {
        data.iter().map(|value| ("", value)).collect()
    } else if let Some(data) = root.as_array() {
        data.iter().map(|value| ("", value)).collect()
    } else {
        root.get("installed")
            .and_then(Value::as_object)
            .or_else(|| root.as_object())
            .into_iter()
            .flat_map(|object| object.iter().map(|(key, value)| (key.as_str(), value)))
            .collect()
    };
    for (key, value) in values {
        let Some(location) = value.get("location").and_then(Value::as_str) else {
            continue;
        };
        let raw_version = value.get("version").and_then(Value::as_str).unwrap_or(key);
        let Some(version) = parse_unity_version(raw_version) else {
            continue;
        };
        records.push(UnityEditorRecord {
            version,
            location: location.into(),
        });
    }
    records.sort_by(|left, right| left.version.cmp(&right.version));
    Ok(records)
}

pub struct UnityHubDetectionSource {
    editors_file: PathBuf,
}

impl UnityHubDetectionSource {
    pub fn new(editors_file: impl Into<PathBuf>) -> Self {
        Self {
            editors_file: editors_file.into(),
        }
    }

    #[cfg(windows)]
    pub fn system() -> Option<Self> {
        std::env::var_os("APPDATA").map(|app_data| {
            Self::new(
                PathBuf::from(app_data)
                    .join("UnityHub")
                    .join("editors-v2.json"),
            )
        })
    }
}

#[async_trait]
impl DetectionSource for UnityHubDetectionSource {
    async fn scan(&self, request: &ScanRequest) -> io::Result<Vec<AppInstallation>> {
        let Some(input) = read_optional(&self.editors_file)? else {
            return Ok(Vec::new());
        };
        let records = parse_unity_hub_editors(&input).map_err(invalid_data)?;
        Ok(records
            .into_iter()
            .flat_map(|record| {
                request.executables.iter().filter_map(move |name| {
                    let candidates = [
                        record.location.join("Editor").join(name),
                        record.location.join(name),
                    ];
                    candidates
                        .into_iter()
                        .find(|path| path.is_file())
                        .map(|path| {
                            installation(
                                record.version.clone(),
                                path,
                                "unity-hub",
                                self.editors_file.display().to_string(),
                                95,
                            )
                        })
                })
            })
            .collect())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EpicManifestRecord {
    pub app_name: Option<String>,
    pub display_name: Option<String>,
    pub version: Version,
    pub install_location: PathBuf,
    pub launch_executable: PathBuf,
}

pub fn parse_epic_manifest(input: &str) -> Result<EpicManifestRecord, serde_json::Error> {
    let value: Value = serde_json::from_str(input)?;
    Ok(EpicManifestRecord {
        app_name: json_string(&value, "AppName"),
        display_name: json_string(&value, "DisplayName"),
        version: parse_loose_version(
            value
                .get("AppVersion")
                .and_then(Value::as_str)
                .unwrap_or(""),
        )
        .unwrap_or_else(UNKNOWN_VERSION),
        install_location: value
            .get("InstallLocation")
            .and_then(Value::as_str)
            .unwrap_or("")
            .into(),
        launch_executable: value
            .get("LaunchExecutable")
            .and_then(Value::as_str)
            .unwrap_or("")
            .into(),
    })
}

pub struct EpicLauncherDetectionSource {
    manifest_directory: PathBuf,
}

impl EpicLauncherDetectionSource {
    pub fn new(manifest_directory: impl Into<PathBuf>) -> Self {
        Self {
            manifest_directory: manifest_directory.into(),
        }
    }

    #[cfg(windows)]
    pub fn system() -> Option<Self> {
        std::env::var_os("PROGRAMDATA").map(|program_data| {
            Self::new(
                PathBuf::from(program_data)
                    .join("Epic")
                    .join("EpicGamesLauncher")
                    .join("Data")
                    .join("Manifests"),
            )
        })
    }
}

#[async_trait]
impl DetectionSource for EpicLauncherDetectionSource {
    async fn scan(&self, request: &ScanRequest) -> io::Result<Vec<AppInstallation>> {
        let Ok(entries) = fs::read_dir(&self.manifest_directory) else {
            return Ok(Vec::new());
        };
        let mut found = Vec::new();
        for path in entries.filter_map(Result::ok).map(|entry| entry.path()) {
            if path.extension().and_then(|ext| ext.to_str()) != Some("item") {
                continue;
            }
            let Ok(input) = fs::read_to_string(&path) else {
                continue;
            };
            let Ok(record) = parse_epic_manifest(&input) else {
                continue;
            };
            if !name_matches(
                record
                    .display_name
                    .as_deref()
                    .or(record.app_name.as_deref()),
                request,
            ) {
                continue;
            }
            let declared = record.install_location.join(&record.launch_executable);
            let executable = if declared.is_file()
                && request
                    .executables
                    .iter()
                    .any(|name| file_name_eq(&declared, name))
            {
                Some(declared)
            } else {
                request
                    .executables
                    .iter()
                    .map(|name| record.install_location.join(name))
                    .find(|candidate| candidate.is_file())
            };
            if let Some(executable) = executable {
                found.push(installation(
                    record.version,
                    executable,
                    "epic-launcher",
                    path.display().to_string(),
                    95,
                ));
            }
        }
        Ok(found)
    }
}

pub fn parse_steam_library_folders(input: &str) -> Vec<PathBuf> {
    let tokens = vdf_tokens(input);
    tokens
        .windows(2)
        .filter(|pair| pair[0].eq_ignore_ascii_case("path"))
        .map(|pair| PathBuf::from(&pair[1]))
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SteamAppRecord {
    pub app_id: String,
    pub name: String,
    pub install_dir: String,
}

pub fn parse_steam_app_manifest(input: &str) -> Option<SteamAppRecord> {
    let tokens = vdf_tokens(input);
    let value = |key: &str| {
        tokens
            .windows(2)
            .find(|pair| pair[0].eq_ignore_ascii_case(key))
            .map(|pair| pair[1].clone())
    };
    Some(SteamAppRecord {
        app_id: value("appid")?,
        name: value("name")?,
        install_dir: value("installdir")?,
    })
}

pub struct SteamDetectionSource {
    library_folders_file: PathBuf,
}

impl SteamDetectionSource {
    pub fn new(library_folders_file: impl Into<PathBuf>) -> Self {
        Self {
            library_folders_file: library_folders_file.into(),
        }
    }

    #[cfg(windows)]
    pub fn system() -> Option<Self> {
        std::env::var_os("ProgramFiles(x86)")
            .or_else(|| std::env::var_os("ProgramFiles"))
            .map(|program_files| {
                Self::new(
                    PathBuf::from(program_files)
                        .join("Steam")
                        .join("steamapps")
                        .join("libraryfolders.vdf"),
                )
            })
    }
}

#[async_trait]
impl DetectionSource for SteamDetectionSource {
    async fn scan(&self, request: &ScanRequest) -> io::Result<Vec<AppInstallation>> {
        let Some(input) = read_optional(&self.library_folders_file)? else {
            return Ok(Vec::new());
        };
        let mut libraries = parse_steam_library_folders(&input);
        if let Some(steamapps) = self.library_folders_file.parent()
            && let Some(root) = steamapps.parent()
        {
            libraries.push(root.to_path_buf());
        }
        libraries.sort();
        libraries.dedup();
        let mut found = Vec::new();
        for library in libraries {
            let steamapps = library.join("steamapps");
            let Ok(entries) = fs::read_dir(&steamapps) else {
                continue;
            };
            for manifest in entries.filter_map(Result::ok).map(|entry| entry.path()) {
                let Some(name) = manifest.file_name().and_then(|name| name.to_str()) else {
                    continue;
                };
                if !name.starts_with("appmanifest_")
                    || manifest.extension().and_then(|ext| ext.to_str()) != Some("acf")
                {
                    continue;
                }
                let Ok(input) = fs::read_to_string(&manifest) else {
                    continue;
                };
                let Some(app) = parse_steam_app_manifest(&input) else {
                    continue;
                };
                if !app.name.eq_ignore_ascii_case(&request.display_name)
                    && !app.app_id.eq_ignore_ascii_case(&request.app_id)
                {
                    continue;
                }
                for executable_name in &request.executables {
                    let executable = steamapps
                        .join("common")
                        .join(&app.install_dir)
                        .join(executable_name);
                    if executable.is_file() {
                        found.push(installation(
                            infer_version(&executable).unwrap_or_else(UNKNOWN_VERSION),
                            executable,
                            "steam",
                            manifest.display().to_string(),
                            90,
                        ));
                    }
                }
            }
        }
        Ok(found)
    }
}

pub struct ShortcutDetectionSource {
    roots: Vec<PathBuf>,
    max_depth: usize,
}

impl ShortcutDetectionSource {
    pub fn new(roots: Vec<PathBuf>, max_depth: usize) -> Self {
        Self { roots, max_depth }
    }

    #[cfg(windows)]
    pub fn system() -> Self {
        let mut roots = Vec::new();
        if let Some(app_data) = std::env::var_os("APPDATA") {
            roots.push(
                PathBuf::from(app_data)
                    .join("Microsoft")
                    .join("Windows")
                    .join("Start Menu")
                    .join("Programs"),
            );
        }
        if let Some(program_data) = std::env::var_os("PROGRAMDATA") {
            roots.push(
                PathBuf::from(program_data)
                    .join("Microsoft")
                    .join("Windows")
                    .join("Start Menu")
                    .join("Programs"),
            );
        }
        if let Some(user_profile) = std::env::var_os("USERPROFILE") {
            roots.push(PathBuf::from(user_profile).join("Desktop"));
        }
        Self::new(roots, 8)
    }
}

#[async_trait]
impl DetectionSource for ShortcutDetectionSource {
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
            {
                if entry
                    .path()
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .is_none_or(|ext| !ext.eq_ignore_ascii_case("lnk"))
                {
                    continue;
                }
                let Ok(bytes) = fs::read(entry.path()) else {
                    continue;
                };
                let Some(executable) = parse_shortcut_target(&bytes) else {
                    continue;
                };
                if executable.is_file()
                    && request
                        .executables
                        .iter()
                        .any(|name| file_name_eq(&executable, name))
                {
                    found.push(installation(
                        infer_version(&executable).unwrap_or_else(UNKNOWN_VERSION),
                        executable,
                        "windows-shortcut",
                        entry.path().display().to_string(),
                        75,
                    ));
                }
            }
        }
        Ok(found)
    }
}

pub fn parse_shortcut_target(bytes: &[u8]) -> Option<PathBuf> {
    if read_u32(bytes, 0)? != 0x4c {
        return None;
    }
    let flags = read_u32(bytes, 0x14)?;
    let mut offset = 0x4c;
    if flags & 0x1 != 0 {
        offset += 2 + read_u16(bytes, offset)? as usize;
    }
    if flags & 0x2 == 0 {
        return None;
    }
    let link_info_size = read_u32(bytes, offset)? as usize;
    let header_size = read_u32(bytes, offset + 4)? as usize;
    if link_info_size < header_size || offset.checked_add(link_info_size)? > bytes.len() {
        return None;
    }
    let local_offset = read_u32(bytes, offset + 16)? as usize;
    if header_size >= 0x24 {
        let unicode_offset = read_u32(bytes, offset + 28)? as usize;
        if unicode_offset != 0 {
            return read_utf16_path(bytes, offset + unicode_offset, offset + link_info_size);
        }
    }
    if local_offset == 0 {
        return None;
    }
    read_ansi_path(bytes, offset + local_offset, offset + link_info_size)
}

fn read_ansi_path(bytes: &[u8], start: usize, end: usize) -> Option<PathBuf> {
    let data = bytes.get(start..end)?;
    let length = data.iter().position(|byte| *byte == 0)?;
    Some(String::from_utf8_lossy(&data[..length]).into_owned().into())
}

fn read_utf16_path(bytes: &[u8], start: usize, end: usize) -> Option<PathBuf> {
    let mut words = Vec::new();
    for chunk in bytes.get(start..end)?.chunks_exact(2) {
        let word = u16::from_le_bytes([chunk[0], chunk[1]]);
        if word == 0 {
            break;
        }
        words.push(word);
    }
    (!words.is_empty()).then(|| PathBuf::from(String::from_utf16_lossy(&words)))
}

fn read_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        bytes.get(offset..offset + 2)?.try_into().ok()?,
    ))
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

fn parse_unity_version(input: &str) -> Option<Version> {
    let numeric_end = input
        .find(|character: char| !character.is_ascii_digit() && character != '.')
        .unwrap_or(input.len());
    let mut version = Version::parse(&input[..numeric_end]).ok()?;
    let suffix = input[numeric_end..].trim();
    if !suffix.is_empty() {
        version.pre = Prerelease::new(suffix).ok()?;
    }
    Some(version)
}

fn parse_loose_version(input: &str) -> Option<Version> {
    for candidate in input.split(|character: char| !character.is_ascii_digit() && character != '.')
    {
        let candidate = candidate.trim_matches('.');
        if candidate.is_empty() {
            continue;
        }
        let parts = candidate.split('.').count();
        let normalized = match parts {
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

fn vdf_tokens(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut characters = input.chars().peekable();
    while let Some(character) = characters.next() {
        if character != '"' {
            continue;
        }
        let mut token = String::new();
        while let Some(character) = characters.next() {
            match character {
                '"' => break,
                '\\' => match characters.next() {
                    Some('\\') => token.push('\\'),
                    Some('"') => token.push('"'),
                    Some(other) => {
                        token.push('\\');
                        token.push(other);
                    }
                    None => token.push('\\'),
                },
                other => token.push(other),
            }
        }
        tokens.push(token);
    }
    tokens
}

fn json_string(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_owned)
}

fn name_matches(name: Option<&str>, request: &ScanRequest) -> bool {
    name.is_some_and(|name| {
        contains_ignore_ascii_case(name, &request.display_name)
            || contains_ignore_ascii_case(name, &request.app_id)
    })
}

fn contains_ignore_ascii_case(value: &str, needle: &str) -> bool {
    !needle.is_empty()
        && value
            .to_ascii_lowercase()
            .contains(&needle.to_ascii_lowercase())
}

fn file_name_eq(path: &Path, name: &str) -> bool {
    path.file_name()
        .and_then(|file| file.to_str())
        .is_some_and(|file| file.eq_ignore_ascii_case(name))
}

fn read_optional(path: &Path) -> io::Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(contents) => Ok(Some(contents)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn invalid_data(error: impl std::error::Error + Send + Sync + 'static) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
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
