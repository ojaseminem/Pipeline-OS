//! Ready-made task presets for engines and DCCs, grouped for the "add task"
//! picker and ranked for the project at hand.
//!
//! Presets are ordinary [`TaskDefinition`]s (argument vectors, never shell
//! strings). [`task_presets`] detects the project's engine from its marker
//! files and the host OS, bakes in what it can (the `.uproject` name, Godot
//! export preset names, the first `.blend` file) and puts the presets that fit
//! first, marked `recommended`.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use serde::Serialize;

use crate::{
    StudioError,
    tasks::{TaskDefinition, TaskKind},
};

/// A preset task plus how the picker should present it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskPreset {
    pub definition: TaskDefinition,
    /// "unity", "unreal", "godot", "blender", or `None` for general tasks.
    pub engine: Option<String>,
    /// Picker heading, e.g. "Unity · Android".
    pub group: String,
    /// Fits this project and this computer.
    pub recommended: bool,
    /// Saved by the team in `.pipelineos/presets.toml` (vs. built in).
    #[serde(default)]
    pub project: bool,
}

/// Operating system the presets are generated for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Host {
    Windows,
    Mac,
    Linux,
}

impl Host {
    pub fn current() -> Self {
        if cfg!(windows) {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::Mac
        } else {
            Self::Linux
        }
    }
}

/// Where the ready-made Unity build script is installed.
pub const UNITY_BUILD_SCRIPT_PATH: &str = "Assets/Editor/PipelineOSBuild.cs";

/// C# source of the Editor build script the Unity presets call
/// (`PipelineOS.Build.Windows`, `.Android`, ...).
pub fn unity_build_script() -> &'static str {
    include_str!("templates/PipelineOSBuild.cs")
}

/// Writes [`unity_build_script`] to `Assets/Editor/PipelineOSBuild.cs` unless
/// a file is already there (it is the team's to edit), returning its path.
pub fn install_unity_build_script(root: &Path) -> Result<PathBuf, StudioError> {
    if !root.join("Assets").is_dir() {
        return Err(StudioError::Invalid(format!(
            "{} has no Assets folder; is it a Unity project?",
            root.display()
        )));
    }
    let path = root.join(UNITY_BUILD_SCRIPT_PATH);
    if path.exists() {
        return Ok(path);
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| StudioError::io(parent, e))?;
    }
    use std::io::Write as _;
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
    {
        Ok(mut file) => file
            .write_all(unity_build_script().as_bytes())
            .map_err(|e| StudioError::io(&path, e))?,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(StudioError::io(&path, error)),
    }
    Ok(path)
}

/// The engine a project folder belongs to, from its marker files:
/// "unity", "unreal", "godot", "blender", or `None`.
pub fn detect_engine(root: &Path) -> Option<&'static str> {
    if root.join("ProjectSettings/ProjectVersion.txt").is_file() {
        return Some("unity");
    }
    if first_with_extension(root, "uproject").is_some() {
        return Some("unreal");
    }
    if root.join("project.godot").is_file() {
        return Some("godot");
    }
    if first_with_extension(root, "blend").is_some() {
        return Some("blender");
    }
    None
}

fn first_with_extension(root: &Path, extension: &str) -> Option<String> {
    let mut names: Vec<String> = std::fs::read_dir(root)
        .ok()?
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|t| t.is_file()))
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| {
            Path::new(name)
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case(extension))
        })
        .collect();
    names.sort();
    names.into_iter().next()
}

/// Presets for the project at `root` on this computer, recommended first.
pub fn task_presets(root: &Path) -> Vec<TaskPreset> {
    task_presets_for(root, Host::current())
}

