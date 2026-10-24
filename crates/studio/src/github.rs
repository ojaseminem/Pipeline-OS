//! GitHub integration for the source-control view: pull requests, checks,
//! Actions re-runs, and the permission checks behind safe remote-branch
//! deletion.
//!
//! The repository comes from the project's git remote (github.com or a
//! GitHub Enterprise host). The token is personal: it lives in the OS
//! keychain (service `PipelineOS GitHub`, account = host), or comes from
//! `GH_TOKEN`/`GITHUB_TOKEN` (`GH_ENTERPRISE_TOKEN` for the Enterprise Server
//! named by `GH_HOST`), or from a signed-in GitHub CLI (`gh auth token`) —
//! never project files. A repository's remote URL alone never decides where a
//! token is sent: hosts other than github.com / *.ghe.com must be set up.

use std::collections::{BTreeMap, HashMap};

use reqwest::{Method, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::StudioError;

pub const KEYRING_SERVICE: &str = "PipelineOS GitHub";
const USER_AGENT: &str = concat!("PipelineOS/", env!("CARGO_PKG_VERSION"));
const API_VERSION: &str = "2022-11-28";
/// Lines of a failed job's log returned for display.
pub const LOG_TAIL_LINES: usize = 200;

// ------------------------------------------------------------ repository

/// A GitHub repository resolved from a git remote URL.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitHubRepo {
    /// `github.com` or the Enterprise host.
    pub host: String,
    pub owner: String,
    pub repo: String,
    /// REST API root: `https://api.github.com` or `https://<host>/api/v3`.
    pub api_base: String,
    pub html_url: String,
}

impl GitHubRepo {
    pub fn full_name(&self) -> String {
        format!("{}/{}", self.owner, self.repo)
    }

    fn graphql_url(&self) -> String {
        if self.host == "github.com" || self.host.ends_with(".ghe.com") {
            format!("{}/graphql", self.api_base)
        } else {
            format!("https://{}/api/graphql", self.host)
        }
    }
}

/// REST API root for a host. github.com and GitHub Enterprise Cloud with
/// data residency (`*.ghe.com`) use an `api.` subdomain; Enterprise Server
/// serves the API under `/api/v3`.
pub fn api_base_for_host(host: &str) -> String {
    if host == "github.com" {
        "https://api.github.com".into()
    } else if host.ends_with(".ghe.com") {
        format!("https://api.{host}")
    } else {
        format!("https://{host}/api/v3")
    }
}

/// Splits a git remote URL into (host, owner, repo). Handles
/// `https://[user@]host[:port]/owner/repo(.git)`, `ssh://git@host[:port]/owner/repo`,
/// and scp-style `git@host:owner/repo.git`. SSH host aliases such as
/// `github.com-work` (multi-account SSH configs) map back to `github.com`.
pub fn parse_remote_url(url: &str) -> Option<(String, String, String)> {
    let url = url.trim();
    let (host, path) = if let Some((scheme, rest)) = url.split_once("://") {
        if !matches!(scheme, "https" | "http" | "ssh" | "git" | "git+ssh") {
            return None;
        }
        let (authority, path) = rest.split_once('/')?;
        let host = authority.rsplit('@').next()?;
        let host = host.split(':').next()?;
        (host, path)
    } else {
        // scp-like: [user@]host:owner/repo
        let (authority, path) = url.split_once(':')?;
        if authority.contains('/') {
            return None;
        }
        (authority.rsplit('@').next()?, path)
    };
    let mut host = host.to_ascii_lowercase();
    if host.starts_with("github.com-") || host == "www.github.com" || host == "ssh.github.com" {
        host = "github.com".into();
    }
    let path = path.trim_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    let mut parts = path.split('/').filter(|part| !part.is_empty());
    let owner = parts.next()?.to_owned();
    let repo = parts.next()?.to_owned();
    if parts.next().is_some() || host.is_empty() {
        return None;
    }
    Some((host, owner, repo))
}

fn is_public_github(host: &str) -> bool {
    host == "github.com" || host.ends_with(".ghe.com")
}

/// The Enterprise Server host the user configured with `GH_HOST`, if any.
fn configured_enterprise_host() -> Option<String> {
    std::env::var("GH_HOST")
        .ok()
        .map(|host| host.trim().to_ascii_lowercase())
        .filter(|host| !host.is_empty())
}

impl GitHubRepo {
    /// Resolves a GitHub repo from a remote URL. Enterprise Server hosts
    /// count only once the user has set them up: a stored token, `GH_HOST`,
    /// or a GitHub CLI sign-in for that exact host. Blocking.
    pub fn from_remote_url(url: &str) -> Option<Self> {
        let (host, owner, repo) = parse_remote_url(url)?;
        let set_up = || {
            configured_enterprise_host().as_deref() == Some(host.as_str())
                || load_keychain_token(&host).is_some()
                || gh_cli_token(&host).is_some()
        };
        if !is_public_github(&host) && !set_up() {
            return None;
        }
        Some(Self::new(&host, &owner, &repo))
    }

    pub fn new(host: &str, owner: &str, repo: &str) -> Self {
        Self {
            api_base: api_base_for_host(host),
            html_url: format!("https://{host}/{owner}/{repo}"),
            host: host.to_owned(),
            owner: owner.to_owned(),
            repo: repo.to_owned(),
        }
    }
}

// ------------------------------------------------------------ credentials

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TokenSource {
    Keychain,
    Env,
    GhCli,
    None,
}

fn keychain_entry(host: &str) -> Result<keyring::Entry, StudioError> {
    keyring::Entry::new(KEYRING_SERVICE, host)
        .map_err(|e| StudioError::Invalid(format!("keychain unavailable: {e}")))
}

fn load_keychain_token(host: &str) -> Option<String> {
    keychain_entry(host)
        .ok()?
        .get_password()
        .ok()
        .filter(|token| !token.is_empty())
}

/// Saves a token for `host` in the OS keychain.
pub fn set_token(host: &str, token: &str) -> Result<(), StudioError> {
    let token = token.trim();
    if token.is_empty() {
        return clear_token(host);
    }
    keychain_entry(host)?
        .set_password(token)
        .map_err(|e| StudioError::Invalid(format!("couldn't save the token to the keychain: {e}")))
}

/// Removes the stored token for `host` (env/gh tokens are unaffected).
pub fn clear_token(host: &str) -> Result<(), StudioError> {
    match keychain_entry(host)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(StudioError::Invalid(format!("keychain: {e}"))),
    }
}

fn env_token(host: &str) -> Option<String> {
    let names: &[&str] = if is_public_github(host) {
        &["GH_TOKEN", "GITHUB_TOKEN"]
    } else if configured_enterprise_host().as_deref() == Some(host) {
        &["GH_ENTERPRISE_TOKEN", "GITHUB_ENTERPRISE_TOKEN"]
    } else {
        return None;
    };
    names
        .iter()
        .find_map(|name| std::env::var(name).ok().filter(|t| !t.trim().is_empty()))
        .map(|token| token.trim().to_owned())
}

