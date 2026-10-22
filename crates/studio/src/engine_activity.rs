//! Detects engine work started *outside* Pipeline OS (lighting builds, shader
//! compiles, cooks, Live Coding, player builds, lightmap bakes) by following
//! the engines' own log files.
//!
//! Unreal writes `<Project>/Saved/Logs/<Project>.log`; Unity writes a single
//! `Editor.log` per machine. A [`LogFollower`] reads only bytes appended since
//! the last poll (it starts at the end of a file the first time, so history is
//! never replayed) and a [`LogParser`] turns lines into [`EngineEvent`]s.

use std::{
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::OnceLock,
};

use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EngineKind {
    Unreal,
    Unity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ActivityKind {
    Lighting,
    Shaders,
    Cook,
    Compile,
    Build,
    Import,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ActivityStatus {
    Running,
    Succeeded,
    Failed,
}

/// One change to an engine operation, as parsed from a log line.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineEvent {
    pub kind: ActivityKind,
    pub status: ActivityStatus,
    /// 0-100 when the log reports it.
    pub progress: Option<f32>,
    /// The log line, trimmed of engine prefixes, for display.
    pub detail: String,
}

struct Rule {
    engine: EngineKind,
    kind: ActivityKind,
    status: ActivityStatus,
    pattern: &'static str,
}

/// Recognized log lines. Ordered: the first match wins.
const RULES: &[Rule] = &[
    // ---- Unreal Engine
    Rule {
        engine: EngineKind::Unreal,
        kind: ActivityKind::Lighting,
        status: ActivityStatus::Failed,
        pattern: r"(?i)lighting build (failed|cancel+ed)|lightmass (crashed|failed)",
    },
    Rule {
        engine: EngineKind::Unreal,
        kind: ActivityKind::Lighting,
        status: ActivityStatus::Succeeded,
        pattern: r"(?i)lighting build (took|completed|finished)|LogStaticLightingSystem: .*(finished|completed)|illumination: .*total",
    },
    Rule {
        engine: EngineKind::Unreal,
        kind: ActivityKind::Lighting,
        status: ActivityStatus::Running,
        pattern: r"(?i)running lightmass|lighting build (started|starting)|LogStaticLightingSystem: (running|build)|Swarm.*(started|starting)",
    },
    Rule {
        engine: EngineKind::Unreal,
        kind: ActivityKind::Shaders,
        status: ActivityStatus::Succeeded,
        pattern: r"(?i)LogShaderCompilers: .*(compil(ed|ation) (completed|finished)|0 shader jobs remaining)",
    },
    Rule {
        engine: EngineKind::Unreal,
        kind: ActivityKind::Shaders,
        status: ActivityStatus::Running,
        pattern: r"(?i)LogShaderCompilers: .*(compiling \d+ shaders|\d+ (shader )?jobs remaining|remaining \d+)",
    },
    Rule {
        engine: EngineKind::Unreal,
        kind: ActivityKind::Cook,
        status: ActivityStatus::Failed,
        pattern: r"(?i)LogCook: .*(cook failed|error)",
    },
    Rule {
        engine: EngineKind::Unreal,
        kind: ActivityKind::Cook,
        status: ActivityStatus::Succeeded,
        pattern: r"(?i)LogCook: Display: Done!|cook (completed|finished)",
    },
    Rule {
        engine: EngineKind::Unreal,
        kind: ActivityKind::Cook,
        status: ActivityStatus::Running,
        pattern: r"(?i)LogCook: Display: (cook|cooking|\[\d+/\d+\])",
    },
    Rule {
        engine: EngineKind::Unreal,
        kind: ActivityKind::Compile,
        status: ActivityStatus::Failed,
        pattern: r"(?i)live ?coding.*(failed|error)",
    },
    Rule {
        engine: EngineKind::Unreal,
        kind: ActivityKind::Compile,
        status: ActivityStatus::Succeeded,
        pattern: r"(?i)live ?coding.*(succeeded|success)|LogHotReload: .*(succeeded|complete)",
    },
    Rule {
        engine: EngineKind::Unreal,
        kind: ActivityKind::Compile,
        status: ActivityStatus::Running,
        pattern: r"(?i)live ?coding.*(starting|compiling)|LogHotReload: .*(starting|compiling)",
    },
    Rule {
        engine: EngineKind::Unreal,
        kind: ActivityKind::Build,
        status: ActivityStatus::Failed,
        pattern: r"(?i)BUILD FAILED|AutomationTool exiting with ExitCode=[1-9]",
    },
    Rule {
        engine: EngineKind::Unreal,
        kind: ActivityKind::Build,
        status: ActivityStatus::Succeeded,
        pattern: r"(?i)BUILD SUCCESSFUL|AutomationTool exiting with ExitCode=0",
    },
    Rule {
        engine: EngineKind::Unreal,
        kind: ActivityKind::Build,
        status: ActivityStatus::Running,
        pattern: r"(?i)Parsing command line: BuildCookRun|LogUAT.*BuildCookRun",
    },
    // ---- Unity
    Rule {
        engine: EngineKind::Unity,
        kind: ActivityKind::Build,
        status: ActivityStatus::Failed,
        pattern: r"(?i)build completed with a result of '(failed|cancelled)'",
    },
    Rule {
        engine: EngineKind::Unity,
        kind: ActivityKind::Build,
        status: ActivityStatus::Succeeded,
        pattern: r"(?i)build completed with a result of 'succeeded'",
    },
    Rule {
        engine: EngineKind::Unity,
        kind: ActivityKind::Build,
        status: ActivityStatus::Running,
        pattern: r"(?i)^\s*(building player|build started|BuildPlayer:)|\[BuildPlayer\]",
    },
    Rule {
        engine: EngineKind::Unity,
        kind: ActivityKind::Lighting,
        status: ActivityStatus::Failed,
        pattern: r"(?i)(lightmap|lighting|bake).*(failed|cancel+ed)",
    },
    Rule {
        engine: EngineKind::Unity,
        kind: ActivityKind::Lighting,
        status: ActivityStatus::Succeeded,
        pattern: r"(?i)(bake|baking|lightmapping) (finished|completed|done)",
    },
    Rule {
        engine: EngineKind::Unity,
        kind: ActivityKind::Lighting,
        status: ActivityStatus::Running,
        pattern: r"(?i)(started|starting) (lightmap|lighting) bak|baking lighting|\[pathtracing\]",
    },
    Rule {
        engine: EngineKind::Unity,
        kind: ActivityKind::Compile,
        status: ActivityStatus::Failed,
        pattern: r"(?i)(script|scripts) .*compil\w* (failed|errors?)|compilation failed",
    },
    Rule {
        engine: EngineKind::Unity,
        kind: ActivityKind::Compile,
        status: ActivityStatus::Succeeded,
        pattern: r"(?i)tundra build success|finished compiling graph|compilation finished",
    },
    Rule {
        engine: EngineKind::Unity,
        kind: ActivityKind::Compile,
        status: ActivityStatus::Running,
        pattern: r"(?i)starting: .*(csc|bee_backend)|requested script compilation",
    },
    Rule {
        engine: EngineKind::Unity,
        kind: ActivityKind::Import,
        status: ActivityStatus::Succeeded,
        pattern: r"(?i)^refresh completed in",
    },
];

fn compiled() -> &'static Vec<(usize, Regex)> {
    static CELL: OnceLock<Vec<(usize, Regex)>> = OnceLock::new();
    CELL.get_or_init(|| {
        RULES
            .iter()
            .enumerate()
            .map(|(i, rule)| (i, Regex::new(rule.pattern).expect("valid rule")))
            .collect()
    })
}

