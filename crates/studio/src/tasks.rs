//! Pipeline tasks: named, repeatable project commands such as engine builds,
//! renders, exports, and tests.
//!
//! Tasks live in `.pipelineos/tasks.toml`, shared through version control.
//! Like launch profiles they are an argument vector, never a shell string, and
//! because running a shared file's commands is code execution, callers gate
//! runs behind an explicit trust decision keyed by [`tasks_fingerprint`].

use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{StudioError, metadata_path, validate_id, write_atomic};

pub const TASKS_FILE: &str = "tasks.toml";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum TaskKind {
    Build,
    Test,
    Render,
    Export,
    Cook,
    #[default]
    Script,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskDefinition {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub kind: TaskKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Program to run. May be `${engine}` (the project's resolved engine
    /// executable) or any path/command; variables are expanded first.
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
    /// Working directory, relative to the project root (default: the root).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TasksFile {
    #[serde(default = "schema_version")]
    schema_version: u32,
    #[serde(default, rename = "task")]
    tasks: Vec<TaskDefinition>,
}

fn schema_version() -> u32 {
    1
}

fn tasks_path(root: &Path) -> PathBuf {
    metadata_path(root, TASKS_FILE)
}

/// Loads the project's tasks; a missing file means no tasks.
pub fn load_tasks(root: &Path) -> Result<Vec<TaskDefinition>, StudioError> {
    let path = tasks_path(root);
    let raw = match std::fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(StudioError::io(&path, error)),
    };
    let file: TasksFile = toml::from_str(&raw).map_err(|e| StudioError::Parse {
        path: path.clone(),
        message: e.to_string(),
    })?;
    validate_tasks(&file.tasks)?;
    Ok(file.tasks)
}

/// Saves the full task list, replacing the file atomically.
pub fn save_tasks(root: &Path, tasks: &[TaskDefinition]) -> Result<(), StudioError> {
    validate_tasks(tasks)?;
    let file = TasksFile {
        schema_version: schema_version(),
        tasks: tasks.to_vec(),
    };
    let body = toml::to_string_pretty(&file).map_err(|e| StudioError::Invalid(e.to_string()))?;
    let header = "# Pipeline OS tasks: repeatable project commands (builds, renders, exports).\n\
                  # Each task is an argument vector, never a shell string. Variables:\n\
                  # ${project} project root, ${engine} resolved engine executable,\n\
                  # ${engineDir} engine install root (Unreal: the folder holding Engine/),\n\
                  # ${projectName}, ${date} (YYYY-MM-DD), ${time} (HHMMSS).\n\n";
    write_atomic(&tasks_path(root), &format!("{header}{body}"))
}

fn validate_tasks(tasks: &[TaskDefinition]) -> Result<(), StudioError> {
    let mut seen = std::collections::BTreeSet::new();
    for task in tasks {
        validate_id(&task.id)?;
        if !seen.insert(task.id.as_str()) {
            return Err(StudioError::Invalid(format!(
                "duplicate task id {:?}",
                task.id
            )));
        }
        if task.name.trim().is_empty() || task.program.trim().is_empty() {
            return Err(StudioError::Invalid(format!(
                "task {:?} needs a name and a program",
                task.id
            )));
        }
        if let Some(cwd) = &task.cwd
            && (Path::new(cwd).is_absolute() || cwd.split(['/', '\\']).any(|part| part == ".."))
        {
            return Err(StudioError::Invalid(format!(
                "task {:?}: cwd must stay inside the project",
                task.id
            )));
        }
    }
    Ok(())
}

/// Content fingerprint of the tasks file. A trust decision is recorded against
/// this value, so any edit to the commands (e.g. from a pull) requires the
/// user to review and trust them again.
pub fn tasks_fingerprint(root: &Path) -> Option<String> {
    let bytes = std::fs::read(tasks_path(root)).ok()?;
    let digest = Sha256::digest(&bytes);
    Some(digest.iter().map(|b| format!("{b:02x}")).collect())
}

/// Per-machine record of which projects' task files the user has reviewed,
/// keyed by project path and pinned to the file's content fingerprint. Stored
/// in the app data folder (never in the project) and shared by the desktop
/// app, the CLI, and the MCP server.
#[derive(Debug, Default)]
pub struct TrustStore {
    path: Option<PathBuf>,
    entries: BTreeMap<String, String>,
}

pub const TRUST_FILE: &str = "trusted-tasks.json";

impl TrustStore {
    pub fn load(data_dir: &Path) -> Self {
        let path = data_dir.join(TRUST_FILE);
        let entries = std::fs::read_to_string(&path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default();
        Self {
            path: Some(path),
            entries,
        }
    }

    /// Whether the project's *current* tasks file is trusted.
    pub fn is_trusted(&self, root: &Path) -> bool {
        tasks_fingerprint(root).is_some_and(|fp| self.entries.get(&trust_key(root)) == Some(&fp))
    }

    /// Trusts `fingerprint` for the project. Fails when the file no longer
    /// matches what the user reviewed.
    pub fn trust(&mut self, root: &Path, fingerprint: &str) -> Result<(), StudioError> {
        if tasks_fingerprint(root).as_deref() != Some(fingerprint) {
            return Err(StudioError::Invalid(
                "the tasks file changed while it was being reviewed; review it again".into(),
            ));
        }
        self.entries.insert(trust_key(root), fingerprint.to_owned());
        self.save()
    }

    fn save(&self) -> Result<(), StudioError> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let json = serde_json::to_string_pretty(&self.entries)
            .map_err(|e| StudioError::Invalid(e.to_string()))?;
        write_atomic(path, &json)
    }
}

/// Normalized trust key so `D:\Game` and `D:/Game/` match.
fn trust_key(root: &Path) -> String {
    root.display()
        .to_string()
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_owned()
}

/// Ready-made tasks for common engines and DCCs, offered when adding a task:
/// every preset's definition (see [`crate::task_presets`] for the grouped,
/// project-aware list).
pub fn task_templates() -> Vec<TaskDefinition> {
    crate::task_presets::all_preset_definitions()
}

/// The engine's install root from its executable: for Unreal the folder that
/// contains `Engine/`, for a macOS app bundle the folder holding the `.app`,
/// otherwise the executable's folder.
fn engine_dir(engine: &Path) -> Option<PathBuf> {
    if let Some(engine_folder) = engine
        .ancestors()
        .find(|dir| dir.file_name().is_some_and(|name| name == "Engine"))
    {
        return engine_folder.parent().map(Path::to_path_buf);
    }
    if let Some(bundle) = engine.ancestors().find(|dir| {
        dir.extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("app"))
    }) {
        return bundle.parent().map(Path::to_path_buf);
    }
    engine.parent().map(Path::to_path_buf)
}

