//! Issue-tracker connectors: pull issues from GitHub, Gitea/Forgejo, Jira,
//! Linear, Plane, or ClickUp into the git-native board.
//!
//! The connection (which tracker, which project) is team configuration and
//! lives in `.pipelineos/tracker.toml`. The access token is personal and lives
//! only in the OS keychain (or `PIPELINEOS_TRACKER_TOKEN` for CI), never in
//! project files. Imports are idempotent: each remote item is matched to its
//! local issue by `external.system` + `external.key`.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    StudioError,
    issues::{
        ExternalRef, Issue, IssueKind, IssuePriority, IssueStatus, NewIssue, create_issue,
        list_issues, update_issue,
    },
    metadata_path, write_atomic,
};

pub const TRACKER_FILE: &str = "tracker.toml";
const KEYRING_SERVICE: &str = "PipelineOS tracker";
const USER_AGENT: &str = concat!("PipelineOS/", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrackerKind {
    Github,
    Gitea,
    Jira,
    Linear,
    Plane,
    Clickup,
}

impl TrackerKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Github => "github",
            Self::Gitea => "gitea",
            Self::Jira => "jira",
            Self::Linear => "linear",
            Self::Plane => "plane",
            Self::Clickup => "clickup",
        }
    }
}

/// Shared, secret-free connection settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackerConfig {
    pub kind: TrackerKind,
    /// Server URL for self-hosted trackers (Gitea/Forgejo, Jira, Plane).
    /// GitHub Enterprise uses its API URL. Empty for cloud defaults.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    /// Which project: `owner/repo` (GitHub, Gitea), project key (Jira), team
    /// key (Linear), `workspace-slug/project-id` (Plane), list id (ClickUp).
    pub project: String,
    /// Optional extra filter: a JQL clause for Jira (ANDed with the project).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
}

impl TrackerConfig {
    fn base(&self, default: &str) -> String {
        self.base_url
            .as_deref()
            .filter(|url| !url.trim().is_empty())
            .unwrap_or(default)
            .trim_end_matches('/')
            .to_owned()
    }

    /// Keychain account for this tracker's token, shared across projects on
    /// the same server so one sign-in covers them all.
    pub fn credential_account(&self) -> String {
        format!(
            "{}@{}",
            self.kind.as_str(),
            self.base(default_base(self.kind))
        )
    }
}

fn default_base(kind: TrackerKind) -> &'static str {
    match kind {
        TrackerKind::Github => "https://api.github.com",
        TrackerKind::Linear => "https://api.linear.app",
        TrackerKind::Clickup => "https://api.clickup.com",
        TrackerKind::Gitea | TrackerKind::Jira | TrackerKind::Plane => "",
    }
}

fn config_path(root: &Path) -> PathBuf {
    metadata_path(root, TRACKER_FILE)
}

pub fn load_tracker(root: &Path) -> Result<Option<TrackerConfig>, StudioError> {
    let path = config_path(root);
    match std::fs::read_to_string(&path) {
        Ok(raw) => toml::from_str(&raw)
            .map(Some)
            .map_err(|e| StudioError::Parse {
                path,
                message: e.to_string(),
            }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(StudioError::io(&path, error)),
    }
}

pub fn save_tracker(root: &Path, config: Option<&TrackerConfig>) -> Result<(), StudioError> {
    let path = config_path(root);
    let Some(config) = config else {
        return match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(StudioError::io(&path, error)),
        };
    };
    validate(config)?;
    let body = toml::to_string_pretty(config).map_err(|e| StudioError::Invalid(e.to_string()))?;
    write_atomic(
        &path,
        &format!("# Issue tracker for this project. Tokens are never stored here.\n{body}"),
    )
}

