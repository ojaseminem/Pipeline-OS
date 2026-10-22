//! Fast project discovery.
//!
//! Instead of walking whole drives, discovery asks the tools that already know
//! where projects are (Unity Hub's project list, Unreal's recent projects,
//! Godot's project manager), then does a shallow, bounded pass over the
//! folders people actually keep projects in. It finishes in well under a
//! second on a typical machine, so the app can run it on every launch and
//! keep the results cached.
//!
//! Besides engine projects it recognizes other creative and app projects
//! (GameMaker, Defold, Xcode, Android Studio, Visual Studio, and DCC working
//! folders for Blender, Houdini, Maya and Spine) so they can be added too.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::{Duration, Instant, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProjectEngine {
    Unity,
    Unreal,
    Godot,
    PipelineOs,
    #[serde(rename = "gamemaker")]
    GameMaker,
    Defold,
    Blender,
    Houdini,
    Maya,
    Spine,
    Xcode,
    AndroidStudio,
    VisualStudio,
}

impl ProjectEngine {
    /// Loose DCC working folders (a folder of `.blend`/`.hip`/`.spine`
    /// files). They are only recognized below a scanned folder, never as the
    /// scanned folder itself, so `~/Desktop` with one `.blend` on it is not
    /// taken for a project.
    pub fn is_loose_folder(self) -> bool {
        matches!(self, Self::Blender | Self::Houdini | Self::Spine)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredProject {
    pub path: String,
    pub name: String,
    pub engine: ProjectEngine,
    /// Engine version the project was last saved with, when known.
    pub engine_version: Option<String>,
    /// Where it was found: "unity-hub", "unreal-recent", "godot-projects", "folders".
    pub source: String,
    /// Unix seconds of the project's last modification (marker file).
    pub modified: u64,
}

/// Where discovery looks. Built from the OS by [`DiscoverySources::system`];
/// tests construct it directly.
#[derive(Debug, Clone, Default)]
pub struct DiscoverySources {
    pub unity_hub_lists: Vec<PathBuf>,
    pub unreal_settings_globs: Vec<String>,
    pub godot_lists: Vec<PathBuf>,
    /// Unity Hub `projectDir.json` files naming the default projects folder.
    pub unity_hub_project_dirs: Vec<PathBuf>,
    pub folders: Vec<PathBuf>,
    pub max_depth: usize,
    /// Directories visited per scanned folder (0: [`DEFAULT_MAX_DIRS`]).
    pub max_dirs: usize,
    /// Overall time budget for the folder pass (`None`: [`DEFAULT_BUDGET`]).
    pub time_budget: Option<Duration>,
}

/// Default cap on directories visited under one scanned folder.
pub const DEFAULT_MAX_DIRS: usize = 25_000;
/// Default time budget for the whole folder pass.
pub const DEFAULT_BUDGET: Duration = Duration::from_secs(8);

impl DiscoverySources {
    pub fn system() -> Self {
        let home = home_dir();
        let mut sources = DiscoverySources {
            max_depth: 4,
            ..Default::default()
        };
        let Some(home) = home else { return sources };
        // Per-OS application config folders.
        let (config, local) = if cfg!(windows) {
            (
                std::env::var_os("APPDATA").map(PathBuf::from),
                std::env::var_os("LOCALAPPDATA").map(PathBuf::from),
            )
        } else if cfg!(target_os = "macos") {
            let support = home.join("Library/Application Support");
            (Some(support.clone()), Some(support))
        } else {
            (
                Some(
                    std::env::var_os("XDG_CONFIG_HOME")
                        .map(PathBuf::from)
                        .unwrap_or_else(|| home.join(".config")),
                ),
                Some(home.join(".local/share")),
            )
        };
        if let Some(config) = &config {
            sources
                .unity_hub_lists
                .push(config.join("UnityHub/projects-v1.json"));
            sources
                .unity_hub_project_dirs
                .push(config.join("UnityHub/projectDir.json"));
            sources.godot_lists.push(config.join("Godot/projects.cfg"));
        }
        if let Some(local) = &local {
            sources.godot_lists.push(local.join("godot/projects.cfg"));
            // Windows: %LOCALAPPDATA%/UnrealEngine/<ver>/Saved/Config/WindowsEditor/EditorSettings.ini
            sources.unreal_settings_globs.push(format!(
                "{}/UnrealEngine/*/Saved/Config/*/EditorSettings.ini",
                slash(local)
            ));
            // macOS: ~/Library/Application Support/Epic/UnrealEngine/<ver>/Saved/Config/...
            sources.unreal_settings_globs.push(format!(
                "{}/Epic/UnrealEngine/*/Saved/Config/*/EditorSettings.ini",
                slash(local)
            ));
        }
        sources.unreal_settings_globs.push(format!(
            "{}/.config/Epic/UnrealEngine/*/Saved/Config/*/EditorSettings.ini",
            slash(&home)
        ));
        // Folders people keep projects in. Never the whole home folder, and
        // never ~/Library (macOS privacy prompts).
        for folder in [
            "Documents",
            "Desktop",
            "Projects",
            "projects",
            "Developer",
            "dev",
            "Dev",
            "Repos",
            "repos",
            "source/repos",
            "GitHub",
            "Work",
            "Unity Projects",
            "Unreal Projects",
            "Godot",
            "Games",
            "Documents/Unreal Projects",
            "Documents/Unity Projects",
            "Documents/GameMakerStudio2",
            "AndroidStudioProjects",
            "Xcode Projects",
        ] {
            sources.folders.push(home.join(folder));
        }
        if cfg!(windows) {
            // OneDrive moves Documents/Desktop when folder backup is on.
            for var in ["OneDrive", "OneDriveConsumer", "OneDriveCommercial"] {
                if let Some(onedrive) = std::env::var_os(var).map(PathBuf::from) {
                    for folder in ["Documents", "Desktop", "Documents/Unreal Projects"] {
                        sources.folders.push(onedrive.join(folder));
                    }
                }
            }
            // Project folders at the root of every fixed (local) drive.
            // Network and removable drives are skipped: probing them can
            // stall or wake hardware.
            for letter in b'C'..=b'Z' {
                let drive = PathBuf::from(format!("{}:\\", letter as char));
                if !is_fixed_drive(&drive) {
                    continue;
                }
                for folder in [
                    "Projects",
                    "Dev",
                    "Development",
                    "Games",
                    "GameDev",
                    "Repos",
                    "Work",
                    "Unity",
                    "UnityProjects",
                    "Unity Projects",
                    "Unreal",
                    "Unreal Projects",
                    "Godot",
                ] {
                    sources.folders.push(drive.join(folder));
                }
            }
        }
        sources.folders.retain(|folder| folder.is_dir());
        sources.folders.sort();
        sources.folders.dedup();
        sources
    }
}

#[cfg(windows)]
fn is_fixed_drive(root: &Path) -> bool {
    use std::os::windows::ffi::OsStrExt;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetDriveTypeW(root_path_name: *const u16) -> u32;
    }
    const DRIVE_FIXED: u32 = 3;
    let wide: Vec<u16> = root
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    // SAFETY: `wide` is a NUL-terminated UTF-16 buffer alive for the call.
    unsafe { GetDriveTypeW(wide.as_ptr()) == DRIVE_FIXED }
}

#[cfg(not(windows))]
fn is_fixed_drive(root: &Path) -> bool {
    root.is_dir()
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from)
}