/// Values substituted into `${name}` placeholders.
#[derive(Debug, Clone, Default)]
pub struct TaskVariables {
    pub values: BTreeMap<String, String>,
}

impl TaskVariables {
    pub fn for_project(root: &Path, project_name: &str, engine: Option<&Path>) -> Self {
        let now = chrono::Local::now();
        let mut values = BTreeMap::new();
        values.insert("project".into(), root.display().to_string());
        values.insert("projectName".into(), project_name.to_owned());
        values.insert("date".into(), now.format("%Y-%m-%d").to_string());
        values.insert("time".into(), now.format("%H%M%S").to_string());
        if let Some(engine) = engine {
            values.insert("engine".into(), engine.display().to_string());
            if let Some(dir) = engine_dir(engine) {
                values.insert("engineDir".into(), dir.display().to_string());
            }
        }
        Self { values }
    }

    fn expand(&self, input: &str) -> Result<String, StudioError> {
        let pattern = Regex::new(r"\$\{([A-Za-z][A-Za-z0-9_]*)\}").expect("valid regex");
        let mut missing = None;
        let out = pattern.replace_all(input, |caps: &regex::Captures| {
            let key = &caps[1];
            match self.values.get(key) {
                Some(value) => value.clone(),
                None => {
                    missing.get_or_insert_with(|| key.to_owned());
                    String::new()
                }
            }
        });
        match missing {
            Some(key) if key == "engine" || key == "engineDir" => Err(StudioError::Invalid(
                "this task uses ${engine}, but no engine executable was found for the project; link an engine or set its version".into(),
            )),
            Some(key) => Err(StudioError::Invalid(format!("unknown variable ${{{key}}}"))),
            None => Ok(out.into_owned()),
        }
    }
}

