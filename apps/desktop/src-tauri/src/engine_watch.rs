//! Watches engine editor logs (Unity's Editor.log, each registered Unreal
//! project's Saved/Logs) and turns recognized lines into "engine activity":
//! lighting builds, shader compiles, cooks, player builds and script compiles
//! that run inside an editor Pipeline OS didn't start. Updates are emitted as
//! `engine://activity` events and kept in a short in-memory history.
//!
//! The same loop measures active development time: a minute counts for a
//! project when its engine log grew in that minute, or (checked every 60s)
//! when the project's files changed.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Duration, Instant},
};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use vantadeck_studio::{
    active_time::{self, ActiveTimeStore, ActiveTimeSummary, ProjectActiveTotal},
    engine_activity::{
        self, ActivityKind, ActivityStatus, EngineEvent, EngineKind, LogFollower, LogParser,
    },
};

use crate::DesktopState;

const POLL: Duration = Duration::from_secs(2);
const PROJECT_REFRESH: Duration = Duration::from_secs(30);
/// A running operation with no news for this long is assumed abandoned
/// (editor crashed or closed mid-way).
const STALE_AFTER: Duration = Duration::from_secs(45 * 60);
const MAX_HISTORY: usize = 40;
/// How often project files are checked for changes (active time).
const FILE_CHECK: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineActivity {
    pub id: String,
    pub engine: EngineKind,
    pub project_path: Option<String>,
    pub project_name: Option<String>,
    pub kind: ActivityKind,
    pub status: ActivityStatus,
    pub progress: Option<f32>,
    pub detail: String,
    pub started_at: String,
    pub updated_at: String,
    pub finished_at: Option<String>,
    #[serde(skip)]
    touched: Option<Instant>,
}

#[derive(Default)]
pub struct EngineWatchState {
    activities: Mutex<Vec<EngineActivity>>,
    /// Loaded on first use from the app data folder.
    active_time: Mutex<Option<ActiveTimeStore>>,
}

/// Runs `apply` on the active-time store, loading it on first use.
fn with_active_time<T>(
    app: &AppHandle,
    apply: impl FnOnce(&mut ActiveTimeStore) -> T,
) -> Option<T> {
    let state = app.try_state::<EngineWatchState>()?;
    let mut guard = state.active_time.lock().ok()?;
    if guard.is_none() {
        let data_dir = app.path().app_data_dir().ok()?;
        *guard = Some(ActiveTimeStore::load(&data_dir));
    }
    guard.as_mut().map(apply)
}

fn record_active_minute(app: &AppHandle, project: &Path) {
    let now = chrono::Local::now();
    with_active_time(app, |store| {
        let _ = store.record_minute(project, now);
    });
}

/// Active development time for one project: today, this week, total, and
/// the last 14 days.
#[tauri::command]
pub fn project_active_time(path: String, app: AppHandle) -> Option<ActiveTimeSummary> {
    with_active_time(&app, |store| store.summary(Path::new(&path)))
}

/// Active minutes per project over the last 7 days, most active first.
#[tauri::command]
pub fn active_time_all(app: AppHandle) -> Vec<ProjectActiveTotal> {
    let today = chrono::Local::now().date_naive();
    with_active_time(&app, |store| store.totals(today, 7)).unwrap_or_default()
}

#[tauri::command]
pub fn engine_activity(state: State<'_, EngineWatchState>) -> Vec<EngineActivity> {
    state
        .activities
        .lock()
        .map(|list| list.clone())
        .unwrap_or_default()
}

#[tauri::command]
pub fn clear_engine_activity(state: State<'_, EngineWatchState>) {
    if let Ok(mut list) = state.activities.lock() {
        list.retain(|activity| activity.status == ActivityStatus::Running);
    }
}

/// Starts the background watcher thread.
pub fn spawn(app: AppHandle) {
    std::thread::Builder::new()
        .name("engine-log-watch".into())
        .spawn(move || watch(app))
        .ok();
}

struct Source {
    engine: EngineKind,
    follower: LogFollower,
    parser: LogParser,
    /// The project this log belongs to (Unreal), or the last project Unity
    /// reported opening.
    project: Option<PathBuf>,
}

