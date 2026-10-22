//! Fast project discovery.
//!
//! Instead of walking whole drives, discovery asks the tools that already know
//! where projects are (Unity Hub's project list, Unreal's recent projects,
//! Godot's project manager), then does a shallow, bounded pass over the
//! folders people actually keep projects in. It finishes in well under a
//! second on a typical machine, so the app can run it on every launch and
//! keep the results cached.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProjectEngine {
    Unity,
    Unreal,
    Godot,
    PipelineOs,
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
    pub folders: Vec<PathBuf>,
    pub max_depth: usize,
}

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
        ] {
            sources.folders.push(home.join(folder));
        }
        // Windows: project folders at the root of every drive.
        if cfg!(windows) {
            for letter in b'C'..=b'Z' {
                let drive = PathBuf::from(format!("{}:\\", letter as char));
                if !drive.is_dir() {
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
                    "Unreal Projects",
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
        ProjectEngine::Unreal => marker.file_stem(),
        _ => dir.file_name(),
    }
    .map(|name| name.to_string_lossy().into_owned())
    .unwrap_or_else(|| dir.display().to_string())
}

fn record(found: &mut BTreeMap<String, DiscoveredProject>, dir: &Path, source: &str) {
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

fn scan_folder(root: &Path, max_depth: usize, found: &mut BTreeMap<String, DiscoveredProject>) {
    let mut walker = walkdir_lite(root, max_depth);
    while let Some(dir) = walker.next_dir() {
        if identify_project(&dir).is_some() {
            record(found, &dir, "folders");
            walker.skip_children_of(&dir);
        }
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
    /// Returns the next directory, queueing its children; call
    /// [`Self::skip_children_of`] to drop them again.
    fn next_dir(&mut self) -> Option<PathBuf> {
        let (dir, depth) = self.queue.pop_front()?;
        Some(self.expand(dir, depth))
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
                if (name.starts_with('.') && name != ".pipelineos") || SKIP.contains(&name.as_ref())
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
    for folder in &sources.folders {
        scan_folder(folder, sources.max_depth, &mut found);
    }
    let known: Vec<String> = known
        .iter()
        .map(|p| slash(p).trim_end_matches('/').to_ascii_lowercase())
        .collect();
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
}
