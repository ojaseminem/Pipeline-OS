//! External merge and diff tools: finds what's installed (VS Code, Rider,
//! Beyond Compare, P4Merge, UnityYAMLMerge for scenes and prefabs…), writes
//! the base/ours/theirs versions of a conflicted file to temp files, and
//! launches the tool with its own 3-way argument order. Launching never
//! blocks the app: the tool runs on its own and the user marks the file
//! resolved afterwards (`mark_resolved`).

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    process::Stdio,
    time::{Duration, SystemTime},
};

use serde::{Deserialize, Serialize};

use crate::{GitProvider, VcsError, VcsOperationResult};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum MergeToolKind {
    VsCode,
    Cursor,
    Rider,
    VisualStudio,
    BeyondCompare,
    P4Merge,
    KDiff3,
    Meld,
    SublimeMerge,
    FileMerge,
    UnityYamlMerge,
}

impl MergeToolKind {
    pub const ALL: [MergeToolKind; 11] = [
        Self::UnityYamlMerge,
        Self::VsCode,
        Self::Cursor,
        Self::Rider,
        Self::VisualStudio,
        Self::BeyondCompare,
        Self::P4Merge,
        Self::KDiff3,
        Self::Meld,
        Self::SublimeMerge,
        Self::FileMerge,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Self::VsCode => "vscode",
            Self::Cursor => "cursor",
            Self::Rider => "rider",
            Self::VisualStudio => "visualstudio",
            Self::BeyondCompare => "bcompare",
            Self::P4Merge => "p4merge",
            Self::KDiff3 => "kdiff3",
            Self::Meld => "meld",
            Self::SublimeMerge => "smerge",
            Self::FileMerge => "filemerge",
            Self::UnityYamlMerge => "unityyamlmerge",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::VsCode => "Visual Studio Code",
            Self::Cursor => "Cursor",
            Self::Rider => "JetBrains Rider",
            Self::VisualStudio => "Visual Studio",
            Self::BeyondCompare => "Beyond Compare",
            Self::P4Merge => "P4Merge",
            Self::KDiff3 => "KDiff3",
            Self::Meld => "Meld",
            Self::SublimeMerge => "Sublime Merge",
            Self::FileMerge => "FileMerge (Xcode)",
            Self::UnityYamlMerge => "UnityYAMLMerge (Smart Merge)",
        }
    }

    pub fn supports_diff(self) -> bool {
        !matches!(self, Self::SublimeMerge | Self::UnityYamlMerge)
    }

    /// UnityYAMLMerge understands only Unity's serialized YAML assets.
    pub fn unity_only(self) -> bool {
        matches!(self, Self::UnityYamlMerge)
    }
}

/// An installed merge/diff tool.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MergeTool {
    pub id: String,
    pub name: String,
    pub kind: MergeToolKind,
    pub path: String,
    pub supports_merge: bool,
    pub supports_diff: bool,
    /// Only offered for Unity YAML assets (.unity, .prefab, .asset…).
    pub unity_only: bool,
}

/// Temp files written for a launched tool.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LaunchedTool {
    pub tool_id: String,
    pub pid: u32,
    pub base: Option<String>,
    pub ours: Option<String>,
    pub theirs: Option<String>,
    /// The file the tool writes the result to (the working copy).
    pub result: String,
}

const UNITY_YAML_EXTENSIONS: &[&str] = &[
    "unity",
    "prefab",
    "asset",
    "mat",
    "anim",
    "controller",
    "overridecontroller",
    "physicmaterial",
    "physicsmaterial2d",
    "playable",
    "mask",
    "lighting",
    "giparams",
    "rendertexture",
    "spriteatlas",
    "spriteatlasv2",
    "terrainlayer",
    "mixer",
    "preset",
    "signal",
    "flare",
    "fontsettings",
    "guiskin",
    "brush",
    "shadervariants",
];

/// Whether `path` is a Unity serialized YAML asset UnityYAMLMerge can merge.
pub fn is_unity_yaml_asset(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| UNITY_YAML_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()))
}

fn os(values: &[&Path]) -> Vec<OsString> {
    values
        .iter()
        .map(|path| path.as_os_str().to_owned())
        .collect()
}