fn validate(config: &TrackerConfig) -> Result<(), StudioError> {
    if config.project.trim().is_empty() {
        return Err(StudioError::Invalid("choose which project to sync".into()));
    }
    let needs_base = matches!(
        config.kind,
        TrackerKind::Gitea | TrackerKind::Jira | TrackerKind::Plane
    );
    let base = config.base_url.as_deref().unwrap_or("").trim();
    if needs_base && base.is_empty() {
        return Err(StudioError::Invalid(
            "this tracker needs its server URL".into(),
        ));
    }
    // Tokens ride on every request, so plain http is only for a server on
    // this machine (the self-hosted Forgejo below).
    let secure = base.is_empty()
        || reqwest::Url::parse(base).is_ok_and(|url| {
            url.username().is_empty() && url.password().is_none() && secure_transport(&url)
        });
    if !secure {
        return Err(StudioError::Invalid(
            "the server URL must start with https://".into(),
        ));
    }
    if matches!(
        config.kind,
        TrackerKind::Github | TrackerKind::Gitea | TrackerKind::Plane
    ) && config.project.split('/').filter(|p| !p.is_empty()).count() != 2
    {
        return Err(StudioError::Invalid(match config.kind {
            TrackerKind::Plane => "use workspace-slug/project-id".into(),
            _ => "use owner/repo".into(),
        }));
    }
    Ok(())
}

// ------------------------------------------------------------ credentials

/// Stores the token in the OS keychain (Windows Credential Manager, macOS
/// Keychain, Linux Secret Service with the kernel keyring as a cache).
pub fn store_token(config: &TrackerConfig, token: &str) -> Result<(), StudioError> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, &config.credential_account())
        .map_err(|e| StudioError::Invalid(format!("keychain unavailable: {e}")))?;
    if token.is_empty() {
        return match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(StudioError::Invalid(format!("keychain: {e}"))),
        };
    }
    entry
        .set_password(token)
        .map_err(|e| StudioError::Invalid(format!("couldn't save the token to the keychain: {e}")))
}

/// The token from `PIPELINEOS_TRACKER_TOKEN` (for CI) or the keychain.
///
/// The environment token goes only to the tracker's own hosted API, or to the
/// server named by `PIPELINEOS_TRACKER_ACCOUNT` (e.g. `jira@https://acme.atlassian.net`),
/// so a project's tracker.toml can't point it at another host.
pub fn load_token(config: &TrackerConfig) -> Option<String> {
    let account = config.credential_account();
    let default_host = config
        .base_url
        .as_deref()
        .is_none_or(|url| url.trim().is_empty())
        && !default_base(config.kind).is_empty();
    let bound = default_host
        || std::env::var("PIPELINEOS_TRACKER_ACCOUNT").is_ok_and(|bound| bound.trim() == account);
    if let Ok(token) = std::env::var("PIPELINEOS_TRACKER_TOKEN")
        && !token.is_empty()
        && bound
    {
        return Some(token);
    }
    keyring::Entry::new(KEYRING_SERVICE, &config.credential_account())
        .ok()?
        .get_password()
        .ok()
}

// ------------------------------------------------------------ remote model

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteIssue {
    pub key: String,
    pub title: String,
    pub body: String,
    pub status: IssueStatus,
    pub kind: IssueKind,
    pub priority: IssuePriority,
    pub assignee: Option<String>,
    pub labels: Vec<String>,
    pub url: Option<String>,
}

fn s(value: &Value, pointer: &str) -> Option<String> {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .filter(|v| !v.is_empty())
}

fn kind_from_labels(labels: &[String], hint: Option<&str>) -> IssueKind {
    let all = labels
        .iter()
        .map(String::as_str)
        .chain(hint)
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>();
    let has = |needle: &str| all.iter().any(|label| label.contains(needle));
    if has("bug") || has("defect") {
        IssueKind::Bug
    } else if has("art") || has("concept") || has("asset") {
        IssueKind::Art
    } else if has("design") || has("ux") {
        IssueKind::Design
    } else if has("feature") || has("story") || has("enhancement") || has("epic") {
        IssueKind::Feature
    } else {
        IssueKind::Task
    }
}

fn priority_from(name: Option<&str>) -> IssuePriority {
    match name.unwrap_or("").to_ascii_lowercase().as_str() {
        n if n.contains("urgent") || n.contains("highest") || n.contains("blocker") || n == "1" => {
            IssuePriority::Urgent
        }
        n if n.contains("high") || n.contains("critical") || n == "2" => IssuePriority::High,
        n if n.contains("medium") || n.contains("normal") || n == "3" => IssuePriority::Medium,
        n if n.contains("low") || n == "4" => IssuePriority::Low,
        _ => IssuePriority::None,
    }
}