/// `gh auth token --hostname <host>`, when the GitHub CLI is installed and
/// signed in. Blocking.
fn gh_cli_token(host: &str) -> Option<String> {
    let mut command = std::process::Command::new("gh");
    command
        .args(["auth", "token", "--hostname", host])
        .stdin(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let output = command.output().ok()?;
    if !output.status.success() {
        return None;
    }
    let token = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    (!token.is_empty()).then_some(token)
}

/// The token to use for `host` and where it came from: keychain, then
/// environment, then the GitHub CLI. Blocking (keychain + `gh`).
pub fn resolve_token(host: &str) -> Option<(String, TokenSource)> {
    if let Some(token) = load_keychain_token(host) {
        return Some((token, TokenSource::Keychain));
    }
    if let Some(token) = env_token(host) {
        return Some((token, TokenSource::Env));
    }
    gh_cli_token(host).map(|token| (token, TokenSource::GhCli))
}

/// Where the token for `host` would come from.
pub fn token_source(host: &str) -> TokenSource {
    resolve_token(host).map_or(TokenSource::None, |(_, source)| source)
}

// ------------------------------------------------------------ models

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Viewer {
    pub login: String,
    pub name: Option<String>,
    pub avatar_url: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoPermissions {
    pub admin: bool,
    pub maintain: bool,
    pub push: bool,
    pub triage: bool,
    pub pull: bool,
}

impl RepoPermissions {
    /// The highest role these permissions imply.
    pub fn role(&self) -> &'static str {
        if self.admin {
            "admin"
        } else if self.maintain {
            "maintain"
        } else if self.push {
            "write"
        } else if self.triage {
            "triage"
        } else if self.pull {
            "read"
        } else {
            "none"
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoInfo {
    pub full_name: String,
    pub default_branch: String,
    pub private: bool,
    pub visibility: String,
    pub html_url: String,
    pub archived: bool,
    /// The viewer's permissions; `None` without a token.
    pub permissions: Option<RepoPermissions>,
    pub allow_merge_commit: bool,
    pub allow_squash_merge: bool,
    pub allow_rebase_merge: bool,
    pub delete_branch_on_merge: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequest {
    pub number: u64,
    pub title: String,
    pub body: Option<String>,
    pub url: String,
    /// `open`, `closed`, or `merged`.
    pub state: String,
    pub draft: bool,
    pub author: Option<String>,
    pub author_avatar: Option<String>,
    pub head: String,
    pub base: String,
    pub head_sha: String,
    /// `owner/repo` of the head branch (differs from the base for forks).
    pub head_repo: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    /// `approved`, `changes_requested`, `review_required`, or `None`.
    pub review_decision: Option<String>,
    /// `None` while GitHub is still computing it.
    pub mergeable: Option<bool>,
    /// GitHub's merge state: `clean`, `dirty`, `blocked`, `behind`,
    /// `unstable`, `draft`, `has_hooks`, `unknown`.
    pub mergeable_state: Option<String>,
    /// Rollup of the head commit's checks, when known.
    pub checks: Option<Rollup>,
    pub requested_reviewers: Vec<String>,
    pub additions: Option<u64>,
    pub deletions: Option<u64>,
    pub changed_files: Option<u64>,
    #[serde(skip)]
    node_id: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewPullRequest {
    pub title: String,
    #[serde(default)]
    pub body: Option<String>,
    pub base: String,
    pub head: String,
    #[serde(default)]
    pub draft: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequestUpdate {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    /// `Some(false)` marks a draft ready for review; `Some(true)` converts
    /// it back to a draft.
    #[serde(default)]
    pub draft: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MergeMethod {
    Merge,
    Squash,
    Rebase,
}

impl MergeMethod {
    fn as_str(self) -> &'static str {
        match self {
            Self::Merge => "merge",
            Self::Squash => "squash",
            Self::Rebase => "rebase",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeResult {
    pub merged: bool,
    pub sha: Option<String>,
    pub message: String,
}

/// Overall state of a commit's checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Rollup {
    Success,
    Failure,
    Pending,
    Neutral,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckItem {
    pub name: String,
    /// `checkRun` (Checks API) or `status` (legacy commit status).
    pub kind: String,
    /// `queued`, `in_progress`, or `completed`.
    pub status: String,
    /// For completed checks: `success`, `failure`, `neutral`, `cancelled`,
    /// `skipped`, `timed_out`, `action_required`, `stale`, `error`.
    pub conclusion: Option<String>,
    pub url: Option<String>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    /// The app or context that reported it (e.g. "GitHub Actions").
    pub app: Option<String>,
    pub description: Option<String>,
    /// GitHub Actions run/job ids (from the check's details URL), for
    /// re-runs and log fetching.
    pub run_id: Option<u64>,
    pub job_id: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChecksSummary {
    pub git_ref: String,
    pub sha: Option<String>,
    pub rollup: Rollup,
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub pending: usize,
    pub items: Vec<CheckItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowRun {
    pub id: u64,
    pub name: String,
    pub workflow_id: u64,
    pub status: String,
    pub conclusion: Option<String>,
    pub run_attempt: u32,
    pub html_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetriedRun {
    pub run_id: u64,
    pub name: String,
    /// The attempt number that's now starting.
    pub attempt: u32,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkippedRun {
    pub run_id: u64,
    pub name: String,
    pub reason: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetryReport {
    pub retried: Vec<RetriedRun>,
    pub skipped: Vec<SkippedRun>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobLog {
    pub job_id: u64,
    pub name: String,
    pub url: Option<String>,
    pub conclusion: Option<String>,
    pub failed_step: Option<String>,
    /// The last `LOG_TAIL_LINES` lines, timestamps stripped.
    pub log_tail: String,
    pub truncated: bool,
}

/// Whether an action is allowed, and why (shown to the user either way).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Decision {
    pub allowed: bool,
    pub reason: String,
}

/// What's known about a remote branch when deciding whether it may be
/// deleted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteBranchFacts {
    pub exists: bool,
    pub is_default: bool,
    pub protected: bool,
    /// From the protection rules, when readable (admins only).
    pub allow_deletions: Option<bool>,
    /// `admin`, `maintain`, `write`, `triage`, `read`, or `none`.
    pub role: String,
}

// ------------------------------------------------------------ pure logic

/// The rule behind remote branch deletion: never the default branch;
/// admins/maintainers may delete unprotected branches (or protected ones
/// whose rules allow deletion); writers only unprotected branches.
pub fn decide_branch_deletion(facts: &RemoteBranchFacts) -> Decision {
    let deny = |reason: &str| Decision {
        allowed: false,
        reason: reason.into(),
    };
    if !facts.exists {
        return deny("This branch doesn't exist on GitHub.");
    }
    if facts.is_default {
        return deny("This is the repository's default branch.");
    }
    match facts.role.as_str() {
        "admin" | "maintain" => {
            if facts.protected && facts.allow_deletions != Some(true) {
                deny(
                    "This branch is protected. Change its protection rules on GitHub to delete it.",
                )
            } else {
                Decision {
                    allowed: true,
                    reason: format!("You can delete it ({} access).", facts.role),
                }
            }
        }
        "write" => {
            if facts.protected {
                deny("This branch is protected; only an admin can delete it.")
            } else {
                Decision {
                    allowed: true,
                    reason: "You can delete it (write access, branch not protected).".into(),
                }
            }
        }
        _ => deny("You need write access to this repository to delete branches."),
    }
}

/// Combines checks into one state, like GitHub's commit status rollup: any
/// failure wins, then anything unfinished, then success; all-neutral or
/// no checks at all is neutral.
pub fn rollup(items: &[CheckItem]) -> Rollup {
    let failed = |item: &CheckItem| {
        item.status == "completed"
            && matches!(
                item.conclusion.as_deref(),
                Some(
                    "failure"
                        | "timed_out"
                        | "cancelled"
                        | "action_required"
                        | "startup_failure"
                        | "error"
                )
            )
    };
    if items.iter().any(failed) {
        return Rollup::Failure;
    }
    if items.iter().any(|item| item.status != "completed") {
        return Rollup::Pending;
    }
    if items
        .iter()
        .any(|item| item.conclusion.as_deref() == Some("success"))
    {
        return Rollup::Success;
    }
    Rollup::Neutral
}

fn is_failed_conclusion(conclusion: Option<&str>) -> bool {
    matches!(
        conclusion,
        Some("failure" | "timed_out" | "startup_failure")
    )
}

/// Which workflow runs to re-run: the newest run of each workflow, if it
/// finished with a failure and hasn't used up `max_attempts`.
pub fn select_runs_to_retry(
    runs: &[WorkflowRun],
    max_attempts: u32,
) -> (Vec<WorkflowRun>, Vec<SkippedRun>) {
    let mut newest: BTreeMap<u64, &WorkflowRun> = BTreeMap::new();
    for run in runs {
        let entry = newest.entry(run.workflow_id).or_insert(run);
        if run.id > entry.id {
            *entry = run;
        }
    }
    let mut retry = Vec::new();
    let mut skipped = Vec::new();
    for run in newest.into_values() {
        if !is_failed_conclusion(run.conclusion.as_deref()) {
            continue;
        }
        if run.status != "completed" {
            continue;
        }
        if run.run_attempt >= max_attempts {
            skipped.push(SkippedRun {
                run_id: run.id,
                name: run.name.clone(),
                reason: format!(
                    "already ran {} time(s) (limit {max_attempts})",
                    run.run_attempt
                ),
            });
            continue;
        }
        retry.push(run.clone());
    }
    (retry, skipped)
}

/// Review decision from a PR's reviews: each reviewer's latest decisive
/// review counts; any "changes requested" wins over approvals.
pub fn review_decision(reviews: &[Value], has_requested_reviewers: bool) -> Option<String> {
    let mut latest: HashMap<String, (&str, &str)> = HashMap::new();
    for review in reviews {
        let Some(user) = review.pointer("/user/login").and_then(Value::as_str) else {
            continue;
        };
        let state = review.get("state").and_then(Value::as_str).unwrap_or("");
        let submitted = review
            .get("submitted_at")
            .and_then(Value::as_str)
            .unwrap_or("");
        if !matches!(state, "APPROVED" | "CHANGES_REQUESTED" | "DISMISSED") {
            continue;
        }
        let entry = latest.entry(user.to_owned()).or_insert((state, submitted));
        if submitted >= entry.1 {
            *entry = (state, submitted);
        }
    }
    let states: Vec<&str> = latest.values().map(|(state, _)| *state).collect();
    if states.contains(&"CHANGES_REQUESTED") {
        Some("changes_requested".into())
    } else if states.contains(&"APPROVED") && !has_requested_reviewers {
        Some("approved".into())
    } else if has_requested_reviewers || states.contains(&"DISMISSED") {
        Some("review_required".into())
    } else if states.contains(&"APPROVED") {
        Some("approved".into())
    } else {
        None
    }
}

/// Actions run and job ids from a check's details URL
/// (`…/actions/runs/<run>/job/<job>`).
pub fn actions_ids(url: &str) -> (Option<u64>, Option<u64>) {
    let mut run = None;
    let mut job = None;
    let parts: Vec<&str> = url.split(['/', '?', '#']).collect();
    for window in parts.windows(2) {
        match window[0] {
            "runs" => run = window[1].parse().ok(),
            "job" | "jobs" => job = window[1].parse().ok(),
            _ => {}
        }
    }
    (run, job)
}

/// The last `lines` lines of a job log, without the ISO timestamp GitHub
/// prefixes to every line.
pub fn tail_log(text: &str, lines: usize) -> (String, bool) {
    let all: Vec<&str> = text.lines().collect();
    let start = all.len().saturating_sub(lines);
    let tail = all[start..]
        .iter()
        .map(|line| {
            let line = line.trim_start_matches('\u{feff}');
            match line.split_once(' ') {
                Some((stamp, rest))
                    if stamp.len() >= 20
                        && stamp.ends_with('Z')
                        && stamp.as_bytes()[0].is_ascii_digit()
                        && stamp.contains('T') =>
                {
                    rest
                }
                _ => line,
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    (tail, start > 0)
}

fn encode_segment(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn str_of(value: &Value, pointer: &str) -> Option<String> {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn bool_of(value: &Value, pointer: &str) -> bool {
    value
        .pointer(pointer)
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn parse_rest_pr(value: &Value) -> PullRequest {
    let merged = bool_of(value, "/merged") || value.get("merged_at").is_some_and(|v| !v.is_null());
    let state = if merged {
        "merged".to_owned()
    } else {
        str_of(value, "/state").unwrap_or_else(|| "open".into())
    };
    PullRequest {
        number: value.get("number").and_then(Value::as_u64).unwrap_or(0),
        title: str_of(value, "/title").unwrap_or_default(),
        body: str_of(value, "/body"),
        url: str_of(value, "/html_url").unwrap_or_default(),
        state,
        draft: bool_of(value, "/draft"),
        author: str_of(value, "/user/login"),
        author_avatar: str_of(value, "/user/avatar_url"),
        head: str_of(value, "/head/ref").unwrap_or_default(),
        base: str_of(value, "/base/ref").unwrap_or_default(),
        head_sha: str_of(value, "/head/sha").unwrap_or_default(),
        head_repo: str_of(value, "/head/repo/full_name"),
        created_at: str_of(value, "/created_at").unwrap_or_default(),
        updated_at: str_of(value, "/updated_at").unwrap_or_default(),
        review_decision: None,
        mergeable: value.get("mergeable").and_then(Value::as_bool),
        mergeable_state: str_of(value, "/mergeable_state"),
        checks: None,
        requested_reviewers: value
            .get("requested_reviewers")
            .and_then(Value::as_array)
            .map(|users| {
                users
                    .iter()
                    .filter_map(|u| str_of(u, "/login"))
                    .chain(
                        value
                            .get("requested_teams")
                            .and_then(Value::as_array)
                            .into_iter()
                            .flatten()
                            .filter_map(|t| str_of(t, "/slug").map(|s| format!("team:{s}"))),
                    )
                    .collect()
            })
            .unwrap_or_default(),
        additions: value.get("additions").and_then(Value::as_u64),
        deletions: value.get("deletions").and_then(Value::as_u64),
        changed_files: value.get("changed_files").and_then(Value::as_u64),
        node_id: str_of(value, "/node_id"),
    }
}

fn graphql_rollup(state: Option<&str>) -> Option<Rollup> {
    Some(match state? {
        "SUCCESS" => Rollup::Success,
        "FAILURE" | "ERROR" => Rollup::Failure,
        "PENDING" | "EXPECTED" => Rollup::Pending,
        _ => Rollup::Neutral,
    })
}

fn parse_graphql_pr(node: &Value) -> PullRequest {
    let state = str_of(node, "/state")
        .unwrap_or_else(|| "OPEN".into())
        .to_ascii_lowercase();
    PullRequest {
        number: node.get("number").and_then(Value::as_u64).unwrap_or(0),
        title: str_of(node, "/title").unwrap_or_default(),
        body: str_of(node, "/body"),
        url: str_of(node, "/url").unwrap_or_default(),
        state,
        draft: bool_of(node, "/isDraft"),
        author: str_of(node, "/author/login"),
        author_avatar: str_of(node, "/author/avatarUrl"),
        head: str_of(node, "/headRefName").unwrap_or_default(),
        base: str_of(node, "/baseRefName").unwrap_or_default(),
        head_sha: str_of(node, "/headRefOid").unwrap_or_default(),
        head_repo: str_of(node, "/headRepository/nameWithOwner"),
        created_at: str_of(node, "/createdAt").unwrap_or_default(),
        updated_at: str_of(node, "/updatedAt").unwrap_or_default(),
        review_decision: str_of(node, "/reviewDecision").map(|d| d.to_ascii_lowercase()),
        mergeable: match node.get("mergeable").and_then(Value::as_str) {
            Some("MERGEABLE") => Some(true),
            Some("CONFLICTING") => Some(false),
            _ => None,
        },
        mergeable_state: str_of(node, "/mergeStateStatus").map(|s| s.to_ascii_lowercase()),
        checks: graphql_rollup(
            node.pointer("/commits/nodes/0/commit/statusCheckRollup/state")
                .and_then(Value::as_str),
        ),
        requested_reviewers: node
            .pointer("/reviewRequests/nodes")
            .and_then(Value::as_array)
            .map(|nodes| {
                nodes
                    .iter()
                    .filter_map(|n| {
                        str_of(n, "/requestedReviewer/login").or_else(|| {
                            str_of(n, "/requestedReviewer/slug").map(|s| format!("team:{s}"))
                        })
                    })
                    .collect()
            })
            .unwrap_or_default(),
        additions: node.get("additions").and_then(Value::as_u64),
        deletions: node.get("deletions").and_then(Value::as_u64),
        changed_files: node.get("changedFiles").and_then(Value::as_u64),
        node_id: str_of(node, "/id"),
    }
}

fn parse_check_run(run: &Value) -> CheckItem {
    let url = str_of(run, "/details_url").or_else(|| str_of(run, "/html_url"));
    let (run_id, job_id) = url.as_deref().map(actions_ids).unwrap_or((None, None));
    let app = str_of(run, "/app/name");
    let is_actions = str_of(run, "/app/slug").as_deref() == Some("github-actions");
    CheckItem {
        name: str_of(run, "/name").unwrap_or_default(),
        kind: "checkRun".into(),
        status: str_of(run, "/status").unwrap_or_else(|| "queued".into()),
        conclusion: str_of(run, "/conclusion"),
        url,
        started_at: str_of(run, "/started_at"),
        completed_at: str_of(run, "/completed_at"),
        app,
        description: str_of(run, "/output/title"),
        run_id: if is_actions { run_id } else { None },
        job_id: if is_actions {
            job_id.or_else(|| run.get("id").and_then(Value::as_u64))
        } else {
            None
        },
    }
}

fn parse_status(status: &Value) -> CheckItem {
    let state = str_of(status, "/state").unwrap_or_else(|| "pending".into());
    let pending = state == "pending";
    CheckItem {
        name: str_of(status, "/context").unwrap_or_default(),
        kind: "status".into(),
        status: if pending {
            "in_progress".into()
        } else {
            "completed".into()
        },
        conclusion: (!pending).then_some(state),
        url: str_of(status, "/target_url"),
        started_at: str_of(status, "/created_at"),
        completed_at: if pending {
            None
        } else {
            str_of(status, "/updated_at")
        },
        app: str_of(status, "/creator/login"),
        description: str_of(status, "/description"),
        run_id: None,
        job_id: None,
    }
}

/// Builds the summary from check runs and the combined-status response.
pub fn summarize_checks(git_ref: &str, check_runs: &Value, combined: &Value) -> ChecksSummary {
    let mut items: Vec<CheckItem> = check_runs
        .get("check_runs")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(parse_check_run)
        .collect();
    items.extend(
        combined
            .get("statuses")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(parse_status),
    );
    items.sort_by_key(|item| item.name.to_lowercase());
    let failed = items
        .iter()
        .filter(|i| rollup(std::slice::from_ref(i)) == Rollup::Failure)
        .count();
    let pending = items.iter().filter(|i| i.status != "completed").count();
    let passed = items
        .iter()
        .filter(|i| i.conclusion.as_deref() == Some("success"))
        .count();
    let sha = str_of(combined, "/sha").or_else(|| str_of(check_runs, "/check_runs/0/head_sha"));
    ChecksSummary {
        git_ref: git_ref.to_owned(),
        sha,
        rollup: rollup(&items),
        total: items.len(),
        passed,
        failed,
        pending,
        items,
    }
}

// ------------------------------------------------------------ client

const PR_FIELDS: &str = "id number title body url state isDraft createdAt updatedAt \
    author { login avatarUrl } headRefName baseRefName headRefOid \
    headRepository { nameWithOwner } reviewDecision mergeable mergeStateStatus \
    additions deletions changedFiles \
    reviewRequests(first: 10) { nodes { requestedReviewer { ... on User { login } ... on Team { slug } } } } \
    commits(last: 1) { nodes { commit { statusCheckRollup { state } } } }";

/// An authenticated (or anonymous) client for one repository.
pub struct GitHubClient {
    http: reqwest::Client,
    pub repo: GitHubRepo,
    token: Option<String>,
}

impl GitHubClient {
    pub fn new(repo: GitHubRepo, token: Option<String>) -> Result<Self, StudioError> {
        if rustls::crypto::CryptoProvider::get_default().is_none() {
            let _ = rustls::crypto::ring::default_provider().install_default();
        }
        let http = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| StudioError::Invalid(format!("HTTP client: {e}")))?;
        Ok(Self {
            http,
            repo,
            token: token.filter(|t| !t.is_empty()),
        })
    }

    pub fn has_token(&self) -> bool {
        self.token.is_some()
    }

    fn repo_path(&self, tail: &str) -> String {
        let base = format!(
            "/repos/{}/{}",
            encode_segment(&self.repo.owner),
            encode_segment(&self.repo.repo)
        );
        if tail.is_empty() {
            base
        } else {
            format!("{base}/{tail}")
        }
    }

    fn build(&self, method: Method, url: &str) -> reqwest::RequestBuilder {
        let mut request = self
            .http
            .request(method, url)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", API_VERSION);
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }
        request
    }

    /// Sends a request; returns the status and the parsed body (Null for
    /// empty bodies), mapping network failures to errors.
    async fn send_raw(
        &self,
        method: Method,
        path: &str,
        body: Option<&Value>,
    ) -> Result<(StatusCode, reqwest::header::HeaderMap, Value), StudioError> {
        let url = format!("{}{path}", self.repo.api_base);
        let mut request = self.build(method, &url);
        if let Some(body) = body {
            request = request.json(body);
        }
        let response = request
            .send()
            .await
            .map_err(|e| crate::unreachable("GitHub", &e))?;
        let status = response.status();
        let headers = response.headers().clone();
        let text = response.text().await.unwrap_or_default();
        let value = if text.trim().is_empty() {
            Value::Null
        } else {
            serde_json::from_str(&text).unwrap_or(Value::String(text))
        };
        Ok((status, headers, value))
    }

    fn error_for(
        &self,
        status: StatusCode,
        headers: &reqwest::header::HeaderMap,
        body: &Value,
        what: &str,
    ) -> StudioError {
        let message = body
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim_end_matches('.')
            .to_owned();
        let detail = body
            .pointer("/errors/0/message")
            .and_then(Value::as_str)
            .map(|m| format!(" ({m})"))
            .unwrap_or_default();
        let rate_limited = headers
            .get("x-ratelimit-remaining")
            .and_then(|v| v.to_str().ok())
            == Some("0");
        match status {
            StatusCode::UNAUTHORIZED => StudioError::Invalid(
                "GitHub rejected the access token (401). Sign in to GitHub again.".into(),
            ),
            StatusCode::FORBIDDEN | StatusCode::TOO_MANY_REQUESTS if rate_limited => {
                StudioError::Invalid(if self.token.is_some() {
                    "GitHub's API rate limit was reached. Try again in a few minutes.".into()
                } else {
                    "GitHub's anonymous rate limit was reached. Sign in to GitHub for a higher limit.".into()
                })
            }
            StatusCode::FORBIDDEN => StudioError::Invalid(format!(
                "GitHub refused to {what}: {message}{detail}. Check the token's permissions."
            )),
            StatusCode::NOT_FOUND => StudioError::NotFound {
                kind: "GitHub resource",
                id: what.to_owned(),
            },
            _ => StudioError::Invalid(format!(
                "GitHub couldn't {what} ({status}): {}{detail}",
                if message.is_empty() {
                    body.to_string().chars().take(300).collect()
                } else {
                    message
                }
            )),
        }
    }

    async fn send(
        &self,
        method: Method,
        path: &str,
        body: Option<&Value>,
        what: &str,
    ) -> Result<Value, StudioError> {
        let (status, headers, value) = self.send_raw(method, path, body).await?;
        if status.is_success() {
            Ok(value)
        } else {
            Err(self.error_for(status, &headers, &value, what))
        }
    }

    async fn get(&self, path: &str, what: &str) -> Result<Value, StudioError> {
        self.send(Method::GET, path, None, what).await
    }

    async fn graphql(&self, query: &str, variables: Value) -> Result<Value, StudioError> {
        if self.token.is_none() {
            return Err(StudioError::Invalid("sign in to GitHub first".into()));
        }
        let response = self
            .build(Method::POST, &self.repo.graphql_url())
            .json(&json!({ "query": query, "variables": variables }))
            .send()
            .await
            .map_err(|e| crate::unreachable("GitHub", &e))?;
        let status = response.status();
        let headers = response.headers().clone();
        let value: Value = response.json().await.unwrap_or(Value::Null);
        if !status.is_success() {
            return Err(self.error_for(status, &headers, &value, "run the query"));
        }
        if let Some(message) = value.pointer("/errors/0/message").and_then(Value::as_str) {
            return Err(StudioError::Invalid(format!("GitHub: {message}")));
        }
        Ok(value.get("data").cloned().unwrap_or(Value::Null))
    }

    // ---------------------------------------------------------- account

    /// The signed-in user.
    pub async fn viewer(&self) -> Result<Viewer, StudioError> {
        if self.token.is_none() {
            return Err(StudioError::Invalid("not signed in to GitHub".into()));
        }
        let user = self.get("/user", "read your account").await?;
        Ok(Viewer {
            login: str_of(&user, "/login").unwrap_or_default(),
            name: str_of(&user, "/name"),
            avatar_url: str_of(&user, "/avatar_url"),
        })
    }

    pub async fn repo_info(&self) -> Result<RepoInfo, StudioError> {
        let repo = self.get(&self.repo_path(""), "read the repository").await?;
        let permissions = repo.get("permissions").map(|p| RepoPermissions {
            admin: bool_of(p, "/admin"),
            maintain: bool_of(p, "/maintain"),
            push: bool_of(p, "/push"),
            triage: bool_of(p, "/triage"),
            pull: bool_of(p, "/pull"),
        });
        Ok(RepoInfo {
            full_name: str_of(&repo, "/full_name").unwrap_or_else(|| self.repo.full_name()),
            default_branch: str_of(&repo, "/default_branch").unwrap_or_else(|| "main".into()),
            private: bool_of(&repo, "/private"),
            visibility: str_of(&repo, "/visibility").unwrap_or_else(|| {
                if bool_of(&repo, "/private") {
                    "private".into()
                } else {
                    "public".into()
                }
            }),
            html_url: str_of(&repo, "/html_url").unwrap_or_else(|| self.repo.html_url.clone()),
            archived: bool_of(&repo, "/archived"),
            permissions,
            allow_merge_commit: repo
                .get("allow_merge_commit")
                .and_then(Value::as_bool)
                .unwrap_or(true),
            allow_squash_merge: repo
                .get("allow_squash_merge")
                .and_then(Value::as_bool)
                .unwrap_or(true),
            allow_rebase_merge: repo
                .get("allow_rebase_merge")
                .and_then(Value::as_bool)
                .unwrap_or(true),
            delete_branch_on_merge: bool_of(&repo, "/delete_branch_on_merge"),
        })
    }

    // ---------------------------------------------------------- pull requests

    /// Open pull requests, newest activity first, with review decision,
    /// mergeability, and checks (one GraphQL query when signed in; a plain
    /// REST listing without those details otherwise).
    pub async fn list_pull_requests(&self) -> Result<Vec<PullRequest>, StudioError> {
        if self.token.is_some() {
            let query = format!(
                "query($owner: String!, $name: String!) {{ repository(owner: $owner, name: $name) {{ \
                 pullRequests(states: OPEN, first: 50, orderBy: {{field: UPDATED_AT, direction: DESC}}) \
                 {{ nodes {{ {PR_FIELDS} }} }} }} }}"
            );
            match self
                .graphql(
                    &query,
                    json!({ "owner": self.repo.owner, "name": self.repo.repo }),
                )
                .await
            {
                Ok(data) => {
                    return Ok(data
                        .pointer("/repository/pullRequests/nodes")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .map(parse_graphql_pr)
                        .collect());
                }
                // Fine-grained tokens without GraphQL access etc.: fall back.
                Err(StudioError::Invalid(_)) => {}
                Err(error) => return Err(error),
            }
        }
        let list = self
            .get(
                &format!(
                    "{}?state=open&sort=updated&direction=desc&per_page=50",
                    self.repo_path("pulls")
                ),
                "list pull requests",
            )
            .await?;
        Ok(list
            .as_array()
            .into_iter()
            .flatten()
            .map(parse_rest_pr)
            .collect())
    }

    /// One pull request with its review decision and checks rollup.
    pub async fn pull_request(&self, number: u64) -> Result<PullRequest, StudioError> {
        let value = self
            .get(
                &self.repo_path(&format!("pulls/{number}")),
                "read the pull request",
            )
            .await?;
        let mut pr = parse_rest_pr(&value);
        if let Ok(reviews) = self
            .get(
                &self.repo_path(&format!("pulls/{number}/reviews?per_page=100")),
                "read reviews",
            )
            .await
        {
            let reviews = reviews.as_array().cloned().unwrap_or_default();
            pr.review_decision = review_decision(&reviews, !pr.requested_reviewers.is_empty());
        }
        if !pr.head_sha.is_empty()
            && let Ok(checks) = self.checks_for_ref(&pr.head_sha).await
        {
            pr.checks = Some(checks.rollup);
        }
        Ok(pr)
    }

    /// The pull request for a branch: the open one if any, otherwise the
    /// most recent closed/merged one.
    pub async fn pr_for_branch(&self, branch: &str) -> Result<Option<PullRequest>, StudioError> {
        let head = format!("{}:{branch}", self.repo.owner);
        let list = self
            .get(
                &format!(
                    "{}?state=all&per_page=10&head={}",
                    self.repo_path("pulls"),
                    encode_segment(&head).replace('/', "%2F")
                ),
                "find the branch's pull request",
            )
            .await?;
        let prs: Vec<PullRequest> = list
            .as_array()
            .into_iter()
            .flatten()
            .map(parse_rest_pr)
            .collect();
        let chosen = prs
            .iter()
            .find(|pr| pr.state == "open")
            .or_else(|| prs.first());
        match chosen {
            Some(pr) => Ok(Some(self.pull_request(pr.number).await?)),
            None => Ok(None),
        }
    }

    pub async fn create_pull_request(
        &self,
        new: &NewPullRequest,
    ) -> Result<PullRequest, StudioError> {
        if new.title.trim().is_empty() {
            return Err(StudioError::Invalid("give the pull request a title".into()));
        }
        if new.head.trim().is_empty() || new.base.trim().is_empty() {
            return Err(StudioError::Invalid(
                "choose the branches to compare".into(),
            ));
        }
        if new.head == new.base {
            return Err(StudioError::Invalid(
                "the head and base branches are the same".into(),
            ));
        }
        let body = json!({
            "title": new.title.trim(),
            "body": new.body.clone().unwrap_or_default(),
            "head": new.head,
            "base": new.base,
            "draft": new.draft,
        });
        let created = self
            .send(
                Method::POST,
                &self.repo_path("pulls"),
                Some(&body),
                "create the pull request",
            )
            .await?;
        Ok(parse_rest_pr(&created))
    }

    pub async fn update_pull_request(
        &self,
        number: u64,
        update: &PullRequestUpdate,
    ) -> Result<PullRequest, StudioError> {
        let mut fields = serde_json::Map::new();
        if let Some(title) = update.title.as_deref() {
            if title.trim().is_empty() {
                return Err(StudioError::Invalid("the title can't be empty".into()));
            }
            fields.insert("title".into(), json!(title.trim()));
        }
        if let Some(body) = &update.body {
            fields.insert("body".into(), json!(body));
        }
        let path = self.repo_path(&format!("pulls/{number}"));
        let mut current = if fields.is_empty() {
            parse_rest_pr(&self.get(&path, "read the pull request").await?)
        } else {
            parse_rest_pr(
                &self
                    .send(
                        Method::PATCH,
                        &path,
                        Some(&Value::Object(fields)),
                        "update the pull request",
                    )
                    .await?,
            )
        };
        if let Some(draft) = update.draft
            && draft != current.draft
        {
            let id = current
                .node_id
                .clone()
                .ok_or_else(|| StudioError::Invalid("GitHub didn't return the PR id".into()))?;
            let mutation = if draft {
                "mutation($id: ID!) { convertPullRequestToDraft(input: {pullRequestId: $id}) { clientMutationId } }"
            } else {
                "mutation($id: ID!) { markPullRequestReadyForReview(input: {pullRequestId: $id}) { clientMutationId } }"
            };
            self.graphql(mutation, json!({ "id": id })).await?;
            current.draft = draft;
        }
        self.pull_request(number).await.or(Ok(current))
    }

    pub async fn request_reviewers(
        &self,
        number: u64,
        reviewers: &[String],
    ) -> Result<PullRequest, StudioError> {
        let (teams, users): (Vec<&String>, Vec<&String>) =
            reviewers.iter().partition(|r| r.starts_with("team:"));
        let teams: Vec<&str> = teams
            .iter()
            .map(|t| t.trim_start_matches("team:"))
            .collect();
        let body = json!({ "reviewers": users, "team_reviewers": teams });
        let value = self
            .send(
                Method::POST,
                &self.repo_path(&format!("pulls/{number}/requested_reviewers")),
                Some(&body),
                "request reviewers",
            )
            .await?;
        Ok(parse_rest_pr(&value))
    }

    /// Adds a conversation comment; returns the comment's URL.
    pub async fn add_comment(&self, number: u64, body: &str) -> Result<String, StudioError> {
        if body.trim().is_empty() {
            return Err(StudioError::Invalid("the comment is empty".into()));
        }
        let value = self
            .send(
                Method::POST,
                &self.repo_path(&format!("issues/{number}/comments")),
                Some(&json!({ "body": body })),
                "add the comment",
            )
            .await?;
        Ok(str_of(&value, "/html_url").unwrap_or_default())
    }

    /// Merges a PR after checking it can be merged: open, not a draft, no
    /// conflicts, and mergeability already computed. Pins the head SHA so a
    /// push that lands meanwhile isn't merged unseen.
    pub async fn merge_pull_request(
        &self,
        number: u64,
        method: MergeMethod,
        commit_title: Option<&str>,
        commit_message: Option<&str>,
    ) -> Result<MergeResult, StudioError> {
        let pr = parse_rest_pr(
            &self
                .get(
                    &self.repo_path(&format!("pulls/{number}")),
                    "read the pull request",
                )
                .await?,
        );
        if pr.state == "merged" {
            return Err(StudioError::Invalid(
                "this pull request is already merged".into(),
            ));
        }
        if pr.state != "open" {
            return Err(StudioError::Invalid("this pull request is closed".into()));
        }
        if pr.draft {
            return Err(StudioError::Invalid(
                "this is a draft; mark it ready for review first".into(),
            ));
        }
        match pr.mergeable {
            Some(false) => {
                return Err(StudioError::Invalid(
                    "this pull request has conflicts with its base branch; resolve them first"
                        .into(),
                ));
            }
            None => {
                return Err(StudioError::Invalid(
                    "GitHub is still checking whether this can merge; try again in a moment".into(),
                ));
            }
            Some(true) => {}
        }
        let mut body = json!({ "merge_method": method.as_str(), "sha": pr.head_sha });
        if let Some(title) = commit_title.filter(|t| !t.trim().is_empty()) {
            body["commit_title"] = json!(title);
        }
        if let Some(message) = commit_message.filter(|m| !m.trim().is_empty()) {
            body["commit_message"] = json!(message);
        }
        let value = self
            .send(
                Method::PUT,
                &self.repo_path(&format!("pulls/{number}/merge")),
                Some(&body),
                "merge the pull request",
            )
            .await?;
        Ok(MergeResult {
            merged: bool_of(&value, "/merged"),
            sha: str_of(&value, "/sha"),
            message: str_of(&value, "/message").unwrap_or_default(),
        })
    }

    pub async fn close_pull_request(&self, number: u64) -> Result<PullRequest, StudioError> {
        let value = self
            .send(
                Method::PATCH,
                &self.repo_path(&format!("pulls/{number}")),
                Some(&json!({ "state": "closed" })),
                "close the pull request",
            )
            .await?;
        Ok(parse_rest_pr(&value))
    }

    // ---------------------------------------------------------- checks

    /// Check runs and legacy commit statuses for a branch, tag, or SHA.
    pub async fn checks_for_ref(&self, git_ref: &str) -> Result<ChecksSummary, StudioError> {
        let encoded = encode_segment(git_ref);
        let runs = self
            .get(
                &self.repo_path(&format!("commits/{encoded}/check-runs?per_page=100")),
                "read check runs",
            )
            .await?;
        let combined = match self
            .get(
                &self.repo_path(&format!("commits/{encoded}/status?per_page=100")),
                "read commit statuses",
            )
            .await
        {
            Ok(value) => value,
            Err(StudioError::NotFound { .. }) => Value::Null,
            Err(error) => return Err(error),
        };
        Ok(summarize_checks(git_ref, &runs, &combined))
    }

    /// Re-runs only the failed jobs of an Actions workflow run.
    pub async fn rerun_failed_jobs(&self, run_id: u64) -> Result<(), StudioError> {
        self.send(
            Method::POST,
            &self.repo_path(&format!("actions/runs/{run_id}/rerun-failed-jobs")),
            None,
            "re-run the failed jobs",
        )
        .await
        .map(|_| ())
    }

    async fn resolve_sha(&self, git_ref: &str) -> Result<String, StudioError> {
        if git_ref.len() == 40 && git_ref.chars().all(|c| c.is_ascii_hexdigit()) {
            return Ok(git_ref.to_owned());
        }
        let commit = self
            .get(
                &self.repo_path(&format!("commits/{}", encode_segment(git_ref))),
                "resolve the commit",
            )
            .await?;
        str_of(&commit, "/sha")
            .ok_or_else(|| StudioError::Invalid(format!("couldn't resolve {git_ref}")))
    }

    /// Workflow runs for a commit.
    pub async fn workflow_runs(&self, git_ref: &str) -> Result<Vec<WorkflowRun>, StudioError> {
        let sha = self.resolve_sha(git_ref).await?;
        let value = self
            .get(
                &self.repo_path(&format!("actions/runs?head_sha={sha}&per_page=100")),
                "list workflow runs",
            )
            .await?;
        Ok(value
            .get("workflow_runs")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|run| WorkflowRun {
                id: run.get("id").and_then(Value::as_u64).unwrap_or(0),
                name: str_of(run, "/name").unwrap_or_default(),
                workflow_id: run.get("workflow_id").and_then(Value::as_u64).unwrap_or(0),
                status: str_of(run, "/status").unwrap_or_default(),
                conclusion: str_of(run, "/conclusion"),
                run_attempt: run.get("run_attempt").and_then(Value::as_u64).unwrap_or(1) as u32,
                html_url: str_of(run, "/html_url").unwrap_or_default(),
            })
            .collect())
    }

    /// Re-runs the failed jobs of each failed workflow run for `git_ref`,
    /// once per call, as long as the run hasn't reached `max_attempts`.
    /// Handy for flaky CI (runners dropping, network blips in builds).
    pub async fn auto_retry_failed_checks(
        &self,
        git_ref: &str,
        max_attempts: u32,
    ) -> Result<RetryReport, StudioError> {
        let runs = self.workflow_runs(git_ref).await?;
        let (to_retry, skipped) = select_runs_to_retry(&runs, max_attempts.max(1));
        let mut report = RetryReport {
            retried: Vec::new(),
            skipped,
        };
        for run in to_retry {
            match self.rerun_failed_jobs(run.id).await {
                Ok(()) => report.retried.push(RetriedRun {
                    run_id: run.id,
                    name: run.name,
                    attempt: run.run_attempt + 1,
                    url: run.html_url,
                }),
                Err(error) => report.skipped.push(SkippedRun {
                    run_id: run.id,
                    name: run.name,
                    reason: error.to_string(),
                }),
            }
        }
        Ok(report)
    }

    /// The last `lines` lines of one job's log.
    pub async fn job_log_tail(
        &self,
        job_id: u64,
        lines: usize,
    ) -> Result<(String, bool), StudioError> {
        let url = format!(
            "{}{}",
            self.repo.api_base,
            self.repo_path(&format!("actions/jobs/{job_id}/logs"))
        );
        // Redirects to a signed blob URL; reqwest drops the auth header on
        // the cross-host hop.
        let response = self
            .build(Method::GET, &url)
            .send()
            .await
            .map_err(|e| crate::unreachable("GitHub", &e))?;
        let status = response.status();
        let headers = response.headers().clone();
        let text = response.text().await.unwrap_or_default();
        if !status.is_success() {
            let body = serde_json::from_str(&text).unwrap_or(Value::Null);
            return Err(self.error_for(status, &headers, &body, "read the job log"));
        }
        Ok(tail_log(&text, lines))
    }

    /// Logs (tails) of the failed jobs in a workflow run, up to five jobs.
    pub async fn failed_job_logs(&self, run_id: u64) -> Result<Vec<JobLog>, StudioError> {
        let value = self
            .get(
                &self.repo_path(&format!(
                    "actions/runs/{run_id}/jobs?filter=latest&per_page=100"
                )),
                "list the run's jobs",
            )
            .await?;
        let mut logs = Vec::new();
        for job in value
            .get("jobs")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|job| is_failed_conclusion(job.get("conclusion").and_then(Value::as_str)))
            .take(5)
        {
            let job_id = job.get("id").and_then(Value::as_u64).unwrap_or(0);
            let failed_step = job
                .get("steps")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .find(|step| is_failed_conclusion(step.get("conclusion").and_then(Value::as_str)))
                .and_then(|step| str_of(step, "/name"));
            let (log_tail, truncated) = match self.job_log_tail(job_id, LOG_TAIL_LINES).await {
                Ok(result) => result,
                Err(error) => (format!("(log unavailable: {error})"), false),
            };
            logs.push(JobLog {
                job_id,
                name: str_of(job, "/name").unwrap_or_default(),
                url: str_of(job, "/html_url"),
                conclusion: str_of(job, "/conclusion"),
                failed_step,
                log_tail,
                truncated,
            });
        }
        Ok(logs)
    }

    // ---------------------------------------------------------- permissions

    /// The viewer's role on this repository: from the collaborator
    /// permission endpoint, falling back to the repo's `permissions`.
    pub async fn viewer_role(&self, login: &str, info: &RepoInfo) -> String {
        let path = self.repo_path(&format!(
            "collaborators/{}/permission",
            encode_segment(login)
        ));
        if let Ok(value) = self.get(&path, "read your permission").await
            && let Some(role) =
                str_of(&value, "/role_name").or_else(|| str_of(&value, "/permission"))
        {
            return role;
        }
        info.permissions
            .map(|p| p.role().to_owned())
            .unwrap_or_else(|| "none".into())
    }

    /// Whether the signed-in user may delete `branch` on GitHub.
    pub async fn can_delete_remote_branch(&self, branch: &str) -> Result<Decision, StudioError> {
        if self.token.is_none() {
            return Ok(Decision {
                allowed: false,
                reason: "Sign in to GitHub to delete branches on the remote.".into(),
            });
        }
        let viewer = self.viewer().await?;
        let info = self.repo_info().await?;
        if info.archived {
            return Ok(Decision {
                allowed: false,
                reason: "This repository is archived (read-only).".into(),
            });
        }
        let encoded = encode_segment(branch);
        let (exists, protected) = match self
            .get(
                &self.repo_path(&format!("branches/{encoded}")),
                "read the branch",
            )
            .await
        {
            Ok(value) => (true, bool_of(&value, "/protected")),
            Err(StudioError::NotFound { .. }) => (false, false),
            Err(error) => return Err(error),
        };
        let allow_deletions = if protected {
            match self
                .send_raw(
                    Method::GET,
                    &self.repo_path(&format!("branches/{encoded}/protection")),
                    None,
                )
                .await
            {
                Ok((status, _, value)) if status.is_success() => {
                    Some(bool_of(&value, "/allow_deletions/enabled"))
                }
                // 404 here means "protected by a ruleset, not classic rules".
                _ => None,
            }
        } else {
            None
        };
        let role = self.viewer_role(&viewer.login, &info).await;
        Ok(decide_branch_deletion(&RemoteBranchFacts {
            exists,
            is_default: branch == info.default_branch,
            protected,
            allow_deletions,
            role,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_remote_urls() {
        let expect = |url: &str, host: &str| {
            assert_eq!(
                parse_remote_url(url),
                Some((host.into(), "octo".into(), "game".into())),
                "{url}"
            );
        };
        expect("https://github.com/octo/game.git", "github.com");
        expect("https://github.com/octo/game", "github.com");
        expect("https://user:tok@github.com/octo/game.git/", "github.com");
        expect("git@github.com:octo/game.git", "github.com");
        expect("ssh://git@github.com/octo/game.git", "github.com");
        expect("ssh://git@ssh.github.com:443/octo/game.git", "github.com");
        expect("git@github.com-work:octo/game.git", "github.com");
        expect(
            "https://git.studio.example:8443/octo/game.git",
            "git.studio.example",
        );
        expect("git@ghe.studio.example:octo/game", "ghe.studio.example");
        assert_eq!(parse_remote_url("/local/path/repo"), None);
        assert_eq!(parse_remote_url("https://github.com/octo"), None);
        assert_eq!(parse_remote_url("file:///tmp/x/y"), None);
    }

    #[test]
    fn api_bases() {
        assert_eq!(api_base_for_host("github.com"), "https://api.github.com");
        assert_eq!(
            api_base_for_host("acme.ghe.com"),
            "https://api.acme.ghe.com"
        );
        assert_eq!(
            api_base_for_host("github.acme.com"),
            "https://github.acme.com/api/v3"
        );
        let repo = GitHubRepo::new("github.acme.com", "o", "r");
        assert_eq!(repo.graphql_url(), "https://github.acme.com/api/graphql");
        assert_eq!(
            GitHubRepo::new("github.com", "o", "r").graphql_url(),
            "https://api.github.com/graphql"
        );
        assert!(GitHubRepo::from_remote_url("git@github.com:o/r.git").is_some());
    }

    fn check(status: &str, conclusion: Option<&str>) -> CheckItem {
        CheckItem {
            name: "c".into(),
            kind: "checkRun".into(),
            status: status.into(),
            conclusion: conclusion.map(str::to_owned),
            url: None,
            started_at: None,
            completed_at: None,
            app: None,
            description: None,
            run_id: None,
            job_id: None,
        }
    }

    #[test]
    fn rollup_logic() {
        assert_eq!(rollup(&[]), Rollup::Neutral);
        assert_eq!(
            rollup(&[
                check("completed", Some("success")),
                check("completed", Some("skipped"))
            ]),
            Rollup::Success
        );
        assert_eq!(
            rollup(&[
                check("completed", Some("success")),
                check("in_progress", None)
            ]),
            Rollup::Pending
        );
        assert_eq!(
            rollup(&[
                check("in_progress", None),
                check("completed", Some("failure"))
            ]),
            Rollup::Failure
        );
        assert_eq!(
            rollup(&[check("completed", Some("timed_out"))]),
            Rollup::Failure
        );
        assert_eq!(
            rollup(&[
                check("completed", Some("neutral")),
                check("completed", Some("skipped"))
            ]),
            Rollup::Neutral
        );
    }

    #[test]
    fn summarizes_check_runs_and_statuses() {
        let runs = json!({ "check_runs": [
            { "id": 11, "name": "build", "status": "completed", "conclusion": "failure",
              "head_sha": "abc", "details_url": "https://github.com/o/r/actions/runs/99/job/11",
              "app": { "name": "GitHub Actions", "slug": "github-actions" } },
            { "id": 12, "name": "lint", "status": "completed", "conclusion": "success",
              "app": { "name": "Other", "slug": "other" } }
        ]});
        let combined = json!({ "sha": "abc", "statuses": [
            { "context": "ci/legacy", "state": "pending", "target_url": "https://ci" }
        ]});
        let summary = summarize_checks("main", &runs, &combined);
        assert_eq!(summary.total, 3);
        assert_eq!(summary.failed, 1);
        assert_eq!(summary.pending, 1);
        assert_eq!(summary.passed, 1);
        assert_eq!(summary.rollup, Rollup::Failure);
        assert_eq!(summary.sha.as_deref(), Some("abc"));
        let build = summary.items.iter().find(|i| i.name == "build").unwrap();
        assert_eq!((build.run_id, build.job_id), (Some(99), Some(11)));
        let legacy = summary.items.iter().find(|i| i.kind == "status").unwrap();
        assert_eq!(legacy.status, "in_progress");
    }

    #[test]
    fn deletion_decisions() {
        let facts = |role: &str, protected: bool, allow: Option<bool>| RemoteBranchFacts {
            exists: true,
            is_default: false,
            protected,
            allow_deletions: allow,
            role: role.into(),
        };
        assert!(decide_branch_deletion(&facts("admin", false, None)).allowed);
        assert!(decide_branch_deletion(&facts("maintain", false, None)).allowed);
        assert!(decide_branch_deletion(&facts("write", false, None)).allowed);
        assert!(!decide_branch_deletion(&facts("write", true, Some(true))).allowed);
        assert!(!decide_branch_deletion(&facts("admin", true, None)).allowed);
        assert!(decide_branch_deletion(&facts("admin", true, Some(true))).allowed);
        assert!(!decide_branch_deletion(&facts("triage", false, None)).allowed);
        assert!(!decide_branch_deletion(&facts("read", false, None)).allowed);
        let mut default = facts("admin", false, None);
        default.is_default = true;
        assert!(!decide_branch_deletion(&default).allowed);
        let mut missing = facts("admin", false, None);
        missing.exists = false;
        assert!(!decide_branch_deletion(&missing).allowed);
    }

    #[test]
    fn selects_failed_runs_to_retry() {
        let run = |id, workflow, conclusion: &str, attempt| WorkflowRun {
            id,
            name: format!("wf{workflow}"),
            workflow_id: workflow,
            status: "completed".into(),
            conclusion: Some(conclusion.into()),
            run_attempt: attempt,
            html_url: String::new(),
        };
        let runs = vec![
            run(1, 1, "failure", 1),
            run(2, 1, "success", 1), // newer run of wf1 passed: nothing to do
            run(3, 2, "failure", 1),
            run(4, 3, "timed_out", 3),
            run(5, 4, "cancelled", 1),
        ];
        let (retry, skipped) = select_runs_to_retry(&runs, 3);
        assert_eq!(retry.iter().map(|r| r.id).collect::<Vec<_>>(), vec![3]);
        assert_eq!(skipped.len(), 1);
        assert_eq!(skipped[0].run_id, 4);
    }

    #[test]
    fn review_decisions() {
        let review = |user: &str, state: &str, at: &str| json!({ "user": { "login": user }, "state": state, "submitted_at": at });
        assert_eq!(review_decision(&[], false), None);
        assert_eq!(
            review_decision(&[], true).as_deref(),
            Some("review_required")
        );
        let approved = [review("a", "APPROVED", "2024-01-02")];
        assert_eq!(
            review_decision(&approved, false).as_deref(),
            Some("approved")
        );
        let changed = [
            review("a", "CHANGES_REQUESTED", "2024-01-01"),
            review("a", "APPROVED", "2024-01-02"),
            review("b", "COMMENTED", "2024-01-03"),
        ];
        assert_eq!(
            review_decision(&changed, false).as_deref(),
            Some("approved")
        );
        let blocking = [
            review("a", "APPROVED", "2024-01-02"),
            review("b", "CHANGES_REQUESTED", "2024-01-03"),
        ];
        assert_eq!(
            review_decision(&blocking, false).as_deref(),
            Some("changes_requested")
        );
    }

    #[test]
    fn log_tail_strips_timestamps() {
        let log =
            "2024-05-01T10:00:00.1234567Z line one\n2024-05-01T10:00:01.0000000Z line two\nplain";
        let (tail, truncated) = tail_log(log, 2);
        assert_eq!(tail, "line two\nplain");
        assert!(truncated);
        assert_eq!(tail_log(log, 10).0, "line one\nline two\nplain");
    }

    #[test]
    fn actions_ids_from_urls() {
        assert_eq!(
            actions_ids("https://github.com/o/r/actions/runs/123/job/456"),
            (Some(123), Some(456))
        );
        assert_eq!(actions_ids("https://ci.example/build/1"), (None, None));
    }

    #[test]
    fn encodes_path_segments() {
        assert_eq!(encode_segment("feature/new hud"), "feature/new%20hud");
        assert_eq!(encode_segment("v1.0#x"), "v1.0%23x");
    }

    /// Live check against GitHub's public API (skipped without network).
    #[tokio::test]
    #[ignore = "network"]
    async fn reads_public_repo_and_checks() {
        let client =
            GitHubClient::new(GitHubRepo::new("github.com", "tauri-apps", "tauri"), None).unwrap();
        let info = client.repo_info().await.unwrap();
        assert_eq!(info.full_name, "tauri-apps/tauri");
        let summary = client.checks_for_ref(&info.default_branch).await.unwrap();
        assert!(summary.total > 0);
    }
}
