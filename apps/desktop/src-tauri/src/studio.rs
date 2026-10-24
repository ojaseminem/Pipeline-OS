//! Desktop commands for studio workflows: pipeline task runs (with live
//! output and a persistent history), task trust, the issue board, AI project
//! briefs, folder layouts and the asset library, per-project terminals and AI
//! agents, and the bundled CLI/MCP integration details.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};
use vantadeck_studio::{
    assets::{self, AssetIndex, AssetMeta, FolderLayout, LayoutResult},
    issues::{self, Issue, NewIssue},
    task_presets::{self, TaskPreset},
    tasks::{self, LogTail, OutputStream, RunStatus, TaskDefinition, TrustStore},
    toml_merge::{self, FileMergeReport, Side},
    trackers::{self, SyncSummary, TrackerConfig},
};

use crate::DesktopState;

const RUN_HISTORY_FILE: &str = "task-runs.json";
const MAX_HISTORY: usize = 60;
const LOG_LINES: usize = 2_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskRun {
    pub id: String,
    pub project_path: String,
    pub project_name: String,
    pub task_id: String,
    pub task_name: String,
    pub kind: tasks::TaskKind,
    pub command_line: String,
    pub status: RunStatus,
    pub progress: Option<f32>,
    pub last_line: Option<String>,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub exit_code: Option<i32>,
    pub duration_ms: Option<u64>,
}

#[derive(Default)]
pub struct StudioState {
    runs: Mutex<Vec<TaskRun>>,
    logs: Mutex<BTreeMap<String, LogTail>>,
    cancels: Mutex<BTreeMap<String, Arc<AtomicBool>>>,
    trusted: Mutex<TrustStore>,
    data_dir: Mutex<Option<PathBuf>>,
}

impl StudioState {
    /// Loads persisted run history and trust decisions from the app data dir.
    pub fn load(data_dir: &Path) -> Self {
        let read = |name: &str| std::fs::read_to_string(data_dir.join(name)).ok();
        let mut runs: Vec<TaskRun> = read(RUN_HISTORY_FILE)
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default();
        // A run still marked running belonged to a previous session that ended.
        for run in &mut runs {
            if matches!(run.status, RunStatus::Running | RunStatus::Queued) {
                run.status = RunStatus::Cancelled;
                run.last_line =
                    Some("Interrupted: Pipeline OS was closed while this task was running.".into());
            }
        }
        Self {
            runs: Mutex::new(runs),
            trusted: Mutex::new(TrustStore::load(data_dir)),
            data_dir: Mutex::new(Some(data_dir.to_path_buf())),
            ..Default::default()
        }
    }

    fn persist_runs(&self) {
        let Some(dir) = self.data_dir.lock().unwrap().clone() else {
            return;
        };
        let runs = self.runs.lock().unwrap().clone();
        if let Ok(json) = serde_json::to_string(&runs) {
            let _ = std::fs::create_dir_all(&dir);
            let _ = std::fs::write(dir.join(RUN_HISTORY_FILE), json);
        }
    }