/// GitHub and Gitea/Forgejo share the same issue shape.
pub fn parse_github_like(items: &Value, in_progress_label: bool) -> Vec<RemoteIssue> {
    items
        .as_array()
        .into_iter()
        .flatten()
        .filter(|item| item.get("pull_request").is_none_or(Value::is_null))
        .filter_map(|item| {
            let number = item.get("number")?.as_u64()?;
            let labels: Vec<String> = item
                .get("labels")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|label| s(label, "/name"))
                .collect();
            let closed = item.get("state").and_then(Value::as_str) == Some("closed");
            let in_progress = in_progress_label
                && labels.iter().any(|l| {
                    let l = l.to_ascii_lowercase();
                    l.contains("in progress") || l.contains("in-progress") || l == "wip"
                });
            Some(RemoteIssue {
                key: format!("#{number}"),
                title: s(item, "/title").unwrap_or_default(),
                body: s(item, "/body").unwrap_or_default(),
                status: if closed {
                    IssueStatus::Done
                } else if in_progress {
                    IssueStatus::InProgress
                } else {
                    IssueStatus::Todo
                },
                kind: kind_from_labels(&labels, None),
                priority: priority_from(
                    labels
                        .iter()
                        .find(|l| l.to_ascii_lowercase().starts_with("priority"))
                        .map(String::as_str),
                ),
                assignee: s(item, "/assignee/login"),
                labels,
                url: s(item, "/html_url"),
            })
        })
        .collect()
}

/// Flattens Atlassian Document Format (Jira Cloud v3 descriptions) to text.
fn adf_text(node: &Value) -> String {
    match node {
        Value::String(text) => text.clone(),
        Value::Object(map) => {
            if let Some(text) = map.get("text").and_then(Value::as_str) {
                return text.to_owned();
            }
            let children = map
                .get("content")
                .and_then(Value::as_array)
                .map(|items| items.iter().map(adf_text).collect::<Vec<_>>())
                .unwrap_or_default();
            let joined = children.join("");
            match map.get("type").and_then(Value::as_str) {
                Some("paragraph" | "heading" | "listItem" | "codeBlock") => format!("{joined}\n"),
                _ => joined,
            }
        }
        _ => String::new(),
    }
}

pub fn parse_jira(response: &Value, base: &str) -> Vec<RemoteIssue> {
    response
        .get("issues")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let key = s(item, "/key")?;
            let fields = item.get("fields")?;
            let labels: Vec<String> = fields
                .get("labels")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|l| l.as_str().map(str::to_owned))
                .collect();
            let status = match s(fields, "/status/statusCategory/key").as_deref() {
                Some("done") => IssueStatus::Done,
                Some("indeterminate") => IssueStatus::InProgress,
                _ => IssueStatus::Todo,
            };
            let issue_type = s(fields, "/issuetype/name");
            Some(RemoteIssue {
                url: Some(format!("{base}/browse/{key}")),
                key,
                title: s(fields, "/summary").unwrap_or_default(),
                body: fields
                    .get("description")
                    .map(adf_text)
                    .unwrap_or_default()
                    .trim()
                    .to_owned(),
                status,
                kind: kind_from_labels(&labels, issue_type.as_deref()),
                priority: priority_from(s(fields, "/priority/name").as_deref()),
                assignee: s(fields, "/assignee/displayName"),
                labels,
            })
        })
        .collect()
}

fn state_group(group: Option<&str>) -> IssueStatus {
    match group.unwrap_or("") {
        "backlog" | "triage" => IssueStatus::Backlog,
        "started" => IssueStatus::InProgress,
        "completed" | "canceled" | "cancelled" => IssueStatus::Done,
        _ => IssueStatus::Todo,
    }
}

pub fn parse_linear(response: &Value) -> Vec<RemoteIssue> {
    response
        .pointer("/data/issues/nodes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let labels: Vec<String> = item
                .pointer("/labels/nodes")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|l| s(l, "/name"))
                .collect();
            Some(RemoteIssue {
                key: s(item, "/identifier")?,
                title: s(item, "/title").unwrap_or_default(),
                body: s(item, "/description").unwrap_or_default(),
                status: state_group(item.pointer("/state/type").and_then(Value::as_str)),
                kind: kind_from_labels(&labels, None),
                priority: match item.get("priority").and_then(Value::as_u64) {
                    Some(1) => IssuePriority::Urgent,
                    Some(2) => IssuePriority::High,
                    Some(3) => IssuePriority::Medium,
                    Some(4) => IssuePriority::Low,
                    _ => IssuePriority::None,
                },
                assignee: s(item, "/assignee/name"),
                labels,
                url: s(item, "/url"),
            })
        })
        .collect()
}