/// Command-line arguments for a 3-way merge, in each tool's own order.
pub fn merge_arguments(
    kind: MergeToolKind,
    base: &Path,
    ours: &Path,
    theirs: &Path,
    result: &Path,
) -> Vec<OsString> {
    let flag = |value: &str| OsString::from(value);
    let mut arguments: Vec<OsString> = Vec::new();
    match kind {
        MergeToolKind::VsCode | MergeToolKind::Cursor => {
            arguments.extend([flag("--wait"), flag("--merge")]);
            arguments.extend(os(&[ours, theirs, base, result]));
        }
        MergeToolKind::Rider => {
            arguments.push(flag("merge"));
            arguments.extend(os(&[ours, theirs, base, result]));
        }
        MergeToolKind::VisualStudio => {
            arguments.push(flag("/m"));
            arguments.extend(os(&[theirs, ours, base, result]));
        }
        MergeToolKind::BeyondCompare => arguments.extend(os(&[ours, theirs, base, result])),
        MergeToolKind::P4Merge => arguments.extend(os(&[base, theirs, ours, result])),
        MergeToolKind::KDiff3 => {
            arguments.extend(os(&[base, ours, theirs]));
            arguments.push(flag("-o"));
            arguments.push(result.as_os_str().to_owned());
        }
        MergeToolKind::Meld => {
            arguments.extend(os(&[ours, base, theirs]));
            let mut output = OsString::from("--output=");
            output.push(result.as_os_str());
            arguments.push(output);
        }
        MergeToolKind::SublimeMerge => {
            arguments.push(flag("mergetool"));
            arguments.extend(os(&[base, ours, theirs]));
            arguments.push(flag("-o"));
            arguments.push(result.as_os_str().to_owned());
        }
        MergeToolKind::FileMerge => {
            arguments.extend(os(&[ours, theirs]));
            arguments.push(flag("-ancestor"));
            arguments.push(base.as_os_str().to_owned());
            arguments.push(flag("-merge"));
            arguments.push(result.as_os_str().to_owned());
        }
        MergeToolKind::UnityYamlMerge => {
            arguments.extend([flag("merge"), flag("-p")]);
            arguments.extend(os(&[base, theirs, ours, result]));
        }
    }
    arguments
}

/// Command-line arguments for a 2-way diff (`left` = old, `right` = new), or
/// `None` when the tool can't diff two arbitrary files.
pub fn diff_arguments(kind: MergeToolKind, left: &Path, right: &Path) -> Option<Vec<OsString>> {
    let mut arguments = Vec::new();
    match kind {
        MergeToolKind::VsCode | MergeToolKind::Cursor => arguments.push(OsString::from("--diff")),
        MergeToolKind::Rider => arguments.push(OsString::from("diff")),
        MergeToolKind::SublimeMerge | MergeToolKind::UnityYamlMerge => return None,
        _ => {}
    }
    arguments.extend(os(&[left, right]));
    Some(arguments)
}

// ------------------------------------------------------------ detection

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn home() -> Option<PathBuf> {
    env_path("HOME").or_else(|| env_path("USERPROFILE"))
}

/// Finds `name` on `PATH` (trying `.exe`/`.cmd`/`.bat` on Windows).
fn which(name: &str) -> Option<PathBuf> {
    let windows = std::env::consts::OS == "windows";
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        if windows && Path::new(name).extension().is_none() {
            for ext in ["exe", "cmd", "bat"] {
                let candidate = dir.join(format!("{name}.{ext}"));
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        } else {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Sort key that orders version-like names numerically (2022.3.10 > 2022.3.9).
fn version_key(name: &str) -> Vec<u64> {
    name.split(|c: char| !c.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse().ok())
        .collect()
}

/// Sub-directories of `dir` whose name starts with `prefix`, newest
/// version first.
fn subdirs(dir: &Path, prefix: &str) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut dirs: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(prefix))
        })
        .collect();
    dirs.sort_by_key(|path| {
        std::cmp::Reverse(version_key(
            path.file_name().and_then(|n| n.to_str()).unwrap_or(""),
        ))
    });
    dirs
}