    fn update_run(&self, id: &str, apply: impl FnOnce(&mut TaskRun)) -> Option<TaskRun> {
        let mut runs = self.runs.lock().unwrap();
        let run = runs.iter_mut().find(|run| run.id == id)?;
        apply(run);
        Some(run.clone())
    }
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn root(path: &str) -> PathBuf {
    PathBuf::from(path)
}

fn err(error: impl std::fmt::Display) -> String {
    error.to_string()
}

// ---------------------------------------------------------------- tasks

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectTasks {
    tasks: Vec<TaskDefinition>,
    /// Content hash of tasks.toml; `None` when the project has no tasks file.
    fingerprint: Option<String>,
    /// Whether the user has reviewed and trusted this exact tasks file.
    trusted: bool,
}

#[tauri::command]
pub async fn tasks_list(
    path: String,
    studio: State<'_, StudioState>,
) -> Result<ProjectTasks, String> {
    let root = root(&path);
    let tasks = tasks::load_tasks(&root).map_err(err)?;
    let fingerprint = tasks::tasks_fingerprint(&root);
    let trusted = studio.trusted.lock().unwrap().is_trusted(&root);
    Ok(ProjectTasks {
        tasks,
        fingerprint,
        trusted,
    })
}

#[tauri::command]
pub async fn tasks_templates() -> Vec<TaskDefinition> {
    tasks::task_templates()
}

/// Grouped task presets for the project, the ones that fit its engine and
/// this computer first (`recommended`).
#[tauri::command]
pub async fn tasks_presets(path: String) -> Vec<TaskPreset> {
    task_presets::task_presets(&root(&path))
}

/// Saves the project's own presets (`.pipelineos/presets.toml`). Presets are
/// templates only, so no trust decision is involved.
#[tauri::command]
pub async fn presets_save(path: String, presets: Vec<TaskDefinition>) -> Result<(), String> {
    let root = root(&path);
    // The picker skips an unreadable presets.toml; never overwrite it with a
    // list built without its contents.
    tasks::load_project_presets(&root).map_err(err)?;
    tasks::save_project_presets(&root, &presets).map_err(err)
}

/// Task definitions as a `[[task]]` TOML snippet for copying or sharing.
#[tauri::command]
pub async fn tasks_to_toml(tasks: Vec<TaskDefinition>) -> Result<String, String> {
    tasks::tasks_to_toml(&tasks).map_err(err)
}

/// Parses pasted or imported task TOML (validated, nothing is saved).
#[tauri::command]
pub async fn tasks_parse_toml(text: String) -> Result<Vec<TaskDefinition>, String> {
    tasks::parse_tasks_toml(&text).map_err(err)
}

/// Installs the Pipeline OS Unity build script
/// (`Assets/Editor/PipelineOSBuild.cs`) unless one exists; returns its path.
#[tauri::command]
pub async fn tasks_install_unity_script(path: String) -> Result<String, String> {
    task_presets::install_unity_build_script(&root(&path))
        .map(|written| written.display().to_string())
        .map_err(err)
}

/// Saves the task list. Trust carries over only when the user already
/// trusted the file (or this creates it): editing one task in an unreviewed
/// file must not approve the commands someone else put there.
#[tauri::command]
pub async fn tasks_save(
    path: String,
    tasks: Vec<TaskDefinition>,
    studio: State<'_, StudioState>,
) -> Result<(), String> {
    let root = root(&path);
    let keep_trust = tasks::tasks_fingerprint(&root).is_none()
        || studio.trusted.lock().unwrap().is_trusted(&root);
    tasks::save_tasks(&root, &tasks).map_err(err)?;
    if !keep_trust {
        return Ok(());
    }
    if let Some(fingerprint) = tasks::tasks_fingerprint(&root) {
        studio
            .trusted
            .lock()
            .unwrap()
            .trust(&root, &fingerprint)
            .map_err(err)?;
    }
    Ok(())
}

/// Records that the user reviewed this exact tasks file and allows it to run.
#[tauri::command]
pub async fn tasks_trust(
    path: String,
    fingerprint: String,
    studio: State<'_, StudioState>,
) -> Result<(), String> {
    studio
        .trusted
        .lock()
        .unwrap()
        .trust(&root(&path), &fingerprint)
        .map_err(err)
}

#[tauri::command]
pub async fn task_plan(
    path: String,
    task_id: String,
    state: State<'_, DesktopState>,
) -> Result<String, String> {
    let (_, resolved) = state
        .service
        .resolve_project_task(&root(&path), &task_id)
        .await
        .map_err(err)?;
    Ok(resolved.display())
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TaskLines {
    run_id: String,
    lines: Vec<TaskLine>,
}

#[derive(Clone, Serialize)]
struct TaskLine {
    stream: OutputStream,
    text: String,
}

/// Starts a task and returns its run immediately; progress arrives as
/// `task://run` (status/progress) and `task://lines` (batched output) events.
#[tauri::command]
pub async fn task_run(
    app: AppHandle,
    path: String,
    task_id: String,
    state: State<'_, DesktopState>,
    studio: State<'_, StudioState>,
) -> Result<TaskRun, String> {
    let project_root = root(&path);
    if !studio.trusted.lock().unwrap().is_trusted(&project_root) {
        return Err(
            "TASKS_UNTRUSTED: review and trust this project's tasks before running them.".into(),
        );
    }
    let (task, resolved) = state
        .service
        .resolve_project_task(&project_root, &task_id)
        .await
        .map_err(err)?;
    // The file could have changed between the trust check and resolving it.
    if !studio.trusted.lock().unwrap().is_trusted(&project_root) {
        return Err("TASKS_UNTRUSTED: the tasks file just changed; review it again.".into());
    }
    let project_name = vantadeck_projects::load_project(&project_root)
        .map(|config| config.name)
        .unwrap_or_else(|_| path.clone());
    let run = TaskRun {
        id: format!("run-{}", uuid::Uuid::new_v4().simple()),
        project_path: path.clone(),
        project_name,
        task_id: task.id.clone(),
        task_name: task.name.clone(),
        kind: task.kind,
        command_line: resolved.display(),
        status: RunStatus::Running,
        progress: None,
        last_line: None,
        started_at: now(),
        finished_at: None,
        exit_code: None,
        duration_ms: None,
    };
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut runs = studio.runs.lock().unwrap();
        runs.insert(0, run.clone());
        // Trim finished history; never drop a run that is still active.
        let mut finished = 0;
        runs.retain(|r| {
            if matches!(r.status, RunStatus::Running | RunStatus::Queued) {
                return true;
            }
            finished += 1;
            finished <= MAX_HISTORY
        });
        // Drop captured output for runs that fell out of the history.
        let kept: std::collections::BTreeSet<&str> = runs.iter().map(|r| r.id.as_str()).collect();
        studio
            .logs
            .lock()
            .unwrap()
            .retain(|id, _| kept.contains(id.as_str()));
    }
    studio
        .cancels
        .lock()
        .unwrap()
        .insert(run.id.clone(), cancel.clone());
    studio
        .logs
        .lock()
        .unwrap()
        .insert(run.id.clone(), LogTail::new(LOG_LINES));
    studio.persist_runs();
    let _ = app.emit("task://run", &run);

    let run_id = run.id.clone();
    let handle = app.clone();
    std::thread::spawn(move || {
        let studio = handle.state::<StudioState>();
        let mut pending: Vec<TaskLine> = Vec::new();
        let mut last_flush = Instant::now();
        let mut progress: Option<f32> = None;
        let flush = |pending: &mut Vec<TaskLine>, progress: Option<f32>| {
            if pending.is_empty() {
                return;
            }
            let last = pending.last().map(|line| line.text.clone());
            if let Some(updated) = studio.update_run(&run_id, |run| {
                run.last_line = last;
                if progress.is_some() {
                    run.progress = progress;
                }
            }) {
                let _ = handle.emit("task://run", &updated);
            }
            let _ = handle.emit(
                "task://lines",
                TaskLines {
                    run_id: run_id.clone(),
                    lines: std::mem::take(pending),
                },
            );
        };
        let outcome = tasks::run_task(&resolved, cancel, |stream, text| {
            if let Some(value) = tasks::parse_progress(text) {
                progress = Some(value);
            }
            if let Some(log) = studio.logs.lock().unwrap().get_mut(&run_id) {
                log.push(text);
            }
            pending.push(TaskLine {
                stream,
                text: text.to_owned(),
            });
            // Batch output: engine builds can print thousands of lines a second.
            if last_flush.elapsed() >= Duration::from_millis(120) || pending.len() >= 500 {
                flush(&mut pending, progress);
                last_flush = Instant::now();
            }
        });
        flush(&mut pending, progress);
        let finished = studio.update_run(&run_id, |run| {
            run.finished_at = Some(now());
            match &outcome {
                Ok(outcome) => {
                    run.status = outcome.status;
                    run.exit_code = outcome.exit_code;
                    run.duration_ms = Some(outcome.duration_ms);
                    if outcome.status == RunStatus::Succeeded {
                        run.progress = Some(100.0);
                    }
                }
                Err(error) => {
                    run.status = RunStatus::Failed;
                    run.last_line = Some(error.to_string());
                }
            }
        });
        studio.cancels.lock().unwrap().remove(&run_id);
        studio.persist_runs();
        if let Some(run) = finished {
            let _ = handle.emit("task://run", &run);
        }
    });
    Ok(run)
}

#[tauri::command]
pub async fn task_cancel(run_id: String, studio: State<'_, StudioState>) -> Result<(), String> {
    match studio.cancels.lock().unwrap().get(&run_id) {
        Some(flag) => {
            flag.store(true, Ordering::SeqCst);
            Ok(())
        }
        None => Err("That task is no longer running.".into()),
    }
}

/// Recent and active runs, newest first (optionally for one project).
#[tauri::command]
pub async fn task_runs(
    path: Option<String>,
    studio: State<'_, StudioState>,
) -> Result<Vec<TaskRun>, String> {
    let runs = studio.runs.lock().unwrap();
    Ok(runs
        .iter()
        .filter(|run| path.as_ref().is_none_or(|p| &run.project_path == p))
        .cloned()
        .collect())
}

/// The captured output of a run in this session (most recent lines).
#[tauri::command]
pub async fn task_run_log(
    run_id: String,
    studio: State<'_, StudioState>,
) -> Result<Vec<String>, String> {
    Ok(studio
        .logs
        .lock()
        .unwrap()
        .get(&run_id)
        .map(|tail| tail.lines.iter().cloned().collect())
        .unwrap_or_default())
}

#[tauri::command]
pub async fn task_clear_history(studio: State<'_, StudioState>) -> Result<(), String> {
    studio
        .runs
        .lock()
        .unwrap()
        .retain(|run| matches!(run.status, RunStatus::Running | RunStatus::Queued));
    studio.persist_runs();
    Ok(())
}

// ---------------------------------------------------------------- issues

#[derive(Serialize)]
pub struct IssueList {
    issues: Vec<Issue>,
    problems: Vec<String>,
}

#[tauri::command]
pub async fn issues_list(path: String) -> Result<IssueList, String> {
    let (issues, problems) = issues::list_issues(&root(&path)).map_err(err)?;
    Ok(IssueList { issues, problems })
}

#[tauri::command]
pub async fn issue_create(path: String, issue: NewIssue) -> Result<Issue, String> {
    issues::create_issue(&root(&path), issue).map_err(err)
}

#[tauri::command]
pub async fn issue_update(path: String, issue: Issue) -> Result<Issue, String> {
    issues::update_issue(&root(&path), issue).map_err(err)
}

#[tauri::command]
pub async fn issue_delete(path: String, id: String) -> Result<(), String> {
    issues::delete_issue(&root(&path), &id).map_err(err)
}

#[tauri::command]
pub async fn issues_everywhere(
    state: State<'_, DesktopState>,
) -> Result<Vec<vantadeck_application::ProjectIssue>, String> {
    state.service.open_issues_everywhere().await.map_err(err)
}

// ---------------------------------------------------------------- trackers

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackerState {
    config: Option<TrackerConfig>,
    has_token: bool,
}

#[tauri::command]
pub async fn tracker_get(path: String) -> Result<TrackerState, String> {
    let config = trackers::load_tracker(&root(&path)).map_err(err)?;
    let has_token = config
        .as_ref()
        .is_some_and(|c| trackers::load_token(c).is_some());
    Ok(TrackerState { config, has_token })
}

/// Saves (or, with `config: null`, removes) the project's tracker. A
/// non-empty `token` is stored in the OS keychain, never in the project.
#[tauri::command]
pub async fn tracker_save(
    path: String,
    config: Option<TrackerConfig>,
    token: Option<String>,
) -> Result<(), String> {
    trackers::save_tracker(&root(&path), config.as_ref()).map_err(err)?;
    if let (Some(config), Some(token)) = (config.as_ref(), token.as_deref())
        && !token.trim().is_empty()
    {
        trackers::store_token(config, token.trim()).map_err(err)?;
    }
    Ok(())
}

#[tauri::command]
pub async fn tracker_sync(path: String) -> Result<SyncSummary, String> {
    trackers::sync_tracker(&root(&path)).await.map_err(err)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelfHostResult {
    compose_path: String,
    docker_available: bool,
    started: bool,
    url: String,
}

/// Writes a Forgejo docker-compose file into `dir` and, when `start` is set
/// and Docker is installed, starts it with `docker compose up -d`.
#[tauri::command]
pub async fn selfhost_forgejo(
    dir: String,
    port: Option<u16>,
    start: bool,
) -> Result<SelfHostResult, String> {
    let dir = PathBuf::from(dir);
    std::fs::create_dir_all(&dir).map_err(err)?;
    let port = port.unwrap_or(3000);
    let compose = dir.join("docker-compose.yml");
    if compose.exists() {
        return Err(format!(
            "{} already exists; choose an empty folder.",
            compose.display()
        ));
    }
    std::fs::write(&compose, trackers::forgejo_compose(port, 2222)).map_err(err)?;
    let docker_available = std::process::Command::new("docker")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success());
    let mut started = false;
    if start && docker_available {
        let output = tauri::async_runtime::spawn_blocking({
            let dir = dir.clone();
            move || {
                std::process::Command::new("docker")
                    .args(["compose", "up", "-d"])
                    .current_dir(dir)
                    .output()
            }
        })
        .await
        .map_err(err)?
        .map_err(err)?;
        if !output.status.success() {
            return Err(format!(
                "docker compose failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        started = true;
    }
    Ok(SelfHostResult {
        compose_path: compose.display().to_string(),
        docker_available,
        started,
        url: format!("http://localhost:{port}"),
    })
}

// ---------------------------------------------------------------- discovery

const DISCOVERY_CACHE: &str = "discovered-projects.json";
const DISCOVERY_DISMISSED: &str = "discovered-dismissed.json";

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Discovery {
    projects: Vec<vantadeck_studio::discovery::DiscoveredProject>,
    /// RFC 3339 time of the last full discovery pass, if any.
    scanned_at: Option<String>,
}

fn data_file(studio: &StudioState, name: &str) -> Option<PathBuf> {
    studio
        .data_dir
        .lock()
        .unwrap()
        .as_ref()
        .map(|dir| dir.join(name))
}

fn read_json<T: serde::de::DeserializeOwned + Default>(path: Option<PathBuf>) -> T {
    path.and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

/// Same key discovery uses, so registered/dismissed/found paths compare even
/// when spelled differently (symlinks, /private/var on macOS, slashes).
fn normalized(path: &str) -> String {
    vantadeck_studio::discovery::path_key(Path::new(path))
}

/// Projects on this computer that aren't in Pipeline OS yet: from Unity Hub,
/// Unreal's recent projects, Godot's project list, and a shallow pass over
/// common project folders. Returns the cached result unless `refresh`.
#[tauri::command]
pub async fn discover_projects(
    refresh: bool,
    state: State<'_, DesktopState>,
    studio: State<'_, StudioState>,
) -> Result<Discovery, String> {
    let registered: Vec<PathBuf> = state
        .service
        .registered_projects()
        .await
        .map_err(err)?
        .into_iter()
        .map(|project| project.root)
        .collect();
    let cache_path = data_file(&studio, DISCOVERY_CACHE);
    let mut discovery: Discovery = if refresh {
        let known = registered.clone();
        let projects = tauri::async_runtime::spawn_blocking(move || {
            vantadeck_studio::discovery::discover(
                &vantadeck_studio::discovery::DiscoverySources::system(),
                &known,
            )
        })
        .await
        .map_err(err)?;
        let fresh = Discovery {
            projects,
            scanned_at: Some(now()),
        };
        if let (Some(path), Ok(json)) = (cache_path, serde_json::to_string(&fresh)) {
            let _ = std::fs::write(path, json);
        }
        fresh
    } else {
        read_json(cache_path)
    };
    let dismissed: Vec<String> = read_json(data_file(&studio, DISCOVERY_DISMISSED));
    let registered: Vec<String> = registered
        .iter()
        .map(|p| normalized(&p.display().to_string()))
        .collect();
    discovery.projects.retain(|project| {
        let key = normalized(&project.path);
        !registered.contains(&key) && !dismissed.contains(&key) && Path::new(&project.path).is_dir()
    });
    Ok(discovery)
}

/// Hides a discovered project from future suggestions.
#[tauri::command]
pub async fn dismiss_discovered(
    path: String,
    studio: State<'_, StudioState>,
) -> Result<(), String> {
    let file = data_file(&studio, DISCOVERY_DISMISSED);
    let mut dismissed: Vec<String> = read_json(file.clone());
    let key = normalized(&path);
    if !dismissed.contains(&key) {
        dismissed.push(key);
    }
    if let Some(file) = file {
        std::fs::write(file, serde_json::to_string(&dismissed).map_err(err)?).map_err(err)?;
    }
    Ok(())
}

// ---------------------------------------------------------------- AI brief

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Brief {
    markdown: String,
    written_to: Option<String>,
}

#[tauri::command]
pub async fn project_brief(
    path: String,
    write: bool,
    state: State<'_, DesktopState>,
) -> Result<Brief, String> {
    let root = root(&path);
    let markdown = state.service.project_brief(&root).await.map_err(err)?;
    let written_to = if write {
        Some(
            vantadeck_studio::context::write_brief(&root, &markdown)
                .map_err(err)?
                .display()
                .to_string(),
        )
    } else {
        None
    };
    Ok(Brief {
        markdown,
        written_to,
    })
}

// ------------------------------------------------- TOML merge conflicts

/// Merges every conflicted `.pipelineos/` TOML file key by key; clean merges
/// are staged, the rest are reported with their conflicting keys.
#[tauri::command]
pub async fn toml_conflicts_auto_merge(path: String) -> Result<Vec<FileMergeReport>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        toml_merge::auto_merge_pipeline_files(&root(&path))
    })
    .await
    .map_err(err)?
    .map_err(err)
}

/// Finishes one conflicted Pipeline OS file with per-key choices
/// (`keyPath` -> "ours" | "theirs" | "base") and stages it.
#[tauri::command]
pub async fn toml_conflict_resolve(
    path: String,
    file: String,
    choices: BTreeMap<String, Side>,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        toml_merge::apply_toml_choices(&root(&path), &file, &choices)
    })
    .await
    .map_err(err)?
    .map_err(err)
}