/// [`task_presets`] for an explicit host OS.
pub fn task_presets_for(root: &Path, host: Host) -> Vec<TaskPreset> {
    let engine = detect_engine(root);
    let facts = ProjectFacts::read(root);
    let mut presets = catalog(&facts, host);
    for preset in &mut presets {
        preset.recommended = match (&preset.engine, engine) {
            (Some(preset_engine), Some(project)) => preset_engine == project && preset.recommended,
            // General tasks are recommended only where they clearly apply.
            (None, _) => preset.recommended,
            (Some(_), None) => false,
        };
    }
    // Recommended first, then this project's engine, then general tasks,
    // then other engines; the catalog order is kept within each band.
    let rank = |preset: &TaskPreset| match (preset.recommended, preset.engine.as_deref()) {
        (true, _) => 0,
        (false, Some(e)) if Some(e) == engine => 1,
        (false, None) => 2,
        _ => 3,
    };
    presets.sort_by_key(rank);
    // The project's own presets come first; a broken presets.toml just means
    // none are offered (the file's error shows when it's opened for editing).
    let own = crate::tasks::load_project_presets(root).unwrap_or_default();
    own.into_iter()
        .map(|definition| TaskPreset {
            definition,
            engine: None,
            group: "This project".into(),
            recommended: false,
            project: true,
        })
        .chain(presets)
        .collect()
}

/// Every preset's definition in catalog order, for a generic project on this
/// computer (CLI and MCP `tasks templates`).
pub fn all_preset_definitions() -> Vec<TaskDefinition> {
    catalog(&ProjectFacts::default(), Host::current())
        .into_iter()
        .map(|preset| preset.definition)
        .collect()
}

/// Project details baked into presets when they are known.
#[derive(Debug, Default)]
struct ProjectFacts {
    /// `.uproject` file name, e.g. "MechBay.uproject".
    uproject: Option<String>,
    blend: Option<String>,
    /// Godot export preset names by platform ("Windows Desktop" -> name).
    godot_presets: BTreeMap<String, String>,
    uses_lfs: bool,
}

impl ProjectFacts {
    fn read(root: &Path) -> Self {
        let godot_presets = std::fs::read_to_string(root.join("export_presets.cfg"))
            .map(|raw| godot_export_presets(&raw))
            .unwrap_or_default();
        let uses_lfs = std::fs::read_to_string(root.join(".gitattributes"))
            .is_ok_and(|raw| raw.contains("filter=lfs"));
        Self {
            uproject: first_with_extension(root, "uproject"),
            blend: first_with_extension(root, "blend"),
            godot_presets,
            uses_lfs,
        }
    }
}

/// Maps each platform in Godot's `export_presets.cfg` to its first preset's
/// name, so presets export with the names the project actually uses.
pub fn godot_export_presets(raw: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut name: Option<String> = None;
    let mut platform: Option<String> = None;
    let mut flush = |name: &mut Option<String>, platform: &mut Option<String>| {
        if let (Some(n), Some(p)) = (name.take(), platform.take()) {
            out.entry(p).or_insert(n);
        }
    };
    for line in raw.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            flush(&mut name, &mut platform);
            continue;
        }
        let quoted = |value: &str| value.trim().trim_matches('"').to_owned();
        if let Some(value) = line.strip_prefix("name=") {
            name = Some(quoted(value));
        } else if let Some(value) = line.strip_prefix("platform=") {
            platform = Some(quoted(value));
        }
    }
    flush(&mut name, &mut platform);
    out
}

struct Builder {
    presets: Vec<TaskPreset>,
}

impl Builder {
    #[allow(clippy::too_many_arguments)]
    fn add(
        &mut self,
        engine: Option<&str>,
        group: &str,
        recommended: bool,
        id: &str,
        name: &str,
        kind: TaskKind,
        description: &str,
        program: &str,
        args: &[&str],
    ) {
        self.presets.push(TaskPreset {
            project: false,
            definition: TaskDefinition {
                id: id.into(),
                name: name.into(),
                kind,
                description: Some(description.into()),
                program: program.into(),
                args: args.iter().map(|a| (*a).to_owned()).collect(),
                cwd: None,
                env: BTreeMap::new(),
            },
            engine: engine.map(str::to_owned),
            group: group.into(),
            recommended,
        });
    }
}

fn catalog(facts: &ProjectFacts, host: Host) -> Vec<TaskPreset> {
    let mut b = Builder {
        presets: Vec::new(),
    };
    unity_presets(&mut b, host);
    unreal_presets(&mut b, facts, host);
    godot_presets(&mut b, facts);
    blender_presets(&mut b, facts);
    general_presets(&mut b, facts);
    b.presets
}