pub fn parse_plane(response: &Value, base: &str, project: &str) -> Vec<RemoteIssue> {
    let list = response.get("results").unwrap_or(response);
    list.as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let id = s(item, "/id")?;
            let sequence = item.get("sequence_id").and_then(Value::as_u64);
            let labels: Vec<String> = item
                .get("labels")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|l| s(l, "/name"))
                .collect();
            let (workspace, project_id) = project.split_once('/').unwrap_or((project, ""));
            Some(RemoteIssue {
                key: sequence
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| id.clone()),
                title: s(item, "/name").unwrap_or_default(),
                body: s(item, "/description_stripped").unwrap_or_default(),
                status: state_group(item.pointer("/state/group").and_then(Value::as_str)),
                kind: kind_from_labels(&labels, None),
                priority: priority_from(s(item, "/priority").as_deref()),
                assignee: item
                    .get("assignees")
                    .and_then(Value::as_array)
                    .and_then(|a| a.first())
                    .and_then(|a| s(a, "/display_name")),
                labels,
                url: Some(format!(
                    "{base}/{workspace}/projects/{project_id}/issues/{id}"
                )),
            })
        })
        .collect()
}

pub fn parse_clickup(response: &Value) -> Vec<RemoteIssue> {
    response
        .get("tasks")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let labels: Vec<String> = item
                .get("tags")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|t| s(t, "/name"))
                .collect();
            let status_name = s(item, "/status/status")
                .unwrap_or_default()
                .to_ascii_lowercase();
            let status = match s(item, "/status/type").as_deref() {
                Some("closed" | "done") => IssueStatus::Done,
                _ if status_name.contains("review") => IssueStatus::Review,
                _ if status_name.contains("progress") => IssueStatus::InProgress,
                _ if status_name.contains("backlog") => IssueStatus::Backlog,
                _ => IssueStatus::Todo,
            };
            Some(RemoteIssue {
                key: s(item, "/custom_id").or_else(|| s(item, "/id"))?,
                title: s(item, "/name").unwrap_or_default(),
                body: s(item, "/text_content")
                    .or_else(|| s(item, "/description"))
                    .unwrap_or_default(),
                status,
                kind: kind_from_labels(&labels, None),
                priority: priority_from(s(item, "/priority/priority").as_deref()),
                assignee: item
                    .get("assignees")
                    .and_then(Value::as_array)
                    .and_then(|a| a.first())
                    .and_then(|a| s(a, "/username")),
                labels,
                url: s(item, "/url"),
            })
        })
        .collect()
}

// ------------------------------------------------------------ network

/// https anywhere, or plain http to this machine only (by parsed host, so
/// `http://localhost@evil.example` doesn't count).
fn secure_transport(url: &reqwest::Url) -> bool {
    match url.scheme() {
        "https" => true,
        "http" => url.host_str().is_some_and(|host| {
            host.eq_ignore_ascii_case("localhost")
                || host
                    .trim_start_matches('[')
                    .trim_end_matches(']')
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|ip| ip.is_loopback())
        }),
        _ => false,
    }
}

fn client() -> Result<reqwest::Client, StudioError> {
    // Same TLS stack as the auto-updater: rustls with the ring provider.
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        let _ = rustls::crypto::ring::default_provider().install_default();
    }
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(std::time::Duration::from_secs(30))
        // Trackers send tokens in custom headers (Plane's X-API-Key) that
        // reqwest doesn't strip on redirect, so only follow redirects that
        // stay on the same origin over a secure transport.
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            let same_origin = attempt
                .previous()
                .last()
                .is_some_and(|previous| previous.origin() == attempt.url().origin());
            if attempt.previous().len() < 5 && same_origin && secure_transport(attempt.url()) {
                attempt.follow()
            } else {
                attempt.stop()
            }
        }))
        .build()
        .map_err(|e| StudioError::Invalid(format!("HTTP client: {e}")))
}

async fn send(request: reqwest::RequestBuilder, what: &str) -> Result<Value, StudioError> {
    let response = request
        .send()
        .await
        .map_err(|e| StudioError::Invalid(format!("couldn't reach {what}: {e}")))?;
    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return Err(StudioError::Invalid(format!(
            "{what} refused the request ({status}). Check the access token and its permissions."
        )));
    }
    if status == reqwest::StatusCode::NOT_FOUND {
        return Err(StudioError::NotFound {
            kind: "tracker project",
            id: what.to_owned(),
        });
    }
    if !status.is_success() {
        let text = response.text().await.unwrap_or_default();
        return Err(StudioError::Invalid(format!(
            "{what} returned {status}: {}",
            text.chars().take(300).collect::<String>()
        )));
    }
    response
        .json::<Value>()
        .await
        .map_err(|e| StudioError::Invalid(format!("{what} sent an unexpected response: {e}")))
}