fn slash(path: &Path) -> String {
    path.display().to_string().replace('\\', "/")
}

/// Identifies a project folder by its engine's marker files.
pub fn identify_project(dir: &Path) -> Option<(ProjectEngine, Option<String>, PathBuf)> {
    let unity_version = dir.join("ProjectSettings/ProjectVersion.txt");
    if unity_version.is_file() && dir.join("Assets").is_dir() {
        let version = std::fs::read_to_string(&unity_version)
            .ok()
            .and_then(|text| {
                text.lines()
                    .find_map(|line| line.strip_prefix("m_EditorVersion:"))
                    .map(|v| v.trim().to_owned())
            });
        return Some((ProjectEngine::Unity, version, unity_version));
    }
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("uproject") {
                let version = std::fs::read_to_string(&path)
                    .ok()
                    .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
                    .and_then(|json| json.get("EngineAssociation")?.as_str().map(str::to_owned))
                    .filter(|v| !v.is_empty());
                return Some((ProjectEngine::Unreal, version, path));
            }
        }
    }
    let godot = dir.join("project.godot");
    if godot.is_file() {
        let version = std::fs::read_to_string(&godot).ok().and_then(|text| {
            text.lines()
                .find(|line| line.starts_with("config/features="))
                .and_then(|line| line.split('"').nth(1).map(str::to_owned))
        });
        return Some((ProjectEngine::Godot, version, godot));
    }
    let pipeline = dir.join(".pipelineos/project.toml");
    if pipeline.is_file() {
        return Some((ProjectEngine::PipelineOs, None, pipeline));
    }
    identify_other(dir)
}