const UNITY_SCRIPT_NOTE: &str = "Calls the Pipeline OS Editor build script (Assets/Editor/PipelineOSBuild.cs); install it from the task picker or write your own static method with the same name.";

fn unity_presets(b: &mut Builder, host: Host) {
    let unity = Some("unity");
    let build = |b: &mut Builder,
                 group: &str,
                 recommended: bool,
                 id: &str,
                 name: &str,
                 summary: &str,
                 target: &str,
                 method: &str,
                 extra: &[&str]| {
        let description =
            format!("{summary} Requires PipelineOS.Build.{method}. {UNITY_SCRIPT_NOTE}");
        let execute = format!("PipelineOS.Build.{method}");
        let mut args = vec![
            "-batchmode",
            "-quit",
            "-nographics",
            "-projectPath",
            "${project}",
            "-buildTarget",
            target,
            "-executeMethod",
            execute.as_str(),
        ];
        args.extend_from_slice(extra);
        args.extend_from_slice(&["-logFile", "-"]);
        b.add(
            unity,
            group,
            recommended,
            id,
            name,
            TaskKind::Build,
            &description,
            "${engine}",
            &args,
        );
    };
    let desktop = "Unity · Desktop";
    build(
        b,
        desktop,
        host == Host::Windows,
        "unity-build-win64",
        "Unity: Windows development build",
        "Headless Windows (x64) development build with script debugging, written to Builds/Windows/.",
        "Win64",
        "Windows",
        &["-pipelineosDevelopment"],
    );
    build(
        b,
        desktop,
        host == Host::Windows,
        "unity-build-win64-release",
        "Unity: Windows release build",
        "Headless Windows (x64) release build, written to Builds/Windows/.",
        "Win64",
        "Windows",
        &[],
    );
    build(
        b,
        desktop,
        host == Host::Mac,
        "unity-build-macos",
        "Unity: macOS build",
        "Headless macOS (Universal) build, written to Builds/macOS/. Needs the Mac Build Support module.",
        "OSXUniversal",
        "MacOS",
        &[],
    );
    build(
        b,
        desktop,
        host == Host::Linux,
        "unity-build-linux",
        "Unity: Linux build",
        "Headless Linux (x86_64) build, written to Builds/Linux/. Needs the Linux Build Support module.",
        "Linux64",
        "Linux",
        &[],
    );
    let android = "Unity · Android";
    build(
        b,
        android,
        true,
        "unity-build-android-dev",
        "Unity: Android development APK",
        "Development APK with script debugging, written to Builds/Android/. Needs Android Build Support.",
        "Android",
        "Android",
        &["-pipelineosDevelopment"],
    );
    build(
        b,
        android,
        true,
        "unity-build-android-aab",
        "Unity: Android release App Bundle (AAB)",
        "Release App Bundle for Google Play, written to Builds/Android/. Configure the keystore in Player Settings; passwords can come from PIPELINEOS_KEYSTORE_PASS and PIPELINEOS_KEYALIAS_PASS.",
        "Android",
        "Android",
        &["-pipelineosAppBundle"],
    );
    build(
        b,
        "Unity · iOS",
        host == Host::Mac,
        "unity-build-ios",
        "Unity: iOS Xcode project export",
        "Exports an Xcode project to Builds/iOS/; open it in Xcode on a Mac to sign and archive. Needs iOS Build Support.",
        "iOS",
        "IOS",
        &[],
    );
    build(
        b,
        "Unity · Web",
        true,
        "unity-build-webgl",
        "Unity: WebGL build",
        "WebGL player written to Builds/WebGL/. Needs WebGL Build Support.",
        "WebGL",
        "WebGL",
        &[],
    );
    let tests = |b: &mut Builder, id: &str, name: &str, platform: &str| {
        let results = format!("${{project}}/Builds/TestResults/{platform}.xml");
        b.add(
            unity,
            "Unity · Tests",
            true,
            id,
            name,
            TaskKind::Test,
            &format!(
                "Runs the {platform} test suite headlessly (Unity Test Framework) and writes NUnit XML to Builds/TestResults/{platform}.xml."
            ),
            "${engine}",
            &[
                "-batchmode",
                "-nographics",
                "-projectPath",
                "${project}",
                "-runTests",
                "-testPlatform",
                platform,
                "-testResults",
                &results,
                "-logFile",
                "-",
            ],
        );
    };
    tests(b, "unity-tests", "Unity: Run EditMode tests", "EditMode");
    tests(
        b,
        "unity-tests-playmode",
        "Unity: Run PlayMode tests",
        "PlayMode",
    );
    b.add(
        unity,
        "Unity · Content",
        true,
        "unity-addressables",
        "Unity: Build Addressables content",
        TaskKind::Build,
        &format!(
            "Builds Addressables content for the active build target. Requires PipelineOS.Build.Addressables and the com.unity.addressables package. {UNITY_SCRIPT_NOTE}"
        ),
        "${engine}",
        &[
            "-batchmode",
            "-quit",
            "-nographics",
            "-projectPath",
            "${project}",
            "-executeMethod",
            "PipelineOS.Build.Addressables",
            "-logFile",
            "-",
        ],
    );
}

