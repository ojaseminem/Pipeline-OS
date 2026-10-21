//! A git-native issue tracker: one TOML file per issue under
//! `.pipelineos/issues/`. One file per issue means two people creating or
//! editing different issues never conflict, history and blame come from the
//! VCS, and the board works fully offline. Issues mirrored from an external
//! tracker (Jira, Linear, GitHub, Plane...) keep a link in `external`.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{StudioError, metadata_path, validate_id, write_atomic};

pub const ISSUES_DIR: &str = "issues";

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "kebab-case")]
pub enum IssueStatus {
    Backlog,
    #[default]
    Todo,
    InProgress,
    Review,
    Done,
}

impl IssueStatus {
    pub const ALL: [IssueStatus; 5] = [
        Self::Backlog,
        Self::Todo,
        Self::InProgress,
        Self::Review,
        Self::Done,
    ];

    pub fn parse(value: &str) -> Option<Self> {
        let normalized = value.trim().to_ascii_lowercase().replace([' ', '_'], "-");
        Self::ALL
            .into_iter()
            .find(|status| status.as_str() == normalized)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Backlog => "backlog",
            Self::Todo => "todo",
            Self::InProgress => "in-progress",
            Self::Review => "review",
            Self::Done => "done",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum IssuePriority {
    #[default]
    None,
    Low,
    Medium,
    High,
    Urgent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum IssueKind {
    #[default]
    Task,
    Bug,
    Feature,
    Art,
    Design,
}

/// Link to the same item in an external tracker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalRef {
    /// e.g. "jira", "linear", "github", "plane", "gitea".
    pub system: String,
    pub key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub status: IssueStatus,
    #[serde(default)]
    pub priority: IssuePriority,
    #[serde(default)]
    pub kind: IssueKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assignee: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<String>,
    /// Project-relative files this issue is about (e.g. a concept sheet).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external: Option<ExternalRef>,
    /// Markdown description.
    #[serde(default)]
    pub body: String,
}

/// Fields for creating an issue; the store assigns id and timestamps.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewIssue {
    pub title: String,
    #[serde(default)]
    pub status: Option<IssueStatus>,
    #[serde(default)]
    pub priority: Option<IssuePriority>,
    #[serde(default)]
    pub kind: Option<IssueKind>,
    #[serde(default)]
    pub assignee: Option<String>,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub files: Vec<String>,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub external: Option<ExternalRef>,
}

fn issues_dir(root: &Path) -> PathBuf {
    metadata_path(root, ISSUES_DIR)
}

fn issue_path(root: &Path, id: &str) -> Result<PathBuf, StudioError> {
    validate_id(id)?;
    Ok(issues_dir(root).join(format!("{id}.toml")))
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// All issues, most urgent and most recently updated first. Unreadable files
/// are skipped (and reported) rather than hiding the whole board.
pub fn list_issues(root: &Path) -> Result<(Vec<Issue>, Vec<String>), StudioError> {
    let dir = issues_dir(root);
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok((Vec::new(), Vec::new()));
        }
        Err(error) => return Err(StudioError::io(&dir, error)),
    };
    let mut issues = Vec::new();
    let mut problems = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("toml") {
            continue;
        }
        match std::fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|raw| toml::from_str::<Issue>(&raw).map_err(|e| e.to_string()))
        {
            Ok(issue) => issues.push(issue),
            Err(message) => problems.push(format!("{}: {message}", path.display())),
        }
    }
    issues.sort_by(|a, b| {
        b.priority
            .cmp(&a.priority)
            .then_with(|| b.updated_at.cmp(&a.updated_at))
    });
    Ok((issues, problems))
}

pub fn get_issue(root: &Path, id: &str) -> Result<Issue, StudioError> {
    let path = issue_path(root, id)?;
    let raw = std::fs::read_to_string(&path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            StudioError::NotFound {
                kind: "issue",
                id: id.to_owned(),
            }
        } else {
            StudioError::io(&path, error)
        }
    })?;
    toml::from_str(&raw).map_err(|e| StudioError::Parse {
        path,
        message: e.to_string(),
    })
}