fn unity_editor_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    match std::env::consts::OS {
        "windows" => {
            for base in [
                env_path("ProgramFiles"),
                Some(PathBuf::from(r"C:\Program Files")),
            ]
            .into_iter()
            .flatten()
            {
                roots.push(base.join("Unity").join("Hub").join("Editor"));
            }
            // Unity Hub's custom install location, a JSON string.
            if let Some(appdata) = env_path("APPDATA") {
                let file = appdata.join("UnityHub").join("secondaryInstallPath.json");
                if let Ok(raw) = std::fs::read_to_string(file) {
                    let path = raw.trim().trim_matches('"').replace("\\\\", "\\");
                    if !path.is_empty() {
                        roots.push(PathBuf::from(path));
                    }
                }
            }
        }
        "macos" => roots.push(PathBuf::from("/Applications/Unity/Hub/Editor")),
        _ => {
            if let Some(home) = home() {
                roots.push(home.join("Unity").join("Hub").join("Editor"));
            }
        }
    }
    roots.dedup();
    roots
}

fn unity_yaml_merge_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    for root in unity_editor_roots() {
        for editor in subdirs(&root, "") {
            candidates.push(match std::env::consts::OS {
                "windows" => editor.join(r"Editor\Data\Tools\UnityYAMLMerge.exe"),
                "macos" => editor.join("Unity.app/Contents/Tools/UnityYAMLMerge"),
                _ => editor.join("Editor/Data/Tools/UnityYAMLMerge"),
            });
        }
    }
    match std::env::consts::OS {
        "windows" => {
            if let Some(base) = env_path("ProgramFiles") {
                candidates.push(base.join(r"Unity\Editor\Data\Tools\UnityYAMLMerge.exe"));
            }
        }
        "macos" => candidates.push(PathBuf::from(
            "/Applications/Unity/Unity.app/Contents/Tools/UnityYAMLMerge",
        )),
        _ => {}
    }
    candidates
}

fn visual_studio_candidates() -> Vec<PathBuf> {
    const TAIL: &str =
        r"Common7\IDE\CommonExtensions\Microsoft\TeamFoundation\Team Explorer\vsDiffMerge.exe";
    let mut candidates = Vec::new();
    let x86 =
        env_path("ProgramFiles(x86)").unwrap_or_else(|| PathBuf::from(r"C:\Program Files (x86)"));
    let vswhere = x86.join(r"Microsoft Visual Studio\Installer\vswhere.exe");
    if vswhere.is_file() {
        let mut command = std::process::Command::new(&vswhere);
        command.args(["-latest", "-products", "*", "-property", "installationPath"]);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }
        if let Ok(output) = command.output() {
            for line in String::from_utf8_lossy(&output.stdout).lines() {
                let line = line.trim();
                if !line.is_empty() {
                    candidates.push(PathBuf::from(line).join(TAIL));
                }
            }
        }
    }
    for base in [env_path("ProgramFiles"), Some(x86)].into_iter().flatten() {
        for year in subdirs(&base.join("Microsoft Visual Studio"), "20") {
            for edition in subdirs(&year, "") {
                candidates.push(edition.join(TAIL));
            }
        }
    }
    candidates
}