fn progress_in(line: &str) -> Option<f32> {
    static PERCENT: OnceLock<Regex> = OnceLock::new();
    static FRACTION: OnceLock<Regex> = OnceLock::new();
    let percent = PERCENT.get_or_init(|| Regex::new(r"(\d{1,3}(?:\.\d+)?)\s?%").expect("regex"));
    let fraction = FRACTION.get_or_init(|| Regex::new(r"\[(\d+)\s*/\s*(\d+)\]").expect("regex"));
    if let Some(caps) = percent.captures(line) {
        let value: f32 = caps[1].parse().ok()?;
        return (0.0..=100.0).contains(&value).then_some(value);
    }
    let caps = fraction.captures(line)?;
    let done: f32 = caps[1].parse().ok()?;
    let total: f32 = caps[2].parse().ok()?;
    (total > 0.0 && done <= total).then(|| (done * 1000.0 / total).round() / 10.0)
}

/// Strips `[2026.10.22-13.00.00:123][ 45]` style prefixes.
fn clean(line: &str) -> String {
    let mut rest = line.trim();
    while rest.starts_with('[') {
        match rest.find(']') {
            Some(end) => rest = rest[end + 1..].trim_start(),
            None => break,
        }
    }
    rest.chars().take(240).collect()
}

/// Turns log lines from one engine into events.
#[derive(Debug, Clone)]
pub struct LogParser {
    engine: EngineKind,
}