/// A task with every variable expanded, ready to spawn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedTask {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub env: BTreeMap<String, String>,
}

impl ResolvedTask {
    /// A human-readable command line for previews and logs (display only).
    pub fn display(&self) -> String {
        std::iter::once(&self.program)
            .chain(self.args.iter())
            .map(|part| {
                if part.contains(' ') || part.is_empty() {
                    format!("\"{part}\"")
                } else {
                    part.clone()
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }
}

pub fn resolve_task(
    root: &Path,
    task: &TaskDefinition,
    vars: &TaskVariables,
) -> Result<ResolvedTask, StudioError> {
    let program = vars.expand(&task.program)?;
    let args = task
        .args
        .iter()
        .map(|arg| vars.expand(arg))
        .collect::<Result<Vec<_>, _>>()?;
    let cwd = match &task.cwd {
        Some(cwd) => root.join(vars.expand(cwd)?),
        None => root.to_path_buf(),
    };
    let env = task
        .env
        .iter()
        .map(|(k, v)| Ok((k.clone(), vars.expand(v)?)))
        .collect::<Result<BTreeMap<_, _>, StudioError>>()?;
    Ok(ResolvedTask {
        program,
        args,
        cwd,
        env,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputStream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RunStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunOutcome {
    pub status: RunStatus,
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
}

/// Extracts a 0-100 progress value from a log line when a tool reports one
/// (e.g. "Progress: 42%", Blender's "Fra:12 ... | 12/240", UAT "[12/340]").
pub fn parse_progress(line: &str) -> Option<f32> {
    static PERCENT: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    static FRACTION: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let percent = PERCENT.get_or_init(|| Regex::new(r"(\d{1,3}(?:\.\d+)?)\s?%").expect("regex"));
    let fraction = FRACTION.get_or_init(|| Regex::new(r"\[(\d+)\s*/\s*(\d+)\]").expect("regex"));
    if let Some(caps) = percent.captures(line) {
        let value: f32 = caps[1].parse().ok()?;
        return (0.0..=100.0).contains(&value).then_some(value);
    }
    let caps = fraction.captures(line)?;
    let done: f32 = caps[1].parse().ok()?;
    let total: f32 = caps[2].parse().ok()?;
    (total > 0.0 && done <= total).then(|| done / total * 100.0)
}

/// Runs a resolved task to completion, streaming each output line to
/// `on_line` as it arrives. Setting `cancel` stops the whole process tree.
/// Blocking: call from a worker thread.
pub fn run_task(
    task: &ResolvedTask,
    cancel: Arc<AtomicBool>,
    mut on_line: impl FnMut(OutputStream, &str),
) -> Result<RunOutcome, StudioError> {
    let started = Instant::now();
    let mut command = Command::new(&task.program);
    command
        .args(&task.args)
        .current_dir(&task.cwd)
        .envs(&task.env)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW: tool output is captured; don't flash a console.
        command.creation_flags(0x0800_0000);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // Own process group, so cancelling can signal the whole tree.
        command.process_group(0);
    }
    let mut child = command.spawn().map_err(|error| {
        StudioError::Invalid(format!("could not start {:?}: {error}", task.program))
    })?;

    let (sender, receiver) = mpsc::channel::<(OutputStream, String)>();
    let mut readers = Vec::new();
    if let Some(stdout) = child.stdout.take() {
        readers.push(spawn_reader(stdout, OutputStream::Stdout, sender.clone()));
    }
    if let Some(stderr) = child.stderr.take() {
        readers.push(spawn_reader(stderr, OutputStream::Stderr, sender.clone()));
    }
    drop(sender);

    let mut cancelled = false;
    let status = loop {
        match receiver.recv_timeout(Duration::from_millis(100)) {
            Ok((stream, line)) => on_line(stream, &line),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            // Both pipes closed but the process is still exiting: don't spin.
            Err(mpsc::RecvTimeoutError::Disconnected) => thread::sleep(Duration::from_millis(50)),
        }
        if !cancelled && cancel.load(Ordering::SeqCst) {
            cancelled = true;
            kill_tree(&mut child);
        }
        if let Some(status) = child
            .try_wait()
            .map_err(|e| StudioError::Invalid(format!("waiting for task: {e}")))?
        {
            break status;
        }
    };
    // After a cancel, a stray grandchild may still hold a pipe open; don't
    // block on it — the readers exit on their own once it does.
    if !cancelled {
        for reader in readers {
            let _ = reader.join();
        }
    }
    while let Ok((stream, line)) = receiver.try_recv() {
        on_line(stream, &line);
    }
    let exit_code = status.code();
    let status = if cancelled {
        RunStatus::Cancelled
    } else if status.success() {
        RunStatus::Succeeded
    } else {
        RunStatus::Failed
    };
    Ok(RunOutcome {
        status,
        exit_code,
        duration_ms: started.elapsed().as_millis() as u64,
    })
}

fn spawn_reader(
    stream: impl Read + Send + 'static,
    kind: OutputStream,
    sender: mpsc::Sender<(OutputStream, String)>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut reader = BufReader::new(stream);
        let mut buffer = Vec::new();
        loop {
            buffer.clear();
            match reader.read_until(b'\n', &mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    let line = String::from_utf8_lossy(&buffer);
                    let line = line.trim_end_matches(['\r', '\n']);
                    if sender.send((kind, line.to_owned())).is_err() {
                        break;
                    }
                }
            }
        }
    })
}

/// Engine builds spawn child processes (compilers, shader workers), so a plain
/// kill would orphan them. Kill the tree: taskkill on Windows, the process
/// group on Unix.
fn kill_tree(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        let _ = Command::new("kill")
            .args(["-KILL", "--", &format!("-{}", child.id())])
            .status();
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = Command::new("taskkill")
            .args(["/T", "/F", "/PID", &child.id().to_string()])
            .creation_flags(0x0800_0000)
            .status();
    }
    let _ = child.kill();
}

/// Keeps the last `capacity` lines of a run's log for display and history.
#[derive(Debug, Clone, Default, Serialize)]
pub struct LogTail {
    capacity: usize,
    pub lines: std::collections::VecDeque<String>,
    pub dropped: usize,
}

impl LogTail {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            lines: Default::default(),
            dropped: 0,
        }
    }

    pub fn push(&mut self, line: &str) {
        if self.lines.len() == self.capacity {
            self.lines.pop_front();
            self.dropped += 1;
        }
        self.lines.push_back(line.to_owned());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(id: &str) -> TaskDefinition {
        TaskDefinition {
            id: id.into(),
            name: "Build".into(),
            kind: TaskKind::Build,
            description: None,
            program: "${engine}".into(),
            args: vec!["-projectPath".into(), "${project}".into()],
            cwd: None,
            env: BTreeMap::new(),
        }
    }

    #[test]
    fn tasks_round_trip_through_toml() {
        let dir = tempfile::tempdir().unwrap();
        assert!(load_tasks(dir.path()).unwrap().is_empty());
        let tasks = vec![sample("build"), sample("test")];
        save_tasks(dir.path(), &tasks).unwrap();
        assert_eq!(load_tasks(dir.path()).unwrap(), tasks);
        let raw = std::fs::read_to_string(dir.path().join(".pipelineos/tasks.toml")).unwrap();
        assert!(raw.contains("[[task]]"), "{raw}");
    }

    #[test]
    fn rejects_duplicates_and_escaping_cwd() {
        let dir = tempfile::tempdir().unwrap();
        assert!(save_tasks(dir.path(), &[sample("a"), sample("a")]).is_err());
        let mut escaping = sample("b");
        escaping.cwd = Some("../outside".into());
        assert!(save_tasks(dir.path(), &[escaping]).is_err());
    }

    #[test]
    fn fingerprint_changes_when_commands_change() {
        let dir = tempfile::tempdir().unwrap();
        assert!(tasks_fingerprint(dir.path()).is_none());
        save_tasks(dir.path(), &[sample("a")]).unwrap();
        let first = tasks_fingerprint(dir.path()).unwrap();
        let mut changed = sample("a");
        changed.args.push("-evil".into());
        save_tasks(dir.path(), &[changed]).unwrap();
        assert_ne!(first, tasks_fingerprint(dir.path()).unwrap());
    }

    #[test]
    fn resolves_variables_and_reports_missing_engine() {
        let root = Path::new("/work/Game");
        let vars = TaskVariables::for_project(root, "Game", Some(Path::new("/apps/unity")));
        let resolved = resolve_task(root, &sample("a"), &vars).unwrap();
        assert_eq!(resolved.program, "/apps/unity");
        assert_eq!(resolved.args[1], "/work/Game");
        let unreal = TaskVariables::for_project(
            root,
            "Game",
            Some(Path::new("/epic/UE_5.4/Engine/Binaries/Linux/UnrealEditor")),
        );
        assert_eq!(unreal.values["engineDir"], "/epic/UE_5.4");
        let no_engine = TaskVariables::for_project(root, "Game", None);
        let error = resolve_task(root, &sample("a"), &no_engine).unwrap_err();
        assert!(error.to_string().contains("engine"), "{error}");
    }

    #[test]
    fn trust_is_pinned_to_content_and_persisted() {
        let project = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        save_tasks(project.path(), &[sample("a")]).unwrap();
        let mut store = TrustStore::load(data.path());
        assert!(!store.is_trusted(project.path()));
        assert!(store.trust(project.path(), "stale").is_err());
        let fp = tasks_fingerprint(project.path()).unwrap();
        store.trust(project.path(), &fp).unwrap();
        assert!(TrustStore::load(data.path()).is_trusted(project.path()));
        let mut changed = sample("a");
        changed.program = "evil".into();
        save_tasks(project.path(), &[changed]).unwrap();
        assert!(
            !TrustStore::load(data.path()).is_trusted(project.path()),
            "edits revoke trust"
        );
    }

    #[test]
    fn every_template_is_valid() {
        let templates = task_templates();
        validate_tasks(&templates).unwrap();
        assert!(templates.len() >= 30);
    }

    #[test]
    fn parses_progress_from_common_formats() {
        assert_eq!(parse_progress("Progress: 42%"), Some(42.0));
        assert_eq!(parse_progress("[3/4] Compile Foo.cpp"), Some(75.0));
        assert_eq!(parse_progress("no numbers here"), None);
        assert_eq!(parse_progress("250%"), None);
    }

    #[cfg(unix)]
    #[test]
    fn runs_and_streams_output() {
        let dir = tempfile::tempdir().unwrap();
        let task = ResolvedTask {
            program: "sh".into(),
            args: vec!["-c".into(), "echo one; echo two 1>&2; exit 3".into()],
            cwd: dir.path().to_path_buf(),
            env: BTreeMap::new(),
        };
        let mut lines = Vec::new();
        let outcome = run_task(&task, Arc::new(AtomicBool::new(false)), |stream, line| {
            lines.push((stream, line.to_owned()))
        })
        .unwrap();
        assert_eq!(outcome.status, RunStatus::Failed);
        assert_eq!(outcome.exit_code, Some(3));
        assert!(lines.contains(&(OutputStream::Stdout, "one".into())));
        assert!(lines.contains(&(OutputStream::Stderr, "two".into())));
    }

    #[cfg(unix)]
    #[test]
    fn cancellation_stops_the_process() {
        let dir = tempfile::tempdir().unwrap();
        let task = ResolvedTask {
            program: "sh".into(),
            args: vec!["-c".into(), "echo start; sleep 30".into()],
            cwd: dir.path().to_path_buf(),
            env: BTreeMap::new(),
        };
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        let outcome = run_task(&task, cancel, move |_, line| {
            if line == "start" {
                flag.store(true, Ordering::SeqCst);
            }
        })
        .unwrap();
        assert_eq!(outcome.status, RunStatus::Cancelled);
        assert!(outcome.duration_ms < 10_000);
    }

    #[test]
    fn log_tail_keeps_the_most_recent_lines() {
        let mut tail = LogTail::new(2);
        for line in ["a", "b", "c"] {
            tail.push(line);
        }
        assert_eq!(tail.lines, ["b", "c"]);
        assert_eq!(tail.dropped, 1);
    }
}