// ---------------------------------------------------------------- assets

#[tauri::command]
pub async fn asset_layouts() -> Vec<FolderLayout> {
    assets::folder_layouts()
}

#[tauri::command]
pub async fn asset_apply_layout(path: String, layout: String) -> Result<LayoutResult, String> {
    assets::apply_layout(&root(&path), &layout).map_err(err)
}

#[tauri::command]
pub async fn assets_index(path: String, limit: Option<usize>) -> Result<AssetIndex, String> {
    let limit = limit.unwrap_or(600).clamp(1, 5_000);
    tauri::async_runtime::spawn_blocking(move || assets::index_assets(&root(&path), limit))
        .await
        .map_err(err)?
        .map_err(err)
}

#[tauri::command]
pub async fn asset_set_meta(path: String, asset: String, meta: AssetMeta) -> Result<(), String> {
    assets::set_asset_meta(&root(&path), &asset, meta).map_err(err)
}

// ------------------------------------------------- terminals and AI agents

/// Command-line AI agents Pipeline OS can start in a project terminal.
const AGENTS: &[(&str, &str)] = &[
    ("claude", "claude"),
    ("codex", "codex"),
    ("gemini", "gemini"),
    ("aider", "aider"),
];

/// Opens a terminal in the project folder, optionally running a known AI
/// agent CLI there. The bundled `vantadeck` CLI is put on PATH for it.
#[tauri::command]
pub async fn open_terminal(path: String, agent: Option<String>) -> Result<(), String> {
    let root = root(&path);
    if !root.is_dir() {
        return Err("The project folder no longer exists.".into());
    }
    let program = match agent.as_deref() {
        None | Some("") => None,
        Some(id) => Some(
            AGENTS
                .iter()
                .find(|(known, _)| *known == id)
                .map(|(_, program)| *program)
                .ok_or_else(|| format!("Unknown agent {id:?}"))?,
        ),
    };
    let path_env = cli_dir()
        .map(|dir| {
            let mut paths = vec![dir];
            if let Some(existing) = std::env::var_os("PATH") {
                paths.extend(std::env::split_paths(&existing));
            }
            std::env::join_paths(paths).unwrap_or_default()
        })
        .unwrap_or_else(|| std::env::var_os("PATH").unwrap_or_default());
    spawn_terminal(&root, program, &path_env)
}