fn watch(app: AppHandle) {
    let mut sources: BTreeMap<PathBuf, Source> = BTreeMap::new();
    if let Some(path) = engine_activity::unity_editor_log() {
        sources.insert(
            path.clone(),
            Source {
                engine: EngineKind::Unity,
                follower: LogFollower::new(path),
                parser: LogParser::new(EngineKind::Unity),
                project: None,
            },
        );
    }
    let mut last_refresh: Option<Instant> = None;
    let mut projects: Vec<PathBuf> = Vec::new();
    let mut signatures: BTreeMap<PathBuf, u64> = BTreeMap::new();
    let mut last_file_check: Option<Instant> = None;
    loop {
        if last_refresh.is_none_or(|at| at.elapsed() >= PROJECT_REFRESH) {
            if let Some(roots) = refresh_unreal_sources(&app, &mut sources) {
                projects = roots;
            }
            last_refresh = Some(Instant::now());
        }
        for source in sources.values_mut() {
            let lines = source.follower.poll();
            let grew = !lines.is_empty();
            for line in lines {
                if source.engine == EngineKind::Unity
                    && let Some(project) = engine_activity::unity_project_from_line(&line)
                {
                    source.project = Some(project);
                    continue;
                }
                if let Some(event) = source.parser.parse_line(&line) {
                    record(&app, source.engine, source.project.as_deref(), event);
                }
            }
            if grew && let Some(project) = &source.project {
                record_active_minute(&app, project);
            }
        }
        if last_file_check.is_none_or(|at| at.elapsed() >= FILE_CHECK) {
            check_project_files(&app, &projects, &mut signatures);
            last_file_check = Some(Instant::now());
        }
        expire_stale(&app);
        std::thread::sleep(POLL);
    }
}

/// Counts an active minute for each project whose files changed since the
/// last check. The first sighting of a project only records its baseline.
fn check_project_files(
    app: &AppHandle,
    projects: &[PathBuf],
    signatures: &mut BTreeMap<PathBuf, u64>,
) {
    signatures.retain(|root, _| projects.contains(root));
    for root in projects {
        let signature = active_time::activity_signature(root);
        if signature == 0 {
            continue;
        }
        let previous = signatures.insert(root.clone(), signature);
        if previous.is_some_and(|previous| previous != signature) {
            record_active_minute(app, root);
        }
    }
}