/// Well-known install locations for a tool on this OS (PATH is searched
/// separately).
fn known_locations(kind: MergeToolKind) -> Vec<PathBuf> {
    let os = std::env::consts::OS;
    let local = env_path("LOCALAPPDATA");
    let program_files: Vec<PathBuf> = [
        env_path("ProgramFiles"),
        env_path("ProgramFiles(x86)"),
        Some(PathBuf::from(r"C:\Program Files")),
    ]
    .into_iter()
    .flatten()
    .collect();
    let user_apps = home().map(|home| home.join("Applications"));
    let mut paths = Vec::new();
    let mac_app = |paths: &mut Vec<PathBuf>, bundle: &str, tail: &str| {
        paths.push(PathBuf::from("/Applications").join(bundle).join(tail));
        if let Some(apps) = &user_apps {
            paths.push(apps.join(bundle).join(tail));
        }
    };
    match (kind, os) {
        (MergeToolKind::UnityYamlMerge, _) => return unity_yaml_merge_candidates(),
        (MergeToolKind::VisualStudio, "windows") => return visual_studio_candidates(),
        (MergeToolKind::VisualStudio, _) => {}
        (MergeToolKind::VsCode, "windows") => {
            if let Some(local) = &local {
                paths.push(local.join(r"Programs\Microsoft VS Code\bin\code.cmd"));
            }
            for base in &program_files {
                paths.push(base.join(r"Microsoft VS Code\bin\code.cmd"));
            }
        }
        (MergeToolKind::VsCode, "macos") => mac_app(
            &mut paths,
            "Visual Studio Code.app",
            "Contents/Resources/app/bin/code",
        ),
        (MergeToolKind::VsCode, _) => {
            paths.extend(["/usr/bin/code", "/snap/bin/code"].map(PathBuf::from))
        }
        (MergeToolKind::Cursor, "windows") => {
            if let Some(local) = &local {
                paths.push(local.join(r"Programs\cursor\resources\app\bin\cursor.cmd"));
            }
        }
        (MergeToolKind::Cursor, "macos") => mac_app(
            &mut paths,
            "Cursor.app",
            "Contents/Resources/app/bin/cursor",
        ),
        (MergeToolKind::Cursor, _) => paths.push(PathBuf::from("/usr/bin/cursor")),
        (MergeToolKind::Rider, "windows") => {
            if let Some(local) = &local {
                paths.push(local.join(r"Programs\Rider\bin\rider64.exe"));
                paths.push(local.join(r"JetBrains\Toolbox\scripts\rider.cmd"));
            }
            for base in &program_files {
                for dir in subdirs(&base.join("JetBrains"), "JetBrains Rider") {
                    paths.push(dir.join(r"bin\rider64.exe"));
                }
            }
        }
        (MergeToolKind::Rider, "macos") => mac_app(&mut paths, "Rider.app", "Contents/MacOS/rider"),
        (MergeToolKind::Rider, _) => {
            if let Some(home) = home() {
                paths.push(home.join(".local/share/JetBrains/Toolbox/scripts/rider"));
            }
            paths.push(PathBuf::from("/snap/bin/rider"));
        }
        (MergeToolKind::BeyondCompare, "windows") => {
            for base in &program_files {
                for version in ["Beyond Compare 5", "Beyond Compare 4"] {
                    paths.push(base.join(version).join("BComp.exe"));
                }
            }
        }
        (MergeToolKind::BeyondCompare, "macos") => {
            mac_app(&mut paths, "Beyond Compare.app", "Contents/MacOS/bcomp")
        }
        (MergeToolKind::BeyondCompare, _) => paths.push(PathBuf::from("/usr/bin/bcompare")),
        (MergeToolKind::P4Merge, "windows") => {
            for base in &program_files {
                paths.push(base.join(r"Perforce\p4merge.exe"));
            }
        }
        (MergeToolKind::P4Merge, "macos") => mac_app(
            &mut paths,
            "p4merge.app",
            "Contents/Resources/launchp4merge",
        ),
        (MergeToolKind::P4Merge, _) => {}
        (MergeToolKind::KDiff3, "windows") => {
            for base in &program_files {
                paths.push(base.join(r"KDiff3\kdiff3.exe"));
                paths.push(base.join(r"KDiff3\bin\kdiff3.exe"));
            }
        }
        (MergeToolKind::KDiff3, "macos") => {
            mac_app(&mut paths, "kdiff3.app", "Contents/MacOS/kdiff3")
        }
        (MergeToolKind::KDiff3, _) => {}
        (MergeToolKind::Meld, "windows") => {
            for base in &program_files {
                paths.push(base.join(r"Meld\Meld.exe"));
            }
        }
        (MergeToolKind::Meld, "macos") => mac_app(&mut paths, "Meld.app", "Contents/MacOS/Meld"),
        (MergeToolKind::Meld, _) => {}
        (MergeToolKind::SublimeMerge, "windows") => {
            for base in &program_files {
                paths.push(base.join(r"Sublime Merge\smerge.exe"));
            }
        }
        (MergeToolKind::SublimeMerge, "macos") => mac_app(
            &mut paths,
            "Sublime Merge.app",
            "Contents/SharedSupport/bin/smerge",
        ),
        (MergeToolKind::SublimeMerge, _) => {
            paths.push(PathBuf::from("/opt/sublime_merge/sublime_merge"))
        }
        (MergeToolKind::FileMerge, "macos") => paths.push(PathBuf::from("/usr/bin/opendiff")),
        (MergeToolKind::FileMerge, _) => {}
    }
    paths
}