#[cfg(windows)]
fn spawn_terminal(
    root: &Path,
    program: Option<&str>,
    path_env: &std::ffi::OsStr,
) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    use std::process::Command;
    const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
    // Windows Terminal when present, otherwise a classic PowerShell window.
    let mut wt = Command::new("wt.exe");
    wt.arg("-d").arg(root).env("PATH", path_env);
    if let Some(program) = program {
        wt.args(["powershell.exe", "-NoExit", "-Command", program]);
    }
    if wt.spawn().is_ok() {
        return Ok(());
    }
    let mut ps = Command::new("powershell.exe");
    ps.arg("-NoExit")
        .current_dir(root)
        .env("PATH", path_env)
        .creation_flags(CREATE_NEW_CONSOLE);
    if let Some(program) = program {
        ps.args(["-Command", program]);
    }
    ps.spawn()
        .map(|_| ())
        .map_err(|e| format!("Could not open a terminal: {e}"))
}

#[cfg(target_os = "macos")]
fn spawn_terminal(
    root: &Path,
    program: Option<&str>,
    _path_env: &std::ffi::OsStr,
) -> Result<(), String> {
    use std::process::Command;
    // Terminal.app starts a fresh login shell, so put the bundled CLI on its
    // PATH there rather than in this process's environment.
    let path_prefix = cli_dir()
        .map(|dir| {
            format!(
                "export PATH={}:\\\"$PATH\\\" && ",
                shell_quote(&dir.display().to_string())
            )
        })
        .unwrap_or_default();
    let cd = shell_quote(&root.display().to_string());
    let script = match program {
        Some(program) => format!(
            "tell application \"Terminal\" to do script \"cd {cd} && {path_prefix}{program}\""
        ),
        None => {
            format!("tell application \"Terminal\" to do script \"cd {cd} && {path_prefix}true\"")
        }
    };
    Command::new("osascript")
        .args([
            "-e",
            &script,
            "-e",
            "tell application \"Terminal\" to activate",
        ])
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Could not open Terminal: {e}"))
}