/// Fetches up to ~100 recent issues from the configured tracker.
pub async fn fetch_issues(
    config: &TrackerConfig,
    token: Option<&str>,
) -> Result<Vec<RemoteIssue>, StudioError> {
    validate(config)?;
    let http = client()?;
    let token = token.filter(|t| !t.is_empty());
    match config.kind {
        TrackerKind::Github => {
            let base = config.base(default_base(config.kind));
            let mut request = http
                .get(format!("{base}/repos/{}/issues", config.project))
                .query(&[("state", "all"), ("per_page", "100"), ("sort", "updated")])
                .header("Accept", "application/vnd.github+json");
            if let Some(token) = token {
                request = request.bearer_auth(token);
            }
            Ok(parse_github_like(&send(request, "GitHub").await?, true))
        }
        TrackerKind::Gitea => {
            let base = config.base("");
            let mut request = http
                .get(format!("{base}/api/v1/repos/{}/issues", config.project))
                .query(&[("state", "all"), ("type", "issues"), ("limit", "50")]);
            if let Some(token) = token {
                request = request.header("Authorization", format!("token {token}"));
            }
            Ok(parse_github_like(
                &send(request, "Gitea/Forgejo").await?,
                true,
            ))
        }
        TrackerKind::Jira => {
            let base = config.base("");
            let mut jql = format!("project = \"{}\"", config.project.replace('"', ""));
            if let Some(extra) = config.query.as_deref().filter(|q| !q.trim().is_empty()) {
                jql.push_str(&format!(" AND ({extra})"));
            }
            jql.push_str(" ORDER BY updated DESC");
            let fields = "summary,description,status,assignee,labels,issuetype,priority";
            let auth = |request: reqwest::RequestBuilder| match token {
                // Jira Cloud: "email:api-token" as Basic; Data Center: a PAT as Bearer.
                Some(token) if token.contains(':') => {
                    use base64::Engine;
                    request.header(
                        "Authorization",
                        format!(
                            "Basic {}",
                            base64::engine::general_purpose::STANDARD.encode(token)
                        ),
                    )
                }
                Some(token) => request.bearer_auth(token),
                None => request,
            };
            let query = [
                ("jql", jql.as_str()),
                ("fields", fields),
                ("maxResults", "100"),
            ];
            // Jira Cloud's current search API, then the classic one (Data Center).
            let modern = auth(
                http.get(format!("{base}/rest/api/3/search/jql"))
                    .query(&query),
            );
            let response = match send(modern, "Jira").await {
                Ok(value) => value,
                Err(StudioError::NotFound { .. }) => {
                    send(
                        auth(http.get(format!("{base}/rest/api/2/search")).query(&query)),
                        "Jira",
                    )
                    .await?
                }
                Err(error) => return Err(error),
            };
            Ok(parse_jira(&response, &base))
        }
        TrackerKind::Linear => {
            let token =
                token.ok_or_else(|| StudioError::Invalid("Linear needs an API key".into()))?;
            let query = "query($team: String!) { issues(first: 100, orderBy: updatedAt, filter: { team: { key: { eq: $team } } }) { nodes { identifier title description url priority state { type } assignee { name } labels { nodes { name } } } } }";
            let request = http
                .post(format!(
                    "{}/graphql",
                    config.base(default_base(config.kind))
                ))
                .header("Authorization", token)
                .json(&json!({ "query": query, "variables": { "team": config.project } }));
            let response = send(request, "Linear").await?;
            if let Some(errors) = response.get("errors") {
                return Err(StudioError::Invalid(format!("Linear: {errors}")));
            }
            Ok(parse_linear(&response))
        }
        TrackerKind::Plane => {
            let base = config.base("");
            let (workspace, project) = config.project.split_once('/').unwrap_or_default();
            let token =
                token.ok_or_else(|| StudioError::Invalid("Plane needs an API key".into()))?;
            let request = http
                .get(format!(
                    "{base}/api/v1/workspaces/{workspace}/projects/{project}/issues/"
                ))
                .query(&[("expand", "state,assignees,labels"), ("per_page", "100")])
                .header("X-API-Key", token);
            Ok(parse_plane(
                &send(request, "Plane").await?,
                &base,
                &config.project,
            ))
        }
        TrackerKind::Clickup => {
            let token =
                token.ok_or_else(|| StudioError::Invalid("ClickUp needs an API token".into()))?;
            let request = http
                .get(format!(
                    "{}/api/v2/list/{}/task",
                    config.base(default_base(config.kind)),
                    config.project
                ))
                .query(&[("include_closed", "true"), ("subtasks", "true")])
                .header("Authorization", token);
            Ok(parse_clickup(&send(request, "ClickUp").await?))
        }
    }
}