pub fn create_issue(root: &Path, new: NewIssue) -> Result<Issue, StudioError> {
    let title = new.title.trim();
    if title.is_empty() {
        return Err(StudioError::Invalid("an issue needs a title".into()));
    }
    // Short random ids: readable in commit messages ("fixes #3f9c1a") and
    // collision-safe when several people create issues on different branches.
    let id = loop {
        let candidate = uuid::Uuid::new_v4().simple().to_string()[..6].to_owned();
        if !issue_path(root, &candidate)?.exists() {
            break candidate;
        }
    };
    let stamp = now();
    let issue = Issue {
        id,
        title: title.to_owned(),
        status: new.status.unwrap_or_default(),
        priority: new.priority.unwrap_or_default(),
        kind: new.kind.unwrap_or_default(),
        assignee: new.assignee.filter(|a| !a.trim().is_empty()),
        labels: new.labels,
        files: new.files,
        created_at: stamp.clone(),
        updated_at: stamp,
        external: new.external,
        body: new.body,
    };
    write_issue(root, &issue)?;
    Ok(issue)
}

/// Saves an edited issue, bumping `updated_at`. The id must already exist.
pub fn update_issue(root: &Path, mut issue: Issue) -> Result<Issue, StudioError> {
    let existing = get_issue(root, &issue.id)?;
    if issue.title.trim().is_empty() {
        return Err(StudioError::Invalid("an issue needs a title".into()));
    }
    issue.created_at = existing.created_at;
    issue.updated_at = now();
    write_issue(root, &issue)?;
    Ok(issue)
}

pub fn set_issue_status(root: &Path, id: &str, status: IssueStatus) -> Result<Issue, StudioError> {
    let mut issue = get_issue(root, id)?;
    issue.status = status;
    update_issue(root, issue)
}

pub fn delete_issue(root: &Path, id: &str) -> Result<(), StudioError> {
    let path = issue_path(root, id)?;
    std::fs::remove_file(&path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            StudioError::NotFound {
                kind: "issue",
                id: id.to_owned(),
            }
        } else {
            StudioError::io(&path, error)
        }
    })
}

fn write_issue(root: &Path, issue: &Issue) -> Result<(), StudioError> {
    let body = toml::to_string_pretty(issue).map_err(|e| StudioError::Invalid(e.to_string()))?;
    write_atomic(&issue_path(root, &issue.id)?, &body)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new(title: &str) -> NewIssue {
        NewIssue {
            title: title.into(),
            ..Default::default()
        }
    }

    #[test]
    fn create_list_update_delete() {
        let dir = tempfile::tempdir().unwrap();
        assert!(list_issues(dir.path()).unwrap().0.is_empty());

        let mut bug = create_issue(
            dir.path(),
            NewIssue {
                title: "Crash on load".into(),
                kind: Some(IssueKind::Bug),
                priority: Some(IssuePriority::Urgent),
                body: "Steps:\n1. Open level\n2. Boom".into(),
                ..Default::default()
            },
        )
        .unwrap();
        let art = create_issue(dir.path(), new("Hero concept v2")).unwrap();
        assert_eq!(art.status, IssueStatus::Todo);

        let (issues, problems) = list_issues(dir.path()).unwrap();
        assert!(problems.is_empty());
        assert_eq!(issues.len(), 2);
        assert_eq!(issues[0].id, bug.id, "urgent issues sort first");
        assert_eq!(issues[0].body, bug.body, "multi-line bodies round-trip");

        bug.title = "Crash on level load".into();
        let updated = update_issue(dir.path(), bug.clone()).unwrap();
        assert_eq!(updated.created_at, bug.created_at);

        let moved = set_issue_status(dir.path(), &art.id, IssueStatus::Review).unwrap();
        assert_eq!(moved.status, IssueStatus::Review);

        delete_issue(dir.path(), &art.id).unwrap();
        assert!(matches!(
            get_issue(dir.path(), &art.id),
            Err(StudioError::NotFound { .. })
        ));
    }

    #[test]
    fn rejects_empty_titles_and_unsafe_ids() {
        let dir = tempfile::tempdir().unwrap();
        assert!(create_issue(dir.path(), new("   ")).is_err());
        assert!(get_issue(dir.path(), "../../etc/passwd").is_err());
    }

    #[test]
    fn corrupt_files_are_reported_not_fatal() {
        let dir = tempfile::tempdir().unwrap();
        create_issue(dir.path(), new("Fine")).unwrap();
        std::fs::write(
            dir.path().join(".pipelineos/issues/bad.toml"),
            "not = [valid",
        )
        .unwrap();
        let (issues, problems) = list_issues(dir.path()).unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(problems.len(), 1);
    }

    #[test]
    fn status_parsing_is_forgiving() {
        assert_eq!(
            IssueStatus::parse("In Progress"),
            Some(IssueStatus::InProgress)
        );
        assert_eq!(
            IssueStatus::parse("in_progress"),
            Some(IssueStatus::InProgress)
        );
        assert_eq!(IssueStatus::parse("DONE"), Some(IssueStatus::Done));
        assert_eq!(IssueStatus::parse("nope"), None);
    }
}