impl LogParser {
    pub fn new(engine: EngineKind) -> Self {
        Self { engine }
    }

    pub fn parse_line(&self, line: &str) -> Option<EngineEvent> {
        let (index, _) = compiled()
            .iter()
            .filter(|(index, _)| RULES[*index].engine == self.engine)
            .find(|(_, regex)| regex.is_match(line))?;
        let rule = &RULES[*index];
        Some(EngineEvent {
            kind: rule.kind,
            status: rule.status,
            progress: if rule.status == ActivityStatus::Running {
                progress_in(line)
            } else {
                None
            },
            detail: clean(line),
        })
    }
}

/// Follows a growing log file, returning only lines appended since the last
/// call. Handles truncation and rotation (the engine recreating its log on
/// launch) by starting again from the beginning of the new file.
#[derive(Debug)]
pub struct LogFollower {
    path: PathBuf,
    offset: Option<u64>,
    partial: String,
}

impl LogFollower {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            offset: None,
            partial: String::new(),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn poll(&mut self) -> Vec<String> {
        let Ok(mut file) = std::fs::File::open(&self.path) else {
            // Missing now: when it reappears it's a new session, read it all.
            self.offset = Some(0);
            return Vec::new();
        };
        let len = file.metadata().map(|m| m.len()).unwrap_or(0);
        let start = match self.offset {
            // First sight: skip history.
            None => {
                self.offset = Some(len);
                return Vec::new();
            }
            Some(offset) if len < offset => {
                self.partial.clear();
                0
            }
            Some(offset) => offset,
        };
        if len == start {
            return Vec::new();
        }
        // Read at most 4 MiB per poll to stay responsive on log floods.
        let to_read = (len - start).min(4 * 1024 * 1024);
        let mut buffer = vec![0; to_read as usize];
        if file.seek(SeekFrom::Start(start)).is_err() || file.read_exact(&mut buffer).is_err() {
            return Vec::new();
        }
        self.offset = Some(start + to_read);
        let text = String::from_utf8_lossy(&buffer);
        let combined = std::mem::take(&mut self.partial) + &text;
        let mut lines: Vec<String> = combined
            .split('\n')
            .map(|l| l.trim_end_matches('\r').to_owned())
            .collect();
        // Keep an unterminated last line for the next poll.
        self.partial = lines.pop().unwrap_or_default();
        lines
    }
}

/// The Unity Editor log for this machine.
pub fn unity_editor_log() -> Option<PathBuf> {
    if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA")
            .map(|dir| PathBuf::from(dir).join("Unity/Editor/Editor.log"))
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME")
            .map(|home| PathBuf::from(home).join("Library/Logs/Unity/Editor.log"))
    } else {
        std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config/unity3d/Editor.log"))
    }
}

/// The log an Unreal project writes while open in the editor.
pub fn unreal_project_log(project_root: &Path) -> Option<PathBuf> {
    let uproject = std::fs::read_dir(project_root)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| p.extension().and_then(|e| e.to_str()) == Some("uproject"))?;
    let name = uproject.file_stem()?.to_string_lossy().into_owned();
    Some(project_root.join("Saved/Logs").join(format!("{name}.log")))
}