#[cfg(target_os = "macos")]
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\\\''").replace('"', "\\\""))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn spawn_terminal(
    root: &Path,
    program: Option<&str>,
    path_env: &std::ffi::OsStr,
) -> Result<(), String> {
    use std::process::Command;
    let mut last_error = String::from("no terminal emulator found");
    for terminal in ["x-terminal-emulator", "gnome-terminal", "konsole", "xterm"] {
        let mut command = Command::new(terminal);
        command.current_dir(root).env("PATH", path_env);
        if let Some(program) = program {
            match terminal {
                "gnome-terminal" => command.args(["--", program]),
                _ => command.args(["-e", program]),
            };
        }
        match command.spawn() {
            Ok(_) => return Ok(()),
            Err(error) => last_error = error.to_string(),
        }
    }
    Err(format!("Could not open a terminal: {last_error}"))
}

// ---------------------------------------------------- CLI / MCP integration

fn cli_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()?
        .parent()
        .map(Path::to_path_buf)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CliInfo {
    /// Absolute path of the bundled `vantadeck` CLI, when installed.
    cli_path: Option<String>,
    channel: &'static str,
    /// Arguments an MCP client should pass to the CLI.
    mcp_args: Vec<String>,
}

fn cli_info_for(app: &AppHandle) -> CliInfo {
    let name = if cfg!(windows) {
        "vantadeck.exe"
    } else {
        "vantadeck"
    };
    let cli_path = cli_dir()
        .map(|dir| dir.join(name))
        .filter(|path| path.is_file())
        .map(|path| path.display().to_string());
    let channel = if app.config().identifier.ends_with(".dev") {
        "dev"
    } else {
        "stable"
    };
    let mut mcp_args = vec!["mcp".to_owned()];
    if channel == "dev" {
        mcp_args.insert(0, "--channel=dev".to_owned());
    }
    CliInfo {
        cli_path,
        channel,
        mcp_args,
    }
}

