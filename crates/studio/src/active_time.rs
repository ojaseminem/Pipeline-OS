//! Active development time per project.
//!
//! A per-machine store (`active-time.json` in the app data folder, never in
//! the project) of how many minutes each project was actively worked on per
//! local day. The desktop app counts a minute when an engine editor log grew
//! for the project or the project's files changed; counting is idempotent per
//! minute, so several signals in the same minute count once.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

use chrono::{DateTime, Duration, Local, NaiveDate};
use serde::{Deserialize, Serialize};

use crate::{StudioError, write_atomic};

pub const ACTIVE_TIME_FILE: &str = "active-time.json";

/// Days older than this are folded into `archivedMinutes` to keep the file
/// small; totals stay exact.
const KEEP_DAYS: i64 = 400;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectTime {
    /// Path as first recorded (for display).
    path: String,
    /// `YYYY-MM-DD` (local) -> active minutes.
    #[serde(default)]
    days: BTreeMap<String, u32>,
    /// Minutes from days pruned out of `days`.
    #[serde(default)]
    archived_minutes: u64,
    /// Unix minute of the last counted minute (idempotency marker).
    #[serde(default)]
    last_minute: Option<i64>,
    /// RFC 3339 local time of the last counted minute.
    #[serde(default)]
    last_active: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoreFile {
    #[serde(default)]
    projects: BTreeMap<String, ProjectTime>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DayMinutes {
    pub date: String,
    pub minutes: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveTimeSummary {
    pub today_minutes: u32,
    /// Today and the six days before it.
    pub week_minutes: u32,
    pub total_minutes: u64,
    pub last_active: Option<String>,
    /// The last 14 days, oldest first, including days with no activity.
    pub days: Vec<DayMinutes>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectActiveTotal {
    pub path: String,
    /// Minutes over the requested window (e.g. the last 7 days).
    pub minutes: u32,
    pub total_minutes: u64,
    pub last_active: Option<String>,
}

/// The active-time store; load once, then record and query.
#[derive(Debug, Default)]
pub struct ActiveTimeStore {
    path: Option<PathBuf>,
    file: StoreFile,
}

/// Normalized key so `D:\Game`, `d:/game/` and `D:/Game` match.
fn key(project: &Path) -> String {
    let raw = project.display().to_string().replace('\\', "/");
    let trimmed = raw.trim_end_matches('/');
    if cfg!(any(windows, target_os = "macos")) {
        trimmed.to_lowercase()
    } else {
        trimmed.to_owned()
    }
}

fn date_key(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

impl ActiveTimeStore {
    pub fn load(data_dir: &Path) -> Self {
        let path = data_dir.join(ACTIVE_TIME_FILE);
        let file = std::fs::read_to_string(&path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default();
        Self {
            path: Some(path),
            file,
        }
    }

    /// Counts the minute containing `now` as active for `project`. Returns
    /// whether it was newly counted (false when this minute already was).
    pub fn record_minute(
        &mut self,
        project: &Path,
        now: DateTime<Local>,
    ) -> Result<bool, StudioError> {
        let minute = now.timestamp().div_euclid(60);
        let entry = self
            .file
            .projects
            .entry(key(project))
            .or_insert_with(|| ProjectTime {
                path: project.display().to_string(),
                ..Default::default()
            });
        if entry.last_minute.is_some_and(|last| last >= minute) {
            return Ok(false);
        }
        entry.last_minute = Some(minute);
        entry.last_active = Some(now.to_rfc3339_opts(chrono::SecondsFormat::Secs, false));
        *entry.days.entry(date_key(now.date_naive())).or_default() += 1;
        let cutoff = date_key(now.date_naive() - Duration::days(KEEP_DAYS));
        let old: Vec<String> = entry
            .days
            .range(..cutoff)
            .map(|(day, _)| day.clone())
            .collect();
        for day in old {
            if let Some(minutes) = entry.days.remove(&day) {
                entry.archived_minutes += u64::from(minutes);
            }
        }
        self.save()?;
        Ok(true)
    }

    /// Summary for `project` as of now.
    pub fn summary(&self, project: &Path) -> ActiveTimeSummary {
        self.summary_at(project, Local::now().date_naive())
    }

    /// Summary for `project` with `today` as the current local day.
    pub fn summary_at(&self, project: &Path, today: NaiveDate) -> ActiveTimeSummary {
        let entry = self.file.projects.get(&key(project));
        let minutes_on = |date: NaiveDate| {
            entry
                .and_then(|e| e.days.get(&date_key(date)).copied())
                .unwrap_or(0)
        };
        let days: Vec<DayMinutes> = (0..14)
            .rev()
            .map(|back| {
                let date = today - Duration::days(back);
                DayMinutes {
                    date: date_key(date),
                    minutes: minutes_on(date),
                }
            })
            .collect();
        ActiveTimeSummary {
            today_minutes: minutes_on(today),
            week_minutes: (0..7)
                .map(|back| minutes_on(today - Duration::days(back)))
                .sum(),
            total_minutes: entry.map(total).unwrap_or(0),
            last_active: entry.and_then(|e| e.last_active.clone()),
            days,
        }
    }

    /// Every project's minutes over the last `days` days (including today),
    /// most active first.
    pub fn totals(&self, today: NaiveDate, days: u32) -> Vec<ProjectActiveTotal> {
        let mut totals: Vec<ProjectActiveTotal> = self
            .file
            .projects
            .values()
            .map(|entry| ProjectActiveTotal {
                path: entry.path.clone(),
                minutes: (0..i64::from(days))
                    .map(|back| {
                        entry
                            .days
                            .get(&date_key(today - Duration::days(back)))
                            .copied()
                            .unwrap_or(0)
                    })
                    .sum(),
                total_minutes: total(entry),
                last_active: entry.last_active.clone(),
            })
            .collect();
        totals.sort_by(|a, b| {
            b.minutes
                .cmp(&a.minutes)
                .then(b.total_minutes.cmp(&a.total_minutes))
                .then_with(|| a.path.cmp(&b.path))
        });
        totals
    }

    fn save(&self) -> Result<(), StudioError> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let json =
            serde_json::to_string(&self.file).map_err(|e| StudioError::Invalid(e.to_string()))?;
        write_atomic(path, &json)
    }
}

fn total(entry: &ProjectTime) -> u64 {
    entry.archived_minutes + entry.days.values().map(|m| u64::from(*m)).sum::<u64>()
}

fn mtime_secs(path: &Path) -> u64 {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// A cheap fingerprint of recent work in a project: the newest mtime among
/// the project folder, `.git/index`, and the authored-content folders
/// (`Assets/`, `Content/`, `Source/`, `Config/`, `Packages/`) plus their
/// immediate subfolders. Editors save by writing a temp file and renaming it,
/// which bumps the containing folder's mtime, so this notices most saves
/// without walking the project. Compare successive values; any change means
/// activity.
pub fn activity_signature(root: &Path) -> u64 {
    let mut newest = mtime_secs(root).max(mtime_secs(&root.join(".git/index")));
    for folder in [
        "Assets", "Content", "Source", "Config", "Packages", "scenes",
    ] {
        let dir = root.join(folder);
        newest = newest.max(mtime_secs(&dir));
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten().take(200) {
                if entry.file_type().is_ok_and(|t| t.is_dir()) {
                    newest = newest.max(mtime_secs(&entry.path()));
                }
            }
        }
    }
    newest
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(y: i32, m: u32, d: u32, h: u32, min: u32, s: u32) -> DateTime<Local> {
        Local
            .with_ymd_and_hms(y, m, d, h, min, s)
            .earliest()
            .unwrap()
    }

    #[test]
    fn records_each_minute_once_and_persists() {
        let data = tempfile::tempdir().unwrap();
        let project = Path::new("/work/Voidline");
        let mut store = ActiveTimeStore::load(data.path());
        assert!(
            store
                .record_minute(project, at(2026, 10, 22, 10, 0, 5))
                .unwrap()
        );
        assert!(
            !store
                .record_minute(project, at(2026, 10, 22, 10, 0, 50))
                .unwrap()
        );
        assert!(
            store
                .record_minute(project, at(2026, 10, 22, 10, 1, 0))
                .unwrap()
        );
        // A late signal for an earlier minute doesn't double count.
        assert!(
            !store
                .record_minute(project, at(2026, 10, 22, 10, 0, 59))
                .unwrap()
        );
        // Anything older than the last counted minute is ignored.
        assert!(
            !store
                .record_minute(project, at(2026, 10, 20, 23, 0, 0))
                .unwrap()
        );

        let reloaded = ActiveTimeStore::load(data.path());
        let today = NaiveDate::from_ymd_opt(2026, 10, 22).unwrap();
        let summary = reloaded.summary_at(Path::new("/work/Voidline/"), today);
        assert_eq!(summary.today_minutes, 2);
        assert_eq!(summary.total_minutes, 2);
        assert_eq!(summary.days.len(), 14);
        assert_eq!(summary.days.last().unwrap().date, "2026-10-22");
        assert_eq!(summary.days.last().unwrap().minutes, 2);
        assert!(
            summary
                .last_active
                .unwrap()
                .starts_with("2026-10-22T10:01:00")
        );
    }

    #[test]
    fn week_and_totals_span_days() {
        let data = tempfile::tempdir().unwrap();
        let mut store = ActiveTimeStore::load(data.path());
        let a = Path::new("/work/A");
        let b = Path::new("/work/B");
        for minute in 0..30 {
            store
                .record_minute(a, at(2026, 10, 10, 9, minute, 0))
                .unwrap();
        }
        for minute in 0..5 {
            store
                .record_minute(a, at(2026, 10, 21, 9, minute, 0))
                .unwrap();
            store
                .record_minute(b, at(2026, 10, 22, 9, minute, 0))
                .unwrap();
        }
        store.record_minute(b, at(2026, 10, 22, 9, 10, 0)).unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 10, 22).unwrap();
        let summary = store.summary_at(a, today);
        assert_eq!(summary.today_minutes, 0);
        assert_eq!(summary.week_minutes, 5);
        assert_eq!(summary.total_minutes, 35);
        assert_eq!(summary.days[1].date, "2026-10-10");
        assert_eq!(summary.days[1].minutes, 30);
        let totals = store.totals(today, 7);
        assert_eq!(totals[0].path, "/work/B");
        assert_eq!(totals[0].minutes, 6);
        assert_eq!(totals[1].minutes, 5);
        assert_eq!(totals[1].total_minutes, 35);
        let empty = store.summary_at(Path::new("/nowhere"), today);
        assert_eq!(empty.total_minutes, 0);
        assert!(empty.last_active.is_none());
    }

    #[test]
    fn old_days_are_archived_without_losing_totals() {
        let data = tempfile::tempdir().unwrap();
        let mut store = ActiveTimeStore::load(data.path());
        let p = Path::new("/work/Old");
        store.record_minute(p, at(2024, 1, 1, 9, 0, 0)).unwrap();
        store.record_minute(p, at(2026, 10, 22, 9, 0, 0)).unwrap();
        let entry = &store.file.projects[&key(p)];
        assert_eq!(entry.days.len(), 1);
        assert_eq!(entry.archived_minutes, 1);
        let today = NaiveDate::from_ymd_opt(2026, 10, 22).unwrap();
        assert_eq!(store.summary_at(p, today).total_minutes, 2);
    }

    #[cfg(unix)]
    #[test]
    fn signature_changes_when_content_folders_change() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("Assets/Art")).unwrap();
        let before = activity_signature(dir.path());
        assert!(before > 0);
        // mtimes have one-second resolution on some filesystems.
        let old = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
        for path in [
            dir.path().to_path_buf(),
            dir.path().join("Assets"),
            dir.path().join("Assets/Art"),
        ] {
            std::fs::File::open(&path)
                .unwrap()
                .set_modified(old)
                .unwrap();
        }
        let quiet = activity_signature(dir.path());
        assert!(quiet < before);
        std::fs::write(dir.path().join("Assets/Art/hero.png"), b"x").unwrap();
        assert!(activity_signature(dir.path()) > quiet);
    }
}