fn unreal_presets(b: &mut Builder, facts: &ProjectFacts, host: Host) {
    let unreal = Some("unreal");
    let uproject = facts
        .uproject
        .clone()
        .unwrap_or_else(|| "${projectName}.uproject".into());
    let target_name = uproject
        .strip_suffix(".uproject")
        .unwrap_or(&uproject)
        .to_owned();
    let project_arg = format!("-project=${{project}}/{uproject}");
    let (uat, desktop_platform, desktop_label) = match host {
        Host::Windows => (
            "${engineDir}/Engine/Build/BatchFiles/RunUAT.bat",
            "Win64",
            "Windows",
        ),
        Host::Mac => (
            "${engineDir}/Engine/Build/BatchFiles/RunUAT.sh",
            "Mac",
            "macOS",
        ),
        Host::Linux => (
            "${engineDir}/Engine/Build/BatchFiles/RunUAT.sh",
            "Linux",
            "Linux",
        ),
    };
    let uat_note =
        "Runs the engine's RunUAT from the project's linked Unreal install (${engineDir}).";
    let package = |b: &mut Builder,
                   group: &str,
                   id: &str,
                   name: &str,
                   platform: &str,
                   config: &str,
                   extra: &[&str]| {
        let platform_arg = format!("-platform={platform}");
        let config_arg = format!("-clientconfig={config}");
        let archive =
            format!("-archivedirectory=${{project}}/Builds/{platform}-{config}-${{date}}");
        let mut args = vec![
            "BuildCookRun",
            project_arg.as_str(),
            platform_arg.as_str(),
            config_arg.as_str(),
            "-build",
            "-cook",
            "-stage",
            "-pak",
            "-archive",
            archive.as_str(),
        ];
        args.extend_from_slice(extra);
        args.extend_from_slice(&["-unattended", "-utf8output", "-nop4"]);
        b.add(
            unreal,
            group,
            true,
            id,
            name,
            TaskKind::Cook,
            &format!(
                "Builds, cooks, stages, packages and archives a {config} {platform} build to Builds/. {uat_note}"
            ),
            uat,
            &args,
        );
    };
    package(
        b,
        "Unreal · Package",
        "unreal-buildcookrun",
        &format!("Unreal: Package {desktop_label} Development"),
        desktop_platform,
        "Development",
        &[],
    );
    package(
        b,
        "Unreal · Package",
        "unreal-buildcookrun-shipping",
        &format!("Unreal: Package {desktop_label} Shipping"),
        desktop_platform,
        "Shipping",
        &["-prereqs", "-nodebuginfo"],
    );
    package(
        b,
        "Unreal · Mobile",
        "unreal-package-android",
        "Unreal: Package Android (ASTC)",
        "Android",
        "Development",
        &["-cookflavor=ASTC"],
    );
    if host == Host::Mac {
        package(
            b,
            "Unreal · Mobile",
            "unreal-package-ios",
            "Unreal: Package iOS",
            "IOS",
            "Development",
            &[],
        );
    }
    let cook_platform = match host {
        Host::Windows => "Win64",
        Host::Mac => "Mac",
        Host::Linux => "Linux",
    };
    let platform_arg = format!("-platform={cook_platform}");
    b.add(
        unreal,
        "Unreal · Content",
        true,
        "unreal-cook",
        "Unreal: Cook content only",
        TaskKind::Cook,
        &format!(
            "Cooks content for {desktop_label} without compiling, staging or packaging. {uat_note}"
        ),
        uat,
        &[
            "BuildCookRun",
            &project_arg,
            &platform_arg,
            "-cook",
            "-skipstage",
            "-nocompileeditor",
            "-unattended",
            "-utf8output",
            "-nop4",
        ],
    );
    let (build_script, build_platform) = match host {
        Host::Windows => ("${engineDir}/Engine/Build/BatchFiles/Build.bat", "Win64"),
        Host::Mac => ("${engineDir}/Engine/Build/BatchFiles/Mac/Build.sh", "Mac"),
        Host::Linux => (
            "${engineDir}/Engine/Build/BatchFiles/Linux/Build.sh",
            "Linux",
        ),
    };
    let editor_target = format!("{target_name}Editor");
    let project_flag = format!("-Project=${{project}}/{uproject}");
    b.add(
        unreal,
        "Unreal · Build",
        true,
        "unreal-build-editor",
        "Unreal: Build editor target",
        TaskKind::Build,
        &format!(
            "Compiles the {editor_target} target (Development, {build_platform}) so the editor opens without a rebuild prompt. Needs a C++ project and the platform toolchain."
        ),
        build_script,
        &[
            &editor_target,
            build_platform,
            "Development",
            &project_flag,
            "-WaitMutex",
        ],
    );
    let project_path = format!("${{project}}/{uproject}");
    b.add(
        unreal,
        "Unreal · Tests",
        true,
        "unreal-automation-tests",
        "Unreal: Run automation tests",
        TaskKind::Test,
        "Runs the project's automation tests headlessly in the editor and exports a report to Builds/AutomationReport/.",
        "${engine}",
        &[
            &project_path,
            "-ExecCmds=Automation RunTests Project; Quit",
            "-unattended",
            "-nullrhi",
            "-nosplash",
            "-nosound",
            "-ReportExportPath=${project}/Builds/AutomationReport",
            "-log",
        ],
    );
}