/// Executable names to look for on PATH.
fn path_names(kind: MergeToolKind) -> &'static [&'static str] {
    let mac = std::env::consts::OS == "macos";
    match kind {
        MergeToolKind::VsCode => &["code"],
        MergeToolKind::Cursor => &["cursor"],
        MergeToolKind::Rider => &["rider", "rider64"],
        MergeToolKind::VisualStudio => &["vsDiffMerge"],
        MergeToolKind::BeyondCompare => &["bcomp", "bcompare"],
        MergeToolKind::P4Merge => &["p4merge"],
        MergeToolKind::KDiff3 => &["kdiff3"],
        MergeToolKind::Meld => &["meld"],
        MergeToolKind::SublimeMerge => &["smerge"],
        MergeToolKind::FileMerge if mac => &["opendiff"],
        MergeToolKind::FileMerge => &[],
        MergeToolKind::UnityYamlMerge => &["UnityYAMLMerge"],
    }
}

/// Every merge/diff tool installed on this machine, in preference order.
/// Blocking (touches the filesystem, may run `vswhere`).
pub fn detect_merge_tools() -> Vec<MergeTool> {
    MergeToolKind::ALL
        .into_iter()
        .filter_map(|kind| {
            let path = path_names(kind)
                .iter()
                .find_map(|name| which(name))
                .or_else(|| known_locations(kind).into_iter().find(|p| p.is_file()))?;
            Some(MergeTool {
                id: kind.id().to_owned(),
                name: kind.name().to_owned(),
                kind,
                path: path.display().to_string(),
                supports_merge: true,
                supports_diff: kind.supports_diff(),
                unity_only: kind.unity_only(),
            })
        })
        .collect()
}

/// Tools that can handle `path`, plus the recommended one: UnityYAMLMerge
/// for Unity YAML assets, otherwise the first general-purpose tool.
pub fn merge_tools_for_path(
    tools: &[MergeTool],
    path: Option<&str>,
) -> (Vec<MergeTool>, Option<String>) {
    let unity = path.is_some_and(is_unity_yaml_asset);
    let usable: Vec<MergeTool> = tools
        .iter()
        .filter(|tool| !tool.unity_only || unity || path.is_none())
        .cloned()
        .collect();
    let recommended = usable
        .iter()
        .find(|tool| unity && tool.unity_only)
        .or_else(|| usable.iter().find(|tool| !tool.unity_only))
        .map(|tool| tool.id.clone());
    (usable, recommended)
}

// ------------------------------------------------------------ launching