// ------------------------------------------------------------ merge

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncSummary {
    pub created: usize,
    pub updated: usize,
    pub unchanged: usize,
}

/// Merges remote issues into the local board. The tracker is the source of
/// truth for imported fields; local-only fields (files, local labels added on
/// top) are kept when unchanged remotely.
pub fn merge_remote(
    root: &Path,
    system: TrackerKind,
    remote: &[RemoteIssue],
) -> Result<SyncSummary, StudioError> {
    let (existing, _) = list_issues(root)?;
    let mut summary = SyncSummary::default();
    for item in remote {
        let local = existing.iter().find(|issue| {
            issue
                .external
                .as_ref()
                .is_some_and(|ext| ext.system == system.as_str() && ext.key == item.key)
        });
        let external = ExternalRef {
            system: system.as_str().into(),
            key: item.key.clone(),
            url: item.url.clone(),
        };
        match local {
            None => {
                create_issue(
                    root,
                    NewIssue {
                        title: if item.title.trim().is_empty() {
                            item.key.clone()
                        } else {
                            item.title.clone()
                        },
                        status: Some(item.status),
                        priority: Some(item.priority),
                        kind: Some(item.kind),
                        assignee: item.assignee.clone(),
                        labels: item.labels.clone(),
                        files: Vec::new(),
                        body: item.body.clone(),
                        external: Some(external),
                    },
                )?;
                summary.created += 1;
            }
            Some(issue) => {
                let next = Issue {
                    title: if item.title.trim().is_empty() {
                        issue.title.clone()
                    } else {
                        item.title.clone()
                    },
                    status: item.status,
                    priority: item.priority,
                    assignee: item.assignee.clone(),
                    labels: item.labels.clone(),
                    body: item.body.clone(),
                    external: Some(external),
                    ..issue.clone()
                };
                if next.title == issue.title
                    && next.status == issue.status
                    && next.priority == issue.priority
                    && next.assignee == issue.assignee
                    && next.labels == issue.labels
                    && next.body == issue.body
                    && next.external == issue.external
                {
                    summary.unchanged += 1;
                } else {
                    update_issue(root, next)?;
                    summary.updated += 1;
                }
            }
        }
    }
    Ok(summary)
}

/// Fetch + merge in one step.
pub async fn sync_tracker(root: &Path) -> Result<SyncSummary, StudioError> {
    let config = load_tracker(root)?
        .ok_or_else(|| StudioError::Invalid("connect a tracker for this project first".into()))?;
    let token = load_token(&config);
    let remote = fetch_issues(&config, token.as_deref()).await?;
    merge_remote(root, config.kind, &remote)
}

// ------------------------------------------------------------ self-hosting