fn godot_presets(b: &mut Builder, facts: &ProjectFacts) {
    // (platform key in export_presets.cfg, default preset name, label, file).
    let platforms = [
        (
            "Windows Desktop",
            "Windows Desktop",
            "Windows",
            "Windows/${projectName}.exe",
        ),
        ("macOS", "macOS", "macOS", "macOS/${projectName}.zip"),
        ("Linux", "Linux", "Linux", "Linux/${projectName}.x86_64"),
        (
            "Android",
            "Android",
            "Android",
            "Android/${projectName}.apk",
        ),
        ("Web", "Web", "Web", "Web/index.html"),
    ];
    for (platform, default_name, label, file) in platforms {
        let preset_name = facts
            .godot_presets
            .get(platform)
            // Godot 4.0-4.2 called the Linux platform "Linux/X11".
            .or_else(|| {
                (platform == "Linux")
                    .then(|| facts.godot_presets.get("Linux/X11"))
                    .flatten()
            })
            .cloned()
            .unwrap_or_else(|| default_name.to_owned());
        let output = format!("${{project}}/Builds/{file}");
        let slug = label.to_ascii_lowercase();
        for (release, flag) in [(true, "--export-release"), (false, "--export-debug")] {
            let (id, name) = if release {
                (
                    if slug == "windows" {
                        "godot-export".to_owned()
                    } else {
                        format!("godot-export-{slug}")
                    },
                    format!("Godot: Export {label} release"),
                )
            } else {
                (
                    format!("godot-export-{slug}-debug"),
                    format!("Godot: Export {label} debug"),
                )
            };
            b.add(
                Some("godot"),
                &format!("Godot · {label}"),
                true,
                &id,
                &name,
                TaskKind::Export,
                &format!(
                    "Exports the \"{preset_name}\" preset headlessly to Builds/{file}. The preset must exist in Project > Export (export_presets.cfg) with export templates installed; create the Builds/{label} folder first if your Godot version doesn't."
                ),
                "${engine}",
                &["--headless", "--path", "${project}", flag, &preset_name, &output],
            );
        }
    }
}