fn temp_session_dir() -> std::io::Result<PathBuf> {
    let base = std::env::temp_dir().join("pipelineos-merge");
    std::fs::create_dir_all(&base)?;
    // Best-effort cleanup of sessions older than two days.
    if let Ok(entries) = std::fs::read_dir(&base) {
        let cutoff = SystemTime::now() - Duration::from_secs(2 * 24 * 3600);
        for entry in entries.flatten() {
            let old = entry
                .metadata()
                .and_then(|m| m.modified())
                .is_ok_and(|modified| modified < cutoff);
            if old {
                let _ = std::fs::remove_dir_all(entry.path());
            }
        }
    }
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = base.join(format!("{nanos}-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// `Hero.prefab` + `BASE` → `Hero.BASE.prefab`, keeping the extension so
/// tools pick the right syntax highlighting.
fn label_file(dir: &Path, path: &str, label: &str) -> PathBuf {
    let file = Path::new(path);
    let stem = file.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
    let name = match file.extension().and_then(|e| e.to_str()) {
        Some(ext) => format!("{stem}.{label}.{ext}"),
        None => format!("{stem}.{label}"),
    };
    dir.join(name)
}

fn find_tool(tool_id: &str) -> Result<MergeTool, VcsError> {
    detect_merge_tools()
        .into_iter()
        .find(|tool| tool.id == tool_id)
        .ok_or_else(|| VcsError::Refused(format!("merge tool '{tool_id}' isn't installed")))
}

fn spawn_detached(tool: &MergeTool, root: &Path, arguments: &[OsString]) -> Result<u32, VcsError> {
    let mut command = std::process::Command::new(&tool.path);
    command
        .args(arguments)
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let mut child = command.spawn()?;
    let pid = child.id();
    // Reap the process when it exits so it never lingers as a zombie.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(pid)
}

/// Whether text still contains a conflict block.
pub fn has_conflict_markers(text: &str) -> bool {
    let mut start = false;
    for line in text.lines() {
        if line.starts_with("<<<<<<< ") || line == "<<<<<<<" {
            start = true;
        } else if start && (line.starts_with(">>>>>>> ") || line == ">>>>>>>") {
            return true;
        }
    }
    false
}

impl GitProvider {
    /// Content of `spec` (`:1:path`, `HEAD:path`…), or `None` if absent.
    async fn blob(&self, root: &Path, spec: &str) -> Option<Vec<u8>> {
        let output = self.run_raw(root, &["show", spec]).await.ok()?;
        output.status.success().then_some(output.stdout)
    }

    /// Opens a conflicted file in an external 3-way merge tool. Writes the
    /// common ancestor (`:1:`), ours (`:2:`) and theirs (`:3:`) to temp files
    /// and points the tool's output at the working copy. Returns right after
    /// launching; call `mark_resolved` once the user has saved the merge.
    pub async fn launch_merge_tool(
        &self,
        root: &Path,
        path: &str,
        tool_id: &str,
    ) -> Result<LaunchedTool, VcsError> {
        let id = tool_id.to_owned();
        let tool = tokio::task::spawn_blocking(move || find_tool(&id))
            .await
            .map_err(|e| VcsError::Io(std::io::Error::other(e)))??;
        if tool.unity_only && !is_unity_yaml_asset(path) {
            return Err(VcsError::Refused(format!(
                "{} only merges Unity scene/prefab/asset files",
                tool.name
            )));
        }
        if !self.conflicted_files(root).await?.iter().any(|f| f == path) {
            return Err(VcsError::Refused(format!("'{path}' has no merge conflict")));
        }
        let dir = temp_session_dir()?;
        let mut written = Vec::new();
        for (stage, label) in [(1, "BASE"), (2, "LOCAL"), (3, "REMOTE")] {
            let file = label_file(&dir, path, label);
            // A missing stage (e.g. no common ancestor) becomes an empty file.
            let content = self
                .blob(root, &format!(":{stage}:{path}"))
                .await
                .unwrap_or_default();
            std::fs::write(&file, content)?;
            written.push(file);
        }
        let result = root.join(path);
        let arguments = merge_arguments(tool.kind, &written[0], &written[1], &written[2], &result);
        let pid = spawn_detached(&tool, root, &arguments)?;
        let display = |p: &PathBuf| Some(p.display().to_string());
        Ok(LaunchedTool {
            tool_id: tool.id,
            pid,
            base: display(&written[0]),
            ours: display(&written[1]),
            theirs: display(&written[2]),
            result: result.display().to_string(),
        })
    }

    /// Opens a 2-way diff of the working copy against `HEAD` in an external
    /// tool. Returns right after launching.
    pub async fn open_diff_tool(
        &self,
        root: &Path,
        path: &str,
        tool_id: &str,
    ) -> Result<LaunchedTool, VcsError> {
        let id = tool_id.to_owned();
        let tool = tokio::task::spawn_blocking(move || find_tool(&id))
            .await
            .map_err(|e| VcsError::Io(std::io::Error::other(e)))??;
        let dir = temp_session_dir()?;
        let left = label_file(&dir, path, "HEAD");
        let head = self
            .blob(root, &format!("HEAD:{path}"))
            .await
            .unwrap_or_default();
        std::fs::write(&left, head)?;
        let mut right = root.join(path);
        if !right.is_file() {
            // Deleted in the working copy: diff against an empty file.
            right = label_file(&dir, path, "WORKING");
            std::fs::write(&right, b"")?;
        }
        let arguments = diff_arguments(tool.kind, &left, &right).ok_or_else(|| {
            VcsError::Refused(format!("{} can't show a two-file diff", tool.name))
        })?;
        let pid = spawn_detached(&tool, root, &arguments)?;
        Ok(LaunchedTool {
            tool_id: tool.id,
            pid,
            base: None,
            ours: Some(left.display().to_string()),
            theirs: None,
            result: right.display().to_string(),
        })
    }

    /// Stages a file whose conflict was resolved in an external tool.
    /// Refuses while a text file still contains conflict markers.
    pub async fn mark_resolved(
        &self,
        root: &Path,
        path: &str,
    ) -> Result<VcsOperationResult, VcsError> {
        if let Ok(bytes) = std::fs::read(root.join(path))
            && !bytes[..bytes.len().min(8192)].contains(&0)
            && has_conflict_markers(&String::from_utf8_lossy(&bytes))
        {
            return Err(VcsError::Refused(format!(
                "'{path}' still contains conflict markers. Finish the merge and save first."
            )));
        }
        self.operation(root, &["add", "--", path]).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(arguments: Vec<OsString>) -> Vec<String> {
        arguments
            .into_iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    fn merge(kind: MergeToolKind) -> Vec<String> {
        strings(merge_arguments(
            kind,
            Path::new("B"),
            Path::new("O"),
            Path::new("T"),
            Path::new("R"),
        ))
    }

    #[test]
    fn merge_argument_orders() {
        assert_eq!(
            merge(MergeToolKind::VsCode),
            ["--wait", "--merge", "O", "T", "B", "R"]
        );
        assert_eq!(
            merge(MergeToolKind::Cursor),
            ["--wait", "--merge", "O", "T", "B", "R"]
        );
        assert_eq!(merge(MergeToolKind::Rider), ["merge", "O", "T", "B", "R"]);
        assert_eq!(
            merge(MergeToolKind::VisualStudio),
            ["/m", "T", "O", "B", "R"]
        );
        assert_eq!(merge(MergeToolKind::BeyondCompare), ["O", "T", "B", "R"]);
        assert_eq!(merge(MergeToolKind::P4Merge), ["B", "T", "O", "R"]);
        assert_eq!(merge(MergeToolKind::KDiff3), ["B", "O", "T", "-o", "R"]);
        assert_eq!(merge(MergeToolKind::Meld), ["O", "B", "T", "--output=R"]);
        assert_eq!(
            merge(MergeToolKind::SublimeMerge),
            ["mergetool", "B", "O", "T", "-o", "R"]
        );
        assert_eq!(
            merge(MergeToolKind::FileMerge),
            ["O", "T", "-ancestor", "B", "-merge", "R"]
        );
        assert_eq!(
            merge(MergeToolKind::UnityYamlMerge),
            ["merge", "-p", "B", "T", "O", "R"]
        );
    }

    #[test]
    fn diff_argument_orders() {
        let diff = |kind| diff_arguments(kind, Path::new("L"), Path::new("W")).map(strings);
        assert_eq!(diff(MergeToolKind::VsCode).unwrap(), ["--diff", "L", "W"]);
        assert_eq!(diff(MergeToolKind::Rider).unwrap(), ["diff", "L", "W"]);
        assert_eq!(diff(MergeToolKind::P4Merge).unwrap(), ["L", "W"]);
        assert!(diff(MergeToolKind::UnityYamlMerge).is_none());
        assert!(diff(MergeToolKind::SublimeMerge).is_none());
    }

    #[test]
    fn unity_assets_get_smart_merge() {
        assert!(is_unity_yaml_asset("Assets/Scenes/Main.unity"));
        assert!(is_unity_yaml_asset("Assets/Hero.PREFAB"));
        assert!(!is_unity_yaml_asset("Assets/Scripts/Hero.cs"));
        let tool = |kind: MergeToolKind| MergeTool {
            id: kind.id().into(),
            name: kind.name().into(),
            kind,
            path: "x".into(),
            supports_merge: true,
            supports_diff: kind.supports_diff(),
            unity_only: kind.unity_only(),
        };
        let tools = vec![
            tool(MergeToolKind::UnityYamlMerge),
            tool(MergeToolKind::VsCode),
        ];
        let (usable, recommended) = merge_tools_for_path(&tools, Some("Assets/Main.unity"));
        assert_eq!(usable.len(), 2);
        assert_eq!(recommended.as_deref(), Some("unityyamlmerge"));
        let (usable, recommended) = merge_tools_for_path(&tools, Some("src/main.cs"));
        assert_eq!(usable.len(), 1);
        assert_eq!(recommended.as_deref(), Some("vscode"));
    }

    #[test]
    fn versions_sort_numerically() {
        assert!(version_key("2022.3.10f1") > version_key("2022.3.9f1"));
        assert!(version_key("6000.0.1f1") > version_key("2022.3.40f1"));
    }

    #[test]
    fn conflict_marker_detection() {
        assert!(has_conflict_markers(
            "<<<<<<< HEAD\na\n=======\nb\n>>>>>>> x\n"
        ));
        assert!(!has_conflict_markers("a\n=======\nb\n"));
    }

    #[test]
    fn labels_keep_extension() {
        assert_eq!(
            label_file(Path::new("/t"), "Assets/Hero.prefab", "BASE"),
            Path::new("/t").join("Hero.BASE.prefab")
        );
        assert_eq!(
            label_file(Path::new("/t"), "Makefile", "LOCAL"),
            Path::new("/t").join("Makefile.LOCAL")
        );
    }
}