/// Non-engine project types, by marker files at the folder's top level.
fn identify_other(dir: &Path) -> Option<(ProjectEngine, Option<String>, PathBuf)> {
    let mut files: Vec<(String, PathBuf, bool)> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .take(2_000)
        .filter_map(|entry| {
            let is_dir = entry.file_type().ok()?.is_dir();
            Some((
                entry.file_name().to_string_lossy().to_ascii_lowercase(),
                entry.path(),
                is_dir,
            ))
        })
        .collect();
    files.sort();
    let with_ext = |exts: &[&str], want_dir: bool| {
        files.iter().find_map(|(name, path, is_dir)| {
            let ext = Path::new(name).extension()?.to_str()?;
            (*is_dir == want_dir && exts.contains(&ext)).then(|| path.clone())
        })
    };
    let named = |file: &str| {
        files
            .iter()
            .find(|(name, _, is_dir)| !is_dir && name == file)
            .map(|(_, path, _)| path.clone())
    };
    if let Some(yyp) = with_ext(&["yyp"], false) {
        static IDE_VERSION: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
        let pattern = IDE_VERSION
            .get_or_init(|| regex::Regex::new(r#""IDEVersion"\s*:\s*"([^"]+)""#).expect("regex"));
        let version = std::fs::read_to_string(&yyp)
            .ok()
            .and_then(|raw| pattern.captures(&raw).map(|caps| caps[1].to_owned()));
        return Some((ProjectEngine::GameMaker, version, yyp));
    }
    if let Some(defold) = named("game.project") {
        return Some((ProjectEngine::Defold, None, defold));
    }
    if let Some(xcode) = with_ext(&["xcworkspace", "xcodeproj"], true) {
        return Some((ProjectEngine::Xcode, None, xcode));
    }
    let has_app = files
        .iter()
        .any(|(name, _, is_dir)| *is_dir && name == "app");
    if has_app
        && let Some(settings) = named("settings.gradle.kts").or_else(|| named("settings.gradle"))
    {
        return Some((ProjectEngine::AndroidStudio, None, settings));
    }
    if let Some(sln) = with_ext(&["sln", "slnx"], false) {
        return Some((ProjectEngine::VisualStudio, None, sln));
    }
    if let Some(workspace) = named("workspace.mel") {
        return Some((ProjectEngine::Maya, None, workspace));
    }
    if let Some(hip) = with_ext(&["hip", "hiplc", "hipnc"], false) {
        return Some((ProjectEngine::Houdini, None, hip));
    }
    if let Some(spine) = with_ext(&["spine"], false) {
        return Some((ProjectEngine::Spine, None, spine));
    }
    if let Some(blend) = with_ext(&["blend"], false) {
        return Some((ProjectEngine::Blender, None, blend));
    }
    None
}

fn modified(path: &Path) -> u64 {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn project_name(dir: &Path, engine: ProjectEngine, marker: &Path) -> String {
    match engine {
        ProjectEngine::Unreal
        | ProjectEngine::GameMaker
        | ProjectEngine::Xcode
        | ProjectEngine::VisualStudio => marker.file_stem(),
        _ => dir.file_name(),
    }
    .map(|name| name.to_string_lossy().into_owned())
    .unwrap_or_else(|| dir.display().to_string())
}

/// The real path of a folder (symlinks, `..`, `/var` → `/private/var` on
/// macOS resolved), without Windows' `\\?\` prefix; the input if it can't be
/// resolved. Used for both found and registered paths so they compare.
pub fn resolve_path(dir: &Path) -> PathBuf {
    std::fs::canonicalize(dir)
        .map(|path| {
            let text = path.display().to_string();
            match text.strip_prefix(r"\\?\") {
                Some(rest) if !rest.starts_with("UNC") => PathBuf::from(rest),
                _ => path,
            }
        })
        .unwrap_or_else(|_| dir.to_path_buf())
}

/// Comparison key for a project folder.
pub fn path_key(dir: &Path) -> String {
    slash(&resolve_path(dir))
        .trim_end_matches('/')
        .to_ascii_lowercase()
}

fn record(found: &mut BTreeMap<String, DiscoveredProject>, dir: &Path, source: &str) {
    // The same folder reaches us spelled differently (Unreal's recent list
    // keeps `..` segments, `./`, symlinks, a trailing slash); resolve it so
    // each project is suggested once.
    let resolved = resolve_path(dir);
    let dir = resolved.as_path();
    let Some((engine, engine_version, marker)) = identify_project(dir) else {
        return;
    };
    let key = slash(dir).trim_end_matches('/').to_ascii_lowercase();
    found.entry(key).or_insert_with(|| DiscoveredProject {
        path: dir.display().to_string(),
        name: project_name(dir, engine, &marker),
        engine,
        engine_version,
        source: source.to_owned(),
        modified: modified(&marker),
    });
}

/// Paths listed in Unity Hub's `projects-v1.json`.
pub fn unity_hub_projects(raw: &str) -> Vec<PathBuf> {
    let Ok(json) = serde_json::from_str::<serde_json::Value>(raw) else {
        return Vec::new();
    };
    let data = json.get("data").unwrap_or(&json);
    data.as_object()
        .into_iter()
        .flat_map(|map| map.iter())
        .filter_map(|(key, value)| {
            value
                .get("path")
                .and_then(|p| p.as_str())
                .map(PathBuf::from)
                .or_else(|| Some(PathBuf::from(key)))
        })
        .collect()
}

/// `.uproject` paths from Unreal's EditorSettings.ini recent-project entries.
pub fn unreal_recent_projects(raw: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for line in raw.lines() {
        let mut rest = line;
        while let Some(end) = rest.find(".uproject") {
            let head = &rest[..end + ".uproject".len()];
            let start = head.rfind(['"', '=']).map(|i| i + 1).unwrap_or(0);
            let candidate = head[start..].trim();
            if !candidate.is_empty() {
                out.push(PathBuf::from(candidate));
            }
            rest = &rest[end + ".uproject".len()..];
        }
    }
    out
}

/// Folders new Unreal projects were created in (`CreatedProjectPaths=`).
pub fn unreal_created_project_dirs(raw: &str) -> Vec<PathBuf> {
    raw.lines()
        .filter_map(|line| line.trim().strip_prefix("CreatedProjectPaths="))
        .map(|value| value.trim().trim_matches('"'))
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .collect()
}

/// The default projects folder from Unity Hub's `projectDir.json`.
pub fn unity_hub_project_dir(raw: &str) -> Option<PathBuf> {
    let json = serde_json::from_str::<serde_json::Value>(raw).ok()?;
    json.get("directoryPath")
        .and_then(|v| v.as_str())
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

/// Project folders from Godot's `projects.cfg` (`[path]` section headers).
pub fn godot_projects(raw: &str) -> Vec<PathBuf> {
    raw.lines()
        .filter_map(|line| {
            line.trim()
                .strip_prefix('[')?
                .strip_suffix(']')
                .map(PathBuf::from)
        })
        .collect()
}

const SKIP: &[&str] = &[
    "node_modules",
    "ShaderCache",
    "UserSettings",
    "Pods",
    "DerivedData",
    "bower_components",
    "$RECYCLE.BIN",
    "System Volume Information",
    "Windows",
    "Program Files",
    "Program Files (x86)",
    "ProgramData",
    "Library",
    "Temp",
    "Logs",
    "obj",
    "Intermediate",
    "Saved",
    "DerivedDataCache",
    "Binaries",
    "Build",
    "Builds",
    "AppData",
    "Packages",
    ".git",
    ".svn",
    ".godot",
    ".cache",
    "__pycache__",
    "venv",
    ".venv",
    "target",
];

fn scan_folder(
    root: &Path,
    sources: &DiscoverySources,
    deadline: Instant,
    found: &mut BTreeMap<String, DiscoveredProject>,
) {
    let max_dirs = if sources.max_dirs == 0 {
        DEFAULT_MAX_DIRS
    } else {
        sources.max_dirs
    };
    let mut walker = walkdir_lite(root, sources.max_depth);
    let mut visited = 0;
    while let Some((dir, depth)) = walker.next_dir() {
        visited += 1;
        if visited > max_dirs || Instant::now() >= deadline {
            break;
        }
        let Some((engine, ..)) = identify_project(&dir) else {
            continue;
        };
        if depth == 0 && engine.is_loose_folder() {
            continue;
        }
        record(found, &dir, "folders");
        walker.skip_children_of(&dir);
    }
}

/// A tiny breadth-first directory walker that can prune subtrees, so a found
/// project's contents (and big generated folders) are never visited.
struct WalkLite {
    queue: std::collections::VecDeque<(PathBuf, usize)>,
    max_depth: usize,
}

fn walkdir_lite(root: &Path, max_depth: usize) -> WalkLite {
    WalkLite {
        queue: [(root.to_path_buf(), 0)].into_iter().collect(),
        max_depth,
    }
}

impl WalkLite {
    /// Returns the next directory and its depth, queueing its children; call
    /// [`Self::skip_children_of`] to drop them again.
    fn next_dir(&mut self) -> Option<(PathBuf, usize)> {
        let (dir, depth) = self.queue.pop_front()?;
        Some((self.expand(dir, depth), depth))
    }

    fn expand(&mut self, dir: PathBuf, depth: usize) -> PathBuf {
        if depth < self.max_depth
            && let Ok(entries) = std::fs::read_dir(&dir)
        {
            for entry in entries.flatten() {
                let Ok(kind) = entry.file_type() else {
                    continue;
                };
                if !kind.is_dir() {
                    continue;
                }
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if (name.starts_with('.') && name != ".pipelineos")
                    || SKIP.contains(&name.as_ref())
                    || name.ends_with(".xcodeproj")
                    || name.ends_with(".xcworkspace")
                    || name.ends_with(".app")
                {
                    continue;
                }
                self.queue.push_back((entry.path(), depth + 1));
            }
        }
        dir
    }

    fn skip_children_of(&mut self, dir: &Path) {
        self.queue
            .retain(|(path, _)| !path.starts_with(dir) || path == dir);
    }
}

/// Runs every discovery source. Paths in `known` (already registered) are
/// left out. Blocking: call from a worker thread.
pub fn discover(sources: &DiscoverySources, known: &[PathBuf]) -> Vec<DiscoveredProject> {
    let mut found = BTreeMap::new();
    for list in &sources.unity_hub_lists {
        if let Ok(raw) = std::fs::read_to_string(list) {
            for dir in unity_hub_projects(&raw) {
                record(&mut found, &dir, "unity-hub");
            }
        }
    }
    let mut folders = sources.folders.clone();
    for list in &sources.unity_hub_project_dirs {
        if let Some(dir) = std::fs::read_to_string(list)
            .ok()
            .and_then(|raw| unity_hub_project_dir(&raw))
        {
            folders.push(dir);
        }
    }
    for pattern in &sources.unreal_settings_globs {
        let Ok(paths) = glob_paths(pattern) else {
            continue;
        };
        for settings in paths {
            if let Ok(raw) = std::fs::read_to_string(&settings) {
                for uproject in unreal_recent_projects(&raw) {
                    if let Some(dir) = uproject.parent() {
                        record(&mut found, dir, "unreal-recent");
                    }
                }
                folders.extend(unreal_created_project_dirs(&raw));
            }
        }
    }
    for list in &sources.godot_lists {
        if let Ok(raw) = std::fs::read_to_string(list) {
            for dir in godot_projects(&raw) {
                record(&mut found, &dir, "godot-projects");
            }
        }
    }
    // Scan each folder once, skipping ones inside an already-listed folder.
    folders.retain(|folder| folder.is_dir());
    folders.sort();
    folders.dedup();
    let mut scanned: Vec<PathBuf> = Vec::new();
    let deadline = Instant::now() + sources.time_budget.unwrap_or(DEFAULT_BUDGET);
    for folder in folders {
        // Already covered by an earlier scan of a parent folder.
        let covered = scanned.iter().any(|done| {
            folder
                .strip_prefix(done)
                .is_ok_and(|rest| rest.components().count() < sources.max_depth.max(1))
        });
        if covered {
            continue;
        }
        scan_folder(&folder, sources, deadline, &mut found);
        scanned.push(folder);
    }
    let known: Vec<String> = known.iter().map(|p| path_key(p)).collect();
    let mut projects: Vec<_> = found
        .into_iter()
        .filter(|(key, _)| !known.contains(key))
        .map(|(_, project)| project)
        .collect();
    projects.sort_by(|a, b| {
        b.modified
            .cmp(&a.modified)
            .then_with(|| a.name.cmp(&b.name))
    });
    projects
}

/// Minimal glob for the `*` wildcards used in the Unreal settings patterns.
fn glob_paths(pattern: &str) -> Result<Vec<PathBuf>, ()> {
    let mut current = vec![PathBuf::new()];
    let absolute = pattern.starts_with('/');
    for (index, part) in pattern.split('/').enumerate() {
        if part.is_empty() {
            if index == 0 && absolute {
                current = vec![PathBuf::from("/")];
            }
            continue;
        }
        let mut next = Vec::new();
        for base in &current {
            if part.contains('*') {
                let (prefix, suffix) = part.split_once('*').unwrap_or((part, ""));
                let dir = if base.as_os_str().is_empty() {
                    PathBuf::from(".")
                } else {
                    base.clone()
                };
                if let Ok(entries) = std::fs::read_dir(&dir) {
                    for entry in entries.flatten() {
                        let name = entry.file_name().to_string_lossy().into_owned();
                        if name.starts_with(prefix) && name.ends_with(suffix) {
                            next.push(base.join(name));
                        }
                    }
                }
            } else {
                let candidate = if base.as_os_str().is_empty() {
                    PathBuf::from(part)
                } else {
                    base.join(part)
                };
                if candidate.exists() {
                    next.push(candidate);
                }
            }
        }
        current = next;
        if current.is_empty() {
            break;
        }
    }
    Ok(current)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, contents: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }

    fn unity(dir: &Path, version: &str) {
        write(
            &dir.join("ProjectSettings/ProjectVersion.txt"),
            &format!("m_EditorVersion: {version}\n"),
        );
        std::fs::create_dir_all(dir.join("Assets")).unwrap();
    }

    #[test]
    fn parses_engine_project_lists() {
        let hub = r#"{"schema_version":"v1","data":{"/w/Voidline":{"path":"/w/Voidline","title":"Voidline"}}}"#;
        assert_eq!(unity_hub_projects(hub), vec![PathBuf::from("/w/Voidline")]);
        let ini = "[/Script/UnrealEd.EditorSettings]\nRecentlyOpenedProjectFiles=(ProjectName=\"C:/Work/Mech/MechBay.uproject\",LastOpenTime=2026.10.01-10.00.00)\nRecentlyOpenedProjectFiles=D:/Old/Old.uproject\n";
        assert_eq!(
            unreal_recent_projects(ini),
            vec![
                PathBuf::from("C:/Work/Mech/MechBay.uproject"),
                PathBuf::from("D:/Old/Old.uproject")
            ]
        );
        let godot =
            "[/home/me/games/atlas]\nfavorite=false\n\n[/home/me/games/helix]\nfavorite=true\n";
        assert_eq!(godot_projects(godot).len(), 2);
    }

    #[test]
    fn discovers_from_lists_and_folders_without_descending_into_projects() {
        let home = tempfile::tempdir().unwrap();
        let root = home.path();
        // Listed by Unity Hub, outside the scanned folders.
        let hub_project = root.join("elsewhere/Voidline");
        unity(&hub_project, "2022.3.18f1");
        write(
            &root.join("config/UnityHub/projects-v1.json"),
            &format!(r#"{{"data":{{"x":{{"path":"{}"}}}}}}"#, slash(&hub_project)),
        );
        // Found by the folder pass.
        let unreal = root.join("Projects/MechBay");
        write(
            &unreal.join("MechBay.uproject"),
            r#"{"EngineAssociation":"5.4"}"#,
        );
        let godot = root.join("Projects/games/Atlas");
        write(
            &godot.join("project.godot"),
            "config/features=PackedStringArray(\"4.3\", \"Forward Plus\")\n",
        );
        // A nested "project" inside a project must not be reported separately.
        unity(&unreal.join("Plugins/Sample"), "6000.0.1f1");
        // Already registered: excluded.
        let known = root.join("Projects/Known");
        write(&known.join(".pipelineos/project.toml"), "name = \"Known\"");

        let sources = DiscoverySources {
            unity_hub_lists: vec![root.join("config/UnityHub/projects-v1.json")],
            folders: vec![root.join("Projects")],
            max_depth: 4,
            ..Default::default()
        };
        let found = discover(&sources, std::slice::from_ref(&known));
        let names: Vec<_> = found
            .iter()
            .map(|p| (p.name.as_str(), p.engine, p.source.as_str()))
            .collect();
        assert!(
            names.contains(&("Voidline", ProjectEngine::Unity, "unity-hub")),
            "{names:?}"
        );
        assert!(
            names.contains(&("MechBay", ProjectEngine::Unreal, "folders")),
            "{names:?}"
        );
        assert!(
            names.contains(&("Atlas", ProjectEngine::Godot, "folders")),
            "{names:?}"
        );
        assert!(
            !names.iter().any(|(name, ..)| *name == "Sample"),
            "nested project skipped"
        );
        assert!(
            !names.iter().any(|(name, ..)| *name == "Known"),
            "registered skipped"
        );
        let mech = found.iter().find(|p| p.name == "MechBay").unwrap();
        assert_eq!(mech.engine_version.as_deref(), Some("5.4"));
        let atlas = found.iter().find(|p| p.name == "Atlas").unwrap();
        assert_eq!(atlas.engine_version.as_deref(), Some("4.3"));
    }

    #[test]
    fn identifies_other_project_types() {
        let root = tempfile::tempdir().unwrap();
        let case = |name: &str, files: &[&str]| {
            let dir = root.path().join(name);
            for file in files {
                if let Some(folder) = file.strip_suffix('/') {
                    std::fs::create_dir_all(dir.join(folder)).unwrap();
                } else {
                    write(
                        &dir.join(file),
                        "{\"MetaData\": {\"IDEVersion\": \"2024.8.1.171\"}}",
                    );
                }
            }
            identify_project(&dir).map(|(engine, version, _)| (engine, version))
        };
        assert_eq!(
            case("Gm", &["Gm.yyp"]),
            Some((ProjectEngine::GameMaker, Some("2024.8.1.171".into())))
        );
        assert_eq!(
            case("Df", &["game.project"]).unwrap().0,
            ProjectEngine::Defold
        );
        assert_eq!(
            case("Xc", &["App.xcodeproj/"]).unwrap().0,
            ProjectEngine::Xcode
        );
        assert_eq!(
            case("As", &["settings.gradle.kts", "app/"]).unwrap().0,
            ProjectEngine::AndroidStudio
        );
        assert_eq!(case("Gradle", &["settings.gradle"]), None, "needs app/");
        assert_eq!(
            case("Vs", &["Tool.sln"]).unwrap().0,
            ProjectEngine::VisualStudio
        );
        assert_eq!(
            case("My", &["workspace.mel"]).unwrap().0,
            ProjectEngine::Maya
        );
        assert_eq!(
            case("Hou", &["fx.hiplc"]).unwrap().0,
            ProjectEngine::Houdini
        );
        assert_eq!(case("Sp", &["hero.spine"]).unwrap().0, ProjectEngine::Spine);
        assert_eq!(
            case("Bl", &["shot.blend"]).unwrap().0,
            ProjectEngine::Blender
        );
        assert_eq!(case("Empty", &["notes.txt"]), None);
        // Unity wins over the .sln it generates.
        let unity_dir = root.path().join("U");
        unity(&unity_dir, "6000.0.1f1");
        write(&unity_dir.join("U.sln"), "");
        assert_eq!(
            identify_project(&unity_dir).unwrap().0,
            ProjectEngine::Unity
        );
        assert_eq!(
            serde_json::to_value(ProjectEngine::GameMaker).unwrap(),
            "gamemaker"
        );
        assert_eq!(
            serde_json::to_value(ProjectEngine::AndroidStudio).unwrap(),
            "android-studio"
        );
    }

    #[test]
    fn loose_dcc_folders_and_extra_sources() {
        let home = tempfile::tempdir().unwrap();
        let root = home.path();
        // A .blend sitting on the scanned folder itself is not a project...
        write(&root.join("Desktop/quick.blend"), "");
        // ...but a folder of .blend files below it is.
        write(&root.join("Desktop/Shorts/shot010.blend"), "");
        // Never descended into: generated folders.
        write(&root.join("Desktop/node_modules/pkg/x.blend"), "");
        // Unity Hub's default project folder and Unreal's created-projects
        // folder are scanned too.
        let hub_dir = root.join("HubProjects");
        unity(&hub_dir.join("Comet"), "2022.3.1f1");
        write(
            &root.join("config/UnityHub/projectDir.json"),
            &format!(r#"{{"directoryPath":"{}"}}"#, slash(&hub_dir)),
        );
        let created = root.join("UnrealCreated");
        write(&created.join("Rift/Rift.uproject"), "{}");
        let settings = root.join("UnrealEngine/5.4/Saved/Config/WindowsEditor/EditorSettings.ini");
        write(
            &settings,
            &format!(
                "[/Script/UnrealEd.EditorSettings]\nCreatedProjectPaths={}\n",
                slash(&created)
            ),
        );
        let sources = DiscoverySources {
            unity_hub_project_dirs: vec![root.join("config/UnityHub/projectDir.json")],
            unreal_settings_globs: vec![format!(
                "{}/UnrealEngine/*/Saved/Config/*/EditorSettings.ini",
                slash(root)
            )],
            folders: vec![root.join("Desktop")],
            max_depth: 4,
            ..Default::default()
        };
        let found = discover(&sources, &[]);
        let names: Vec<_> = found.iter().map(|p| (p.name.as_str(), p.engine)).collect();
        assert!(
            names.contains(&("Shorts", ProjectEngine::Blender)),
            "{names:?}"
        );
        assert!(
            names.contains(&("Comet", ProjectEngine::Unity)),
            "{names:?}"
        );
        assert!(
            names.contains(&("Rift", ProjectEngine::Unreal)),
            "{names:?}"
        );
        assert!(
            !names.iter().any(|(n, _)| *n == "Desktop" || *n == "pkg"),
            "{names:?}"
        );
    }

    #[test]
    fn walker_respects_the_directory_cap() {
        let home = tempfile::tempdir().unwrap();
        for i in 0..20 {
            std::fs::create_dir_all(home.path().join(format!("Projects/d{i:02}"))).unwrap();
        }
        write(&home.path().join("Projects/zz/Late/game.project"), "");
        let sources = |max_dirs| DiscoverySources {
            folders: vec![home.path().join("Projects")],
            max_depth: 4,
            max_dirs,
            ..Default::default()
        };
        assert_eq!(discover(&sources(0), &[]).len(), 1);
        assert!(discover(&sources(5), &[]).is_empty());
    }

    #[test]
    fn globs_unreal_settings() {
        let home = tempfile::tempdir().unwrap();
        let settings = home
            .path()
            .join("UnrealEngine/5.4/Saved/Config/WindowsEditor/EditorSettings.ini");
        write(&settings, "x");
        let pattern = format!(
            "{}/UnrealEngine/*/Saved/Config/*/EditorSettings.ini",
            slash(home.path())
        );
        assert_eq!(glob_paths(&pattern).unwrap(), vec![settings]);
    }

    #[cfg(unix)]
    #[test]
    fn registered_projects_match_however_their_path_is_spelled() {
        // macOS temp and home folders resolve through symlinks (/var ->
        // /private/var); a project registered under one spelling must not be
        // suggested again under the other.
        let root = tempfile::tempdir().expect("root");
        let project = root.path().join("real/Known");
        write(
            &project.join(".pipelineos/project.toml"),
            "name = \"Known\"",
        );
        let alias = root.path().join("alias");
        std::os::unix::fs::symlink(root.path().join("real"), &alias).expect("symlink");
        let sources = DiscoverySources {
            folders: vec![alias.clone()],
            max_depth: 3,
            ..Default::default()
        };
        assert!(discover(&sources, &[alias.join("Known")]).is_empty());
        assert!(discover(&sources, &[project.clone()]).is_empty());
        assert_eq!(path_key(&alias.join("Known")), path_key(&project));
    }
}