#[tauri::command]
pub async fn cli_info(app: AppHandle) -> CliInfo {
    cli_info_for(&app)
}

// ------------------------------------------------- AI clients (MCP setup)

use vantadeck_studio::ai_clients::{self, AiClientStatus, ClientLocations, ServerCommand};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiSetupState {
    clients: Vec<AiClientStatus>,
    /// Agent CLIs found on this machine (claude, codex, gemini, aider).
    agent_clis: Vec<String>,
    /// Whether the bundled CLI exists, so clients can be connected.
    can_configure: bool,
}

fn locations() -> Result<ClientLocations, String> {
    ClientLocations::system().ok_or_else(|| "Couldn't find your home folder.".to_owned())
}

#[tauri::command]
pub async fn ai_clients(app: AppHandle) -> Result<AiSetupState, String> {
    let locations = locations()?;
    Ok(AiSetupState {
        clients: ai_clients::detect_clients(&locations),
        agent_clis: ai_clients::installed_agent_clis(&locations),
        can_configure: cli_info_for(&app).cli_path.is_some(),
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiConfigureResult {
    id: String,
    ok: bool,
    error: Option<String>,
}

/// Connects the given AI clients to the bundled Pipeline OS MCP server.
#[tauri::command]
pub async fn ai_configure(
    app: AppHandle,
    ids: Vec<String>,
) -> Result<Vec<AiConfigureResult>, String> {
    let info = cli_info_for(&app);
    let command = info
        .cli_path
        .ok_or("The vantadeck CLI isn't installed next to the app (development build?).")?;
    let server = ServerCommand {
        command,
        args: info.mcp_args,
    };
    let locations = locations()?;
    Ok(ids
        .into_iter()
        .map(
            |id| match ai_clients::configure_client(&locations, &id, &server) {
                Ok(()) => AiConfigureResult {
                    id,
                    ok: true,
                    error: None,
                },
                Err(error) => AiConfigureResult {
                    id,
                    ok: false,
                    error: Some(error.to_string()),
                },
            },
        )
        .collect())
}

#[tauri::command]
pub async fn ai_unconfigure(id: String) -> Result<(), String> {
    ai_clients::unconfigure_client(&locations()?, &id).map_err(err)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interrupted_runs_are_marked_cancelled_on_load() {
        let dir = tempfile::tempdir().unwrap();
        let run = TaskRun {
            id: "run-1".into(),
            project_path: "/p".into(),
            project_name: "P".into(),
            task_id: "build".into(),
            task_name: "Build".into(),
            kind: tasks::TaskKind::Build,
            command_line: "x".into(),
            status: RunStatus::Running,
            progress: None,
            last_line: None,
            started_at: now(),
            finished_at: None,
            exit_code: None,
            duration_ms: None,
        };
        std::fs::write(
            dir.path().join(RUN_HISTORY_FILE),
            serde_json::to_string(&vec![run]).unwrap(),
        )
        .unwrap();
        let state = StudioState::load(dir.path());
        assert_eq!(state.runs.lock().unwrap()[0].status, RunStatus::Cancelled);
    }
}

/// The tools the bundled MCP server offers AI apps, for Settings → AI.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpToolInfo {
    name: String,
    title: String,
    description: String,
    read_only: bool,
}

#[tauri::command]
pub fn mcp_tools() -> Vec<McpToolInfo> {
    vantadeck::mcp::tool_definitions()
        .into_iter()
        .map(|tool| {
            let text = |key: &str| {
                tool.get(key)
                    .and_then(|value| value.as_str())
                    .unwrap_or_default()
                    .to_owned()
            };
            McpToolInfo {
                name: text("name"),
                title: text("title"),
                description: text("description"),
                read_only: tool
                    .pointer("/annotations/readOnlyHint")
                    .and_then(|value| value.as_bool())
                    .unwrap_or(false),
            }
        })
        .collect()
}