/// Unity logs the project it opened; used to attribute Editor.log events.
pub fn unity_project_from_line(line: &str) -> Option<PathBuf> {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    let pattern = PATTERN.get_or_init(|| {
        Regex::new(r#"(?i)(?:successfully changed project path to:|-projectpath\s+)"?([^"\r\n]+?)"?(?:\s+-|\s*$)"#)
            .expect("regex")
    });
    pattern
        .captures(line)
        .map(|caps| PathBuf::from(caps[1].trim()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unreal(line: &str) -> Option<(ActivityKind, ActivityStatus)> {
        LogParser::new(EngineKind::Unreal)
            .parse_line(line)
            .map(|e| (e.kind, e.status))
    }

    fn unity(line: &str) -> Option<(ActivityKind, ActivityStatus)> {
        LogParser::new(EngineKind::Unity)
            .parse_line(line)
            .map(|e| (e.kind, e.status))
    }

    #[test]
    fn recognizes_unreal_operations() {
        assert_eq!(
            unreal(
                "[2026.10.22-13.00.00:123][  0]LogStaticLightingSystem: Running Lightmass w/ ImmediateImport mode ENABLED"
            ),
            Some((ActivityKind::Lighting, ActivityStatus::Running))
        );
        assert_eq!(
            unreal("LogStaticLightingSystem: Lighting build took 252.3 seconds"),
            Some((ActivityKind::Lighting, ActivityStatus::Succeeded))
        );
        assert_eq!(
            unreal("LightingResults: Error: Lighting build failed. Swarm failed to kick off."),
            Some((ActivityKind::Lighting, ActivityStatus::Failed))
        );
        assert_eq!(
            unreal("LogShaderCompilers: Display: Compiling 1520 shaders"),
            Some((ActivityKind::Shaders, ActivityStatus::Running))
        );
        assert_eq!(
            unreal("LogCook: Display: Done!"),
            Some((ActivityKind::Cook, ActivityStatus::Succeeded))
        );
        assert_eq!(
            unreal("LogLiveCoding: Display: Starting Live Coding compile."),
            Some((ActivityKind::Compile, ActivityStatus::Running))
        );
        assert_eq!(
            unreal("LogLiveCoding: Display: Live coding succeeded"),
            Some((ActivityKind::Compile, ActivityStatus::Succeeded))
        );
        assert_eq!(unreal("LogTemp: Hello world"), None);
    }

    #[test]
    fn recognizes_unity_operations() {
        assert_eq!(
            unity("Building Player"),
            Some((ActivityKind::Build, ActivityStatus::Running))
        );
        assert_eq!(
            unity("Build completed with a result of 'Succeeded' in 84 seconds (84120 ms)"),
            Some((ActivityKind::Build, ActivityStatus::Succeeded))
        );
        assert_eq!(
            unity("Build completed with a result of 'Failed' in 3 seconds"),
            Some((ActivityKind::Build, ActivityStatus::Failed))
        );
        assert_eq!(
            unity("*** Tundra build success (2.31 seconds), 12 items updated"),
            Some((ActivityKind::Compile, ActivityStatus::Succeeded))
        );
        assert_eq!(unity("UnityEngine.Debug:Log (object)"), None);
    }

    #[test]
    fn reads_progress_and_strips_prefixes() {
        let event = LogParser::new(EngineKind::Unreal)
            .parse_line("[2026.10.22-13.00.00:123][ 45]LogCook: Display: [120/400] cooking /Game/Maps/Reactor")
            .unwrap();
        assert_eq!(event.progress, Some(30.0));
        assert!(event.detail.starts_with("LogCook"));
    }

    #[test]
    fn follower_skips_history_and_handles_rotation() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Game.log");
        std::fs::write(&path, "old line\n").unwrap();
        let mut follower = LogFollower::new(path.clone());
        assert!(follower.poll().is_empty(), "history is skipped");
        let append = |text: &str| {
            use std::io::Write;
            std::fs::OpenOptions::new()
                .append(true)
                .open(&path)
                .unwrap()
                .write_all(text.as_bytes())
                .unwrap();
        };
        append("first\nsecond partial");
        assert_eq!(follower.poll(), vec!["first"]);
        append(" done\n");
        assert_eq!(follower.poll(), vec!["second partial done"]);
        // The engine relaunches and recreates its log.
        std::fs::write(&path, "new session\n").unwrap();
        assert_eq!(follower.poll(), vec!["new session"]);
    }

    #[test]
    fn finds_unity_project_paths_in_log_lines() {
        assert_eq!(
            unity_project_from_line(
                "Successfully changed project path to: /Users/ana/Projects/Voidline"
            ),
            Some(PathBuf::from("/Users/ana/Projects/Voidline"))
        );
        assert_eq!(
            unity_project_from_line(
                "COMMAND LINE ARGUMENTS: Unity -projectpath \"C:/Work/Helix\" -useHub"
            ),
            Some(PathBuf::from("C:/Work/Helix"))
        );
    }

    #[test]
    fn finds_unreal_project_logs() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("MechBay.uproject"), "{}").unwrap();
        assert_eq!(
            unreal_project_log(dir.path()),
            Some(dir.path().join("Saved/Logs/MechBay.log"))
        );
    }
}