fn blender_presets(b: &mut Builder, facts: &ProjectFacts) {
    let blend = format!(
        "${{project}}/{}",
        facts.blend.as_deref().unwrap_or("scene.blend")
    );
    let note = if facts.blend.is_some() {
        ""
    } else {
        " Set the .blend file path."
    };
    b.add(
        Some("blender"),
        "Blender · Render",
        true,
        "blender-render",
        "Blender: Render animation",
        TaskKind::Render,
        &format!("Renders the scene's frame range in the background to Renders/<date>/.{note}"),
        "${engine}",
        &[
            "-b",
            &blend,
            "-o",
            "${project}/Renders/${date}/frame_####",
            "-a",
        ],
    );
    b.add(
        Some("blender"),
        "Blender · Render",
        true,
        "blender-render-frame",
        "Blender: Render current frame",
        TaskKind::Render,
        &format!("Renders a single frame (frame 1; change -f) in the background.{note}"),
        "${engine}",
        &[
            "-b",
            &blend,
            "-o",
            "${project}/Renders/${date}/still_####",
            "-f",
            "1",
        ],
    );
    b.add(
        Some("blender"),
        "Blender · Scripts",
        true,
        "blender-python",
        "Blender: Run Python script",
        TaskKind::Script,
        &format!(
            "Opens the file in the background and runs tools/blender_script.py (exports, batch fixes, validation).{note}"
        ),
        "${engine}",
        &[
            "-b",
            &blend,
            "--python",
            "${project}/tools/blender_script.py",
            "--python-exit-code",
            "1",
        ],
    );
}