/// Follows each registered Unreal project's log; returns the registered
/// project roots (`None` when they couldn't be read).
fn refresh_unreal_sources(
    app: &AppHandle,
    sources: &mut BTreeMap<PathBuf, Source>,
) -> Option<Vec<PathBuf>> {
    let state = app.try_state::<DesktopState>()?;
    let projects = tauri::async_runtime::block_on(state.service.registered_projects()).ok()?;
    let roots: Vec<PathBuf> = projects
        .iter()
        .map(|project| project.root.clone())
        .collect();
    let mut wanted = BTreeMap::new();
    for project in projects {
        if let Some(log) = engine_activity::unreal_project_log(&project.root) {
            wanted.insert(log, project.root);
        }
    }
    sources.retain(|path, source| source.engine == EngineKind::Unity || wanted.contains_key(path));
    for (log, root) in wanted {
        sources.entry(log.clone()).or_insert_with(|| Source {
            engine: EngineKind::Unreal,
            follower: LogFollower::new(log),
            parser: LogParser::new(EngineKind::Unreal),
            project: Some(root),
        });
    }
    Some(roots)
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn project_name(path: &Path) -> Option<String> {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
}

/// Folds one parsed event into the activity list and emits the change.
fn record(app: &AppHandle, engine: EngineKind, project: Option<&Path>, event: EngineEvent) {
    let Some(state) = app.try_state::<EngineWatchState>() else {
        return;
    };
    let changed = {
        let Ok(mut list) = state.activities.lock() else {
            return;
        };
        apply(&mut list, engine, project, event, &now())
    };
    if let Some(activity) = changed {
        let _ = app.emit("engine://activity", activity);
    }
}

fn apply(
    list: &mut Vec<EngineActivity>,
    engine: EngineKind,
    project: Option<&Path>,
    event: EngineEvent,
    timestamp: &str,
) -> Option<EngineActivity> {
    let project_path = project.map(|p| p.display().to_string());
    let running = list.iter_mut().find(|activity| {
        activity.status == ActivityStatus::Running
            && activity.engine == engine
            && activity.kind == event.kind
            && activity.project_path == project_path
    });
    match (running, event.status) {
        (Some(activity), status) => {
            activity.status = status;
            if event.progress.is_some() {
                activity.progress = event.progress;
            }
            if status != ActivityStatus::Running {
                activity.finished_at = Some(timestamp.to_owned());
                activity.progress = (status == ActivityStatus::Succeeded).then_some(100.0);
            }
            activity.detail = event.detail;
            activity.updated_at = timestamp.to_owned();
            activity.touched = Some(Instant::now());
            Some(activity.clone())
        }
        // A finish without a start is only worth showing for the long,
        // deliberate operations; routine compiles/imports would be noise.
        (None, ActivityStatus::Succeeded | ActivityStatus::Failed)
            if !matches!(
                event.kind,
                ActivityKind::Lighting | ActivityKind::Build | ActivityKind::Cook
            ) =>
        {
            None
        }
        (None, status) => {
            let activity = EngineActivity {
                id: format!("{}-{}", timestamp, list.len()),
                engine,
                project_name: project.and_then(project_name),
                project_path,
                kind: event.kind,
                status,
                progress: event.progress,
                detail: event.detail,
                started_at: timestamp.to_owned(),
                updated_at: timestamp.to_owned(),
                finished_at: (status != ActivityStatus::Running).then(|| timestamp.to_owned()),
                touched: Some(Instant::now()),
            };
            list.insert(0, activity.clone());
            list.truncate(MAX_HISTORY);
            Some(activity)
        }
    }
}

fn expire_stale(app: &AppHandle) {
    let Some(state) = app.try_state::<EngineWatchState>() else {
        return;
    };
    let expired: Vec<EngineActivity> = {
        let Ok(mut list) = state.activities.lock() else {
            return;
        };
        let timestamp = now();
        list.iter_mut()
            .filter(|activity| {
                activity.status == ActivityStatus::Running
                    && activity
                        .touched
                        .is_some_and(|at| at.elapsed() >= STALE_AFTER)
            })
            .map(|activity| {
                activity.status = ActivityStatus::Failed;
                activity.detail = "No further output from the editor; it may have closed.".into();
                activity.finished_at = Some(timestamp.clone());
                activity.updated_at = timestamp.clone();
                activity.clone()
            })
            .collect()
    };
    for activity in expired {
        let _ = app.emit("engine://activity", activity);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(kind: ActivityKind, status: ActivityStatus, progress: Option<f32>) -> EngineEvent {
        EngineEvent {
            kind,
            status,
            progress,
            detail: "line".into(),
        }
    }

    #[test]
    fn running_then_finished_updates_one_entry() {
        let mut list = Vec::new();
        let project = Path::new("/work/Reactor");
        let start = apply(
            &mut list,
            EngineKind::Unreal,
            Some(project),
            event(ActivityKind::Lighting, ActivityStatus::Running, None),
            "t1",
        )
        .unwrap();
        assert_eq!(start.project_name.as_deref(), Some("Reactor"));
        apply(
            &mut list,
            EngineKind::Unreal,
            Some(project),
            event(ActivityKind::Lighting, ActivityStatus::Running, Some(40.0)),
            "t2",
        );
        let done = apply(
            &mut list,
            EngineKind::Unreal,
            Some(project),
            event(ActivityKind::Lighting, ActivityStatus::Succeeded, None),
            "t3",
        )
        .unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(done.status, ActivityStatus::Succeeded);
        assert_eq!(done.progress, Some(100.0));
        assert_eq!(done.started_at, "t1");
        assert_eq!(done.finished_at.as_deref(), Some("t3"));
    }

    #[test]
    fn ignores_orphan_finishes_of_routine_work() {
        let mut list = Vec::new();
        let none = apply(
            &mut list,
            EngineKind::Unity,
            None,
            event(ActivityKind::Import, ActivityStatus::Succeeded, None),
            "t1",
        );
        assert!(none.is_none());
        let build = apply(
            &mut list,
            EngineKind::Unity,
            None,
            event(ActivityKind::Build, ActivityStatus::Failed, None),
            "t1",
        );
        assert!(build.is_some());
        assert_eq!(list.len(), 1);
    }
}