/// A docker-compose file for a self-hosted Forgejo: Git hosting with LFS,
/// issues, pull requests, and CI in one lightweight container. Studios that
/// want their own server can run it on any machine with Docker.
pub fn forgejo_compose(http_port: u16, ssh_port: u16) -> String {
    format!(
        r#"# Self-hosted Forgejo for your team: Git + LFS + issues + pull requests.
# Generated by Pipeline OS. Start with:  docker compose up -d
# Then open http://localhost:{http_port} to finish setup (first user = admin).
# Connect projects in Pipeline OS with tracker "Gitea / Forgejo" and this URL.
services:
  forgejo:
    image: codeberg.org/forgejo/forgejo:15
    container_name: pipelineos-forgejo
    restart: unless-stopped
    environment:
      - USER_UID=1000
      - USER_GID=1000
      - FORGEJO__server__ROOT_URL=http://localhost:{http_port}/
      - FORGEJO__server__LFS_START_SERVER=true
      - FORGEJO__server__SSH_PORT={ssh_port}
    volumes:
      - ./data:/data
    ports:
      - "{http_port}:3000"
      - "{ssh_port}:22"
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_round_trips_without_secrets() {
        let dir = tempfile::tempdir().unwrap();
        assert!(load_tracker(dir.path()).unwrap().is_none());
        let config = TrackerConfig {
            kind: TrackerKind::Jira,
            base_url: Some("https://studio.atlassian.net".into()),
            project: "VOID".into(),
            query: Some("labels = art".into()),
        };
        save_tracker(dir.path(), Some(&config)).unwrap();
        assert_eq!(load_tracker(dir.path()).unwrap(), Some(config.clone()));
        assert_eq!(
            config.credential_account(),
            "jira@https://studio.atlassian.net"
        );
        save_tracker(dir.path(), None).unwrap();
        assert!(load_tracker(dir.path()).unwrap().is_none());
    }

    #[test]
    fn validation_catches_common_mistakes() {
        let base = TrackerConfig {
            kind: TrackerKind::Github,
            base_url: None,
            project: "only-owner".into(),
            query: None,
        };
        assert!(validate(&base).is_err());
        assert!(
            validate(&TrackerConfig {
                project: "owner/repo".into(),
                ..base.clone()
            })
            .is_ok()
        );
        assert!(
            validate(&TrackerConfig {
                kind: TrackerKind::Jira,
                project: "KEY".into(),
                ..base.clone()
            })
            .is_err(),
            "Jira needs a URL"
        );
        assert!(
            validate(&TrackerConfig {
                kind: TrackerKind::Jira,
                project: "KEY".into(),
                base_url: Some("ftp://x".into()),
                query: None
            })
            .is_err()
        );
    }

    #[test]
    fn plain_http_only_reaches_this_machine() {
        let ok = |url: &str| {
            reqwest::Url::parse(url).is_ok_and(|url| {
                url.username().is_empty() && url.password().is_none() && secure_transport(&url)
            })
        };
        assert!(ok("https://acme.atlassian.net"));
        assert!(ok("http://localhost:3000"));
        assert!(ok("http://127.0.0.1/"));
        assert!(ok("http://[::1]:8080"));
        assert!(!ok("http://example.com"));
        assert!(!ok("http://localhost:3000@attacker.example"));
        assert!(!ok("http://localhost.attacker.example"));
        assert!(!ok("ftp://localhost"));
    }

    #[test]
    fn parses_github_issues_and_skips_pull_requests() {
        let items = json!([
            { "number": 7, "title": "Crash on load", "body": "boom", "state": "open", "html_url": "https://github.com/o/r/issues/7",
              "labels": [{ "name": "bug" }, { "name": "in progress" }], "assignee": { "login": "sam" } },
            { "number": 8, "title": "A PR", "state": "open", "pull_request": { "url": "x" } },
            { "number": 9, "title": "Concept pass", "state": "closed", "labels": [{ "name": "art" }], "assignee": null }
        ]);
        let issues = parse_github_like(&items, true);
        assert_eq!(issues.len(), 2);
        assert_eq!(issues[0].key, "#7");
        assert_eq!(issues[0].status, IssueStatus::InProgress);
        assert_eq!(issues[0].kind, IssueKind::Bug);
        assert_eq!(issues[0].assignee.as_deref(), Some("sam"));
        assert_eq!(issues[1].status, IssueStatus::Done);
        assert_eq!(issues[1].kind, IssueKind::Art);
    }

    #[test]
    fn parses_jira_including_adf_descriptions() {
        let response = json!({ "issues": [{
            "key": "VOID-12",
            "fields": {
                "summary": "Lightmap seams",
                "description": { "type": "doc", "content": [{ "type": "paragraph", "content": [{ "type": "text", "text": "Seams on " }, { "type": "text", "text": "level 2" }] }] },
                "status": { "statusCategory": { "key": "indeterminate" } },
                "assignee": { "displayName": "Ana" },
                "labels": ["lighting"],
                "issuetype": { "name": "Bug" },
                "priority": { "name": "Highest" }
            }
        }]});
        let issues = parse_jira(&response, "https://studio.atlassian.net");
        assert_eq!(issues[0].key, "VOID-12");
        assert_eq!(issues[0].body, "Seams on level 2");
        assert_eq!(issues[0].status, IssueStatus::InProgress);
        assert_eq!(issues[0].kind, IssueKind::Bug);
        assert_eq!(issues[0].priority, IssuePriority::Urgent);
        assert_eq!(
            issues[0].url.as_deref(),
            Some("https://studio.atlassian.net/browse/VOID-12")
        );
    }

    #[test]
    fn parses_linear_plane_and_clickup() {
        let linear = parse_linear(&json!({ "data": { "issues": { "nodes": [
            { "identifier": "ENG-4", "title": "Save slots", "description": null, "url": "https://linear.app/x/issue/ENG-4", "priority": 2,
              "state": { "type": "backlog" }, "assignee": null, "labels": { "nodes": [{ "name": "Feature" }] } }
        ] } } }));
        assert_eq!(linear[0].status, IssueStatus::Backlog);
        assert_eq!(linear[0].priority, IssuePriority::High);
        assert_eq!(linear[0].kind, IssueKind::Feature);

        let plane = parse_plane(
            &json!({ "results": [
            { "id": "uuid-1", "sequence_id": 31, "name": "Rig hero", "description_stripped": "IK arms", "priority": "medium",
              "state": { "group": "started" }, "assignees": [{ "display_name": "lee" }], "labels": [{ "name": "art" }] }
        ] }),
            "https://plane.studio.local",
            "studio/proj-1",
        );
        assert_eq!(plane[0].key, "31");
        assert_eq!(plane[0].status, IssueStatus::InProgress);
        assert_eq!(plane[0].assignee.as_deref(), Some("lee"));
        assert_eq!(
            plane[0].url.as_deref(),
            Some("https://plane.studio.local/studio/projects/proj-1/issues/uuid-1")
        );

        let clickup = parse_clickup(&json!({ "tasks": [
            { "id": "86x", "name": "Trailer edit", "text_content": "v2 cut", "status": { "status": "in review", "type": "custom" },
              "url": "https://app.clickup.com/t/86x", "assignees": [{ "username": "kai" }], "tags": [{ "name": "design" }], "priority": { "priority": "urgent" } }
        ] }));
        assert_eq!(clickup[0].status, IssueStatus::Review);
        assert_eq!(clickup[0].priority, IssuePriority::Urgent);
        assert_eq!(clickup[0].kind, IssueKind::Design);
    }

    #[test]
    fn merge_is_idempotent_and_updates_changes() {
        let dir = tempfile::tempdir().unwrap();
        let mut remote = vec![RemoteIssue {
            key: "#1".into(),
            title: "Fix jump".into(),
            body: "".into(),
            status: IssueStatus::Todo,
            kind: IssueKind::Bug,
            priority: IssuePriority::High,
            assignee: None,
            labels: vec!["bug".into()],
            url: Some("https://github.com/o/r/issues/1".into()),
        }];
        let first = merge_remote(dir.path(), TrackerKind::Github, &remote).unwrap();
        assert_eq!((first.created, first.updated, first.unchanged), (1, 0, 0));
        let again = merge_remote(dir.path(), TrackerKind::Github, &remote).unwrap();
        assert_eq!((again.created, again.updated, again.unchanged), (0, 0, 1));
        remote[0].status = IssueStatus::Done;
        let changed = merge_remote(dir.path(), TrackerKind::Github, &remote).unwrap();
        assert_eq!(changed.updated, 1);
        let (issues, _) = list_issues(dir.path()).unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].status, IssueStatus::Done);
        assert_eq!(issues[0].external.as_ref().unwrap().system, "github");
    }

    #[test]
    fn forgejo_compose_pins_a_version_and_ports() {
        let compose = forgejo_compose(3000, 2222);
        assert!(compose.contains("codeberg.org/forgejo/forgejo:15"));
        assert!(compose.contains("\"3000:3000\"") && compose.contains("\"2222:22\""));
        assert!(compose.contains("LFS_START_SERVER=true"));
    }

    /// Live check against GitHub's public API (skipped without network).
    #[tokio::test]
    #[ignore = "network"]
    async fn fetches_public_github_issues() {
        let config = TrackerConfig {
            kind: TrackerKind::Github,
            base_url: None,
            project: "tauri-apps/tauri".into(),
            query: None,
        };
        let issues = fetch_issues(&config, None).await.unwrap();
        assert!(!issues.is_empty());
    }
}