fn general_presets(b: &mut Builder, facts: &ProjectFacts) {
    b.add(
        None,
        "General",
        false,
        "custom-script",
        "Custom script",
        TaskKind::Script,
        "Any program with arguments, e.g. python tools/export.py.",
        "python",
        &["tools/script.py"],
    );
    b.add(
        None,
        "General",
        facts.uses_lfs,
        "git-lfs-pull",
        "Git LFS: Pull large files",
        TaskKind::Script,
        "Downloads the Git LFS files for the current checkout (textures, models, audio stored as pointers).",
        "git",
        &["lfs", "pull"],
    );
    b.add(
        None,
        "General",
        false,
        "clean-builds",
        "Clean build output",
        TaskKind::Script,
        "Deletes untracked and ignored files under Builds/ with git clean (tracked files are never touched).",
        "git",
        &["clean", "-d", "-x", "-f", "--", "Builds"],
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, contents: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }

    #[test]
    fn detects_engines_from_marker_files() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(detect_engine(dir.path()), None);
        write(&dir.path().join("scene.blend"), "");
        assert_eq!(detect_engine(dir.path()), Some("blender"));
        write(&dir.path().join("project.godot"), "");
        assert_eq!(detect_engine(dir.path()), Some("godot"));
        write(&dir.path().join("Mech.uproject"), "{}");
        assert_eq!(detect_engine(dir.path()), Some("unreal"));
        write(&dir.path().join("ProjectSettings/ProjectVersion.txt"), "");
        assert_eq!(detect_engine(dir.path()), Some("unity"));
    }

    #[test]
    fn unity_project_gets_unity_presets_first_for_the_host() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("ProjectSettings/ProjectVersion.txt"), "");
        let presets = task_presets_for(dir.path(), Host::Mac);
        let first_other = presets.iter().position(|p| !p.recommended).unwrap();
        assert!(first_other > 5);
        assert!(
            presets[..first_other]
                .iter()
                .all(|p| p.engine.as_deref() == Some("unity"))
        );
        let rec = |id: &str| {
            presets
                .iter()
                .find(|p| p.definition.id == id)
                .unwrap()
                .recommended
        };
        assert!(rec("unity-build-macos") && rec("unity-build-ios"));
        assert!(
            !rec("unity-build-win64"),
            "Windows build not recommended on a Mac"
        );
        // Not-recommended Unity presets still come before other engines.
        let win = presets
            .iter()
            .position(|p| p.definition.id == "unity-build-win64")
            .unwrap();
        let godot = presets
            .iter()
            .position(|p| p.engine.as_deref() == Some("godot"))
            .unwrap();
        assert!(win < godot);
        let windows = task_presets_for(dir.path(), Host::Windows);
        let ios = windows
            .iter()
            .find(|p| p.definition.id == "unity-build-ios")
            .unwrap();
        assert!(!ios.recommended);
    }

    #[test]
    fn unreal_presets_use_the_uproject_and_host_scripts() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("MechBay.uproject"), "{}");
        let windows = task_presets_for(dir.path(), Host::Windows);
        assert_eq!(windows[0].engine.as_deref(), Some("unreal"));
        let package = windows
            .iter()
            .find(|p| p.definition.id == "unreal-buildcookrun")
            .unwrap();
        assert!(package.definition.program.ends_with("RunUAT.bat"));
        assert!(
            package
                .definition
                .args
                .contains(&"-project=${project}/MechBay.uproject".into())
        );
        assert!(package.definition.args.contains(&"-platform=Win64".into()));
        assert!(
            !windows
                .iter()
                .any(|p| p.definition.id == "unreal-package-ios")
        );
        let mac = task_presets_for(dir.path(), Host::Mac);
        let editor = mac
            .iter()
            .find(|p| p.definition.id == "unreal-build-editor")
            .unwrap();
        assert!(editor.definition.program.ends_with("Mac/Build.sh"));
        assert_eq!(editor.definition.args[0], "MechBayEditor");
        assert!(mac.iter().any(|p| p.definition.id == "unreal-package-ios"));
    }

    #[test]
    fn godot_presets_use_names_from_export_presets() {
        let raw = "[preset.0]\n\nname=\"Steam Windows\"\nplatform=\"Windows Desktop\"\n\n[preset.0.options]\n\n[preset.1]\nname=\"Linux\"\nplatform=\"Linux/X11\"\n";
        let parsed = godot_export_presets(raw);
        assert_eq!(parsed["Windows Desktop"], "Steam Windows");
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("project.godot"), "");
        write(&dir.path().join("export_presets.cfg"), raw);
        let presets = task_presets_for(dir.path(), Host::Linux);
        let export = presets
            .iter()
            .find(|p| p.definition.id == "godot-export")
            .unwrap();
        assert!(export.recommended);
        assert!(export.definition.args.contains(&"Steam Windows".into()));
    }

    #[test]
    fn general_project_recommends_lfs_only_when_used() {
        let dir = tempfile::tempdir().unwrap();
        let presets = task_presets_for(dir.path(), Host::Linux);
        assert!(presets.iter().all(|p| !p.recommended));
        write(
            &dir.path().join(".gitattributes"),
            "*.psd filter=lfs diff=lfs merge=lfs -text\n",
        );
        let presets = task_presets_for(dir.path(), Host::Linux);
        assert_eq!(presets[0].definition.id, "git-lfs-pull");
        assert!(presets[0].recommended);
    }

    #[test]
    fn every_preset_is_a_valid_unique_task_on_every_host() {
        for host in [Host::Windows, Host::Mac, Host::Linux] {
            let definitions: Vec<_> = catalog(&ProjectFacts::default(), host)
                .into_iter()
                .map(|p| p.definition)
                .collect();
            let dir = tempfile::tempdir().unwrap();
            crate::tasks::save_tasks(dir.path(), &definitions).unwrap();
            assert!(definitions.len() >= 30, "{}", definitions.len());
        }
    }

    #[test]
    fn installs_the_unity_script_once() {
        let dir = tempfile::tempdir().unwrap();
        assert!(install_unity_build_script(dir.path()).is_err());
        std::fs::create_dir_all(dir.path().join("Assets")).unwrap();
        let path = install_unity_build_script(dir.path()).unwrap();
        assert!(path.ends_with("Assets/Editor/PipelineOSBuild.cs"));
        let script = std::fs::read_to_string(&path).unwrap();
        for method in [
            "Windows()",
            "Android()",
            "IOS()",
            "WebGL()",
            "Addressables()",
        ] {
            assert!(script.contains(method), "{method}");
        }
        assert!(script.contains("EditorUserBuildSettings.buildAppBundle"));
        std::fs::write(&path, "// mine").unwrap();
        install_unity_build_script(dir.path()).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "// mine");
    }
}
