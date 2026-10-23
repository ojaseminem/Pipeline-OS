//! Desktop commands for advanced source control: pre-commit checks and
//! auto-fixes, safe branch/tag/history operations, external merge and diff
//! tools, and GitHub (pull requests, checks, Actions re-runs).
//!
//! Mutations take `confirmed` like the other `git_*` commands; GitHub calls
//! resolve the repository from the project's `origin` remote and the token
//! from the keychain, environment, or the GitHub CLI.

use std::path::Path;

use serde::Serialize;
use tauri::State;
use vantadeck_studio::github::{
    self, ChecksSummary, Decision, GitHubClient, GitHubRepo, JobLog, MergeMethod, MergeResult,
    NewPullRequest, PullRequest, PullRequestUpdate, RepoInfo, RetryReport, TokenSource, Viewer,
};
use vantadeck_vcs::{
    BlameLine, CheckFinding, FixKind, GitCommit, GitProvider, LaunchedTool, MergeOutcome,
    MergeTool, PendingOperation, StashDetails, TagInfo, VcsOperationResult,
};

use crate::{DesktopState, require_confirmation};

fn git<'a>(state: &'a State<'_, DesktopState>) -> &'a GitProvider {
    state.service.git()
}

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|e| e.to_string())
}

/// The GitHub repo behind `origin`, if it is one.
async fn github_repo(
    state: &State<'_, DesktopState>,
    root: &Path,
) -> Result<Option<GitHubRepo>, String> {
    let Some(url) = git(state).remote_url(root, None).await else {
        return Ok(None);
    };
    blocking(move || GitHubRepo::from_remote_url(&url)).await
}

/// A client for the project's GitHub repo, signed in when a token exists.
async fn github_client(
    state: &State<'_, DesktopState>,
    root: &Path,
) -> Result<GitHubClient, String> {
    let repo = github_repo(state, root)
        .await?
        .ok_or_else(|| "This project's origin remote isn't a GitHub repository.".to_owned())?;
    let host = repo.host.clone();
    let token = blocking(move || github::resolve_token(&host)).await?;
    GitHubClient::new(repo, token.map(|(token, _)| token)).map_err(|e| e.to_string())
}

async fn current_branch(state: &State<'_, DesktopState>, root: &Path) -> Result<String, String> {
    git(state)
        .current_branch(root)
        .await
        .filter(|branch| branch != "HEAD")
        .ok_or_else(|| "No branch is checked out (detached HEAD).".to_owned())
}

// ------------------------------------------------------------ pre-commit

/// Pre-commit checks over `paths` (or every changed file).
#[tauri::command(rename_all = "camelCase")]
pub async fn scm_checks(
    root: String,
    paths: Option<Vec<String>>,
    state: State<'_, DesktopState>,
) -> Result<Vec<CheckFinding>, String> {
    git(&state)
        .pre_commit_checks(Path::new(&root), paths.as_deref())
        .await
        .map_err(|e| e.to_string())
}

/// Applies a finding's automatic fix (the `fix` object from `scm_checks`).
#[tauri::command(rename_all = "camelCase")]
pub async fn scm_fix(
    root: String,
    fix: FixKind,
    confirmed: bool,
    state: State<'_, DesktopState>,
) -> Result<VcsOperationResult, String> {
    require_confirmation(confirmed)?;
    git(&state)
        .apply_fix(Path::new(&root), &fix)
        .await
        .map_err(|e| e.to_string())
}

// ------------------------------------------------------------ branches & history

async fn remote_delete_decision(
    state: &State<'_, DesktopState>,
    root: &Path,
    branch: &str,
) -> Result<Decision, String> {
    match github_repo(state, root).await? {
        None => Ok(Decision {
            allowed: false,
            reason: "Remote branches can only be deleted here for GitHub repositories.".into(),
        }),
        Some(_) => github_client(state, root)
            .await?
            .can_delete_remote_branch(branch)
            .await
            .map_err(|e| e.to_string()),
    }
}

/// Whether the signed-in GitHub user may delete `branch` on the remote.
#[tauri::command(rename_all = "camelCase")]
pub async fn scm_can_delete_remote_branch(
    root: String,
    branch: String,
    state: State<'_, DesktopState>,
) -> Result<Decision, String> {
    remote_delete_decision(&state, Path::new(&root), &branch).await
}

/// Deletes a branch locally and (when `remote`) on origin, with the safety
/// rules in `GitProvider::delete_branch` and the GitHub permission check
/// for the remote side.
#[tauri::command(rename_all = "camelCase")]
pub async fn scm_delete_branch(
    root: String,
    name: String,
    remote: bool,
    force: bool,
    confirmed: bool,
    state: State<'_, DesktopState>,
) -> Result<VcsOperationResult, String> {
    require_confirmation(confirmed)?;
    let path = Path::new(&root);
    let remote_allowed = if remote {
        let decision = remote_delete_decision(&state, path, &name).await?;
        if !decision.allowed {
            return Err(decision.reason);
        }
        true
    } else {
        false
    };
    git(&state)
        .delete_branch(path, &name, remote, force, remote_allowed)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
pub async fn scm_rename_branch(
    root: String,
    old_name: String,
    new_name: String,
    confirmed: bool,
    state: State<'_, DesktopState>,
) -> Result<VcsOperationResult, String> {
    require_confirmation(confirmed)?;
    git(&state)
        .rename_branch(Path::new(&root), &old_name, &new_name)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
pub async fn scm_tags(
    root: String,
    state: State<'_, DesktopState>,
) -> Result<Vec<TagInfo>, String> {
    git(&state)
        .list_tags(Path::new(&root))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
pub async fn scm_create_tag(
    root: String,
    name: String,
    target: Option<String>,
    message: Option<String>,
    confirmed: bool,
    state: State<'_, DesktopState>,
) -> Result<VcsOperationResult, String> {
    require_confirmation(confirmed)?;
    git(&state)
        .create_tag(
            Path::new(&root),
            &name,
            target.as_deref(),
            message.as_deref(),
        )
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
pub async fn scm_push_tag(
    root: String,
    name: String,
    confirmed: bool,
    state: State<'_, DesktopState>,
) -> Result<VcsOperationResult, String> {
    require_confirmation(confirmed)?;
    git(&state)
        .push_tag(Path::new(&root), &name)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
pub async fn scm_cherry_pick(
    root: String,
    hash: String,
    confirmed: bool,
    state: State<'_, DesktopState>,
) -> Result<MergeOutcome, String> {
    require_confirmation(confirmed)?;
    git(&state)
        .cherry_pick(Path::new(&root), &hash)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
pub async fn scm_revert(
    root: String,
    hash: String,
    confirmed: bool,
    state: State<'_, DesktopState>,
) -> Result<MergeOutcome, String> {
    require_confirmation(confirmed)?;
    git(&state)
        .revert_commit(Path::new(&root), &hash)
        .await
        .map_err(|e| e.to_string())
}

/// Which operation (merge/cherry-pick/revert/rebase) is paused, if any.
#[tauri::command(rename_all = "camelCase")]
pub async fn scm_pending_operation(
    root: String,
    state: State<'_, DesktopState>,
) -> Result<Option<PendingOperation>, String> {
    Ok(git(&state).pending_operation(Path::new(&root)).await)
}

/// Aborts the paused merge/cherry-pick/revert/rebase.
#[tauri::command(rename_all = "camelCase")]
pub async fn scm_abort_operation(
    root: String,
    confirmed: bool,
    state: State<'_, DesktopState>,
) -> Result<VcsOperationResult, String> {
    require_confirmation(confirmed)?;
    git(&state)
        .abort_operation(Path::new(&root))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
pub async fn scm_amend_message(
    root: String,
    message: String,
    confirmed: bool,
    state: State<'_, DesktopState>,
) -> Result<VcsOperationResult, String> {
    require_confirmation(confirmed)?;
    git(&state)
        .amend_last_commit_message(Path::new(&root), &message)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
pub async fn scm_undo_last_commit(
    root: String,
    confirmed: bool,
    state: State<'_, DesktopState>,
) -> Result<VcsOperationResult, String> {
    require_confirmation(confirmed)?;
    git(&state)
        .undo_last_commit(Path::new(&root))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
pub async fn scm_blame(
    root: String,
    path: String,
    state: State<'_, DesktopState>,
) -> Result<Vec<BlameLine>, String> {
    git(&state)
        .blame(Path::new(&root), &path)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
pub async fn scm_file_history(
    root: String,
    path: String,
    limit: Option<u32>,
    state: State<'_, DesktopState>,
) -> Result<Vec<GitCommit>, String> {
    git(&state)
        .file_history(Path::new(&root), &path, limit.unwrap_or(100))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
pub async fn scm_stash_show(
    root: String,
    stash_ref: Option<String>,
    state: State<'_, DesktopState>,
) -> Result<StashDetails, String> {
    git(&state)
        .stash_show(Path::new(&root), stash_ref.as_deref())
        .await
        .map_err(|e| e.to_string())
}

// ------------------------------------------------------------ merge tools

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeToolList {
    tools: Vec<MergeTool>,
    recommended: Option<String>,
}

/// Installed merge/diff tools usable for `path` (all tools when omitted),
/// plus the recommended one (UnityYAMLMerge for scenes/prefabs).
#[tauri::command(rename_all = "camelCase")]
pub async fn scm_merge_tools(path: Option<String>) -> Result<MergeToolList, String> {
    let tools = blocking(vantadeck_vcs::detect_merge_tools).await?;
    let (tools, recommended) = vantadeck_vcs::merge_tools_for_path(&tools, path.as_deref());
    Ok(MergeToolList { tools, recommended })
}

/// Opens a conflicted file in a 3-way merge tool and returns immediately.
#[tauri::command(rename_all = "camelCase")]
pub async fn scm_launch_merge_tool(
    root: String,
    path: String,
    tool_id: String,
    state: State<'_, DesktopState>,
) -> Result<LaunchedTool, String> {
    git(&state)
        .launch_merge_tool(Path::new(&root), &path, &tool_id)
        .await
        .map_err(|e| e.to_string())
}

/// Opens the working copy vs HEAD in an external diff tool.
#[tauri::command(rename_all = "camelCase")]
pub async fn scm_open_diff_tool(
    root: String,
    path: String,
    tool_id: String,
    state: State<'_, DesktopState>,
) -> Result<LaunchedTool, String> {
    git(&state)
        .open_diff_tool(Path::new(&root), &path, &tool_id)
        .await
        .map_err(|e| e.to_string())
}

/// Stages a file merged in an external tool (refuses while conflict markers
/// remain).
#[tauri::command(rename_all = "camelCase")]
pub async fn scm_mark_resolved(
    root: String,
    path: String,
    state: State<'_, DesktopState>,
) -> Result<VcsOperationResult, String> {
    git(&state)
        .mark_resolved(Path::new(&root), &path)
        .await
        .map_err(|e| e.to_string())
}

// ------------------------------------------------------------ GitHub

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GithubStatus {
    remote_url: Option<String>,
    /// `None` when origin isn't a GitHub repository.
    repo: Option<GitHubRepo>,
    token_source: TokenSource,
    viewer: Option<Viewer>,
    repo_info: Option<RepoInfo>,
    /// Why viewer/repo info couldn't be loaded (bad token, offline…).
    error: Option<String>,
}

/// Which GitHub repo this project uses, who's signed in, and what they can do.
#[tauri::command(rename_all = "camelCase")]
pub async fn scm_github_status(
    root: String,
    state: State<'_, DesktopState>,
) -> Result<GithubStatus, String> {
    let root = Path::new(&root);
    let remote_url = git(&state).remote_url(root, None).await;
    let mut status = GithubStatus {
        remote_url: remote_url.clone(),
        repo: None,
        token_source: TokenSource::None,
        viewer: None,
        repo_info: None,
        error: None,
    };
    let Some(url) = remote_url else {
        return Ok(status);
    };
    let Some(repo) = blocking(move || GitHubRepo::from_remote_url(&url)).await? else {
        return Ok(status);
    };
    status.repo = Some(repo.clone());
    let host = repo.host.clone();
    let token = blocking(move || github::resolve_token(&host)).await?;
    status.token_source = token.as_ref().map_or(TokenSource::None, |(_, s)| *s);
    let client =
        GitHubClient::new(repo, token.map(|(token, _)| token)).map_err(|e| e.to_string())?;
    if client.has_token() {
        match client.viewer().await {
            Ok(viewer) => status.viewer = Some(viewer),
            Err(error) => status.error = Some(error.to_string()),
        }
    }
    match client.repo_info().await {
        Ok(info) => status.repo_info = Some(info),
        Err(error) => {
            status.error.get_or_insert(error.to_string());
        }
    }
    Ok(status)
}

/// Verifies and stores a GitHub token for `host` in the OS keychain.
#[tauri::command(rename_all = "camelCase")]
pub async fn scm_github_set_token(host: String, token: String) -> Result<Viewer, String> {
    let host = host.trim().to_ascii_lowercase();
    let token = token.trim().to_owned();
    if host.is_empty() || host.contains('/') {
        return Err("Enter the GitHub host, e.g. github.com".into());
    }
    let client = GitHubClient::new(GitHubRepo::new(&host, "", ""), Some(token.clone()))
        .map_err(|e| e.to_string())?;
    let viewer = client.viewer().await.map_err(|e| e.to_string())?;
    blocking(move || github::set_token(&host, &token))
        .await?
        .map_err(|e| e.to_string())?;
    Ok(viewer)
}

/// Removes the stored token; returns where a token would now come from
/// (environment or GitHub CLI), if anywhere.
#[tauri::command(rename_all = "camelCase")]
pub async fn scm_github_clear_token(host: String) -> Result<TokenSource, String> {
    blocking(move || {
        github::clear_token(&host)?;
        Ok::<_, vantadeck_studio::StudioError>(github::token_source(&host))
    })
    .await?
    .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
pub async fn scm_prs(
    root: String,
    state: State<'_, DesktopState>,
) -> Result<Vec<PullRequest>, String> {
    github_client(&state, Path::new(&root))
        .await?
        .list_pull_requests()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
pub async fn scm_pr(
    root: String,
    number: u64,
    state: State<'_, DesktopState>,
) -> Result<PullRequest, String> {
    github_client(&state, Path::new(&root))
        .await?
        .pull_request(number)
        .await
        .map_err(|e| e.to_string())
}

/// The pull request for `branch` (default: the current branch).
#[tauri::command(rename_all = "camelCase")]
pub async fn scm_pr_for_branch(
    root: String,
    branch: Option<String>,
    state: State<'_, DesktopState>,
) -> Result<Option<PullRequest>, String> {
    let path = Path::new(&root);
    let branch = match branch {
        Some(branch) => branch,
        None => current_branch(&state, path).await?,
    };
    github_client(&state, path)
        .await?
        .pr_for_branch(&branch)
        .await
        .map_err(|e| e.to_string())
}

/// Opens a pull request. `head` defaults to the current branch (which must
/// be published), `base` to the repository's default branch.
#[tauri::command(rename_all = "camelCase")]
pub async fn scm_pr_create(
    root: String,
    title: String,
    body: Option<String>,
    base: Option<String>,
    head: Option<String>,
    draft: Option<bool>,
    state: State<'_, DesktopState>,
) -> Result<PullRequest, String> {
    let path = Path::new(&root);
    let client = github_client(&state, path).await?;
    let head = match head {
        Some(head) => head,
        None => {
            let branch = current_branch(&state, path).await?;
            let status = git(&state).status(path).await.map_err(|e| e.to_string())?;
            if !status.has_upstream {
                return Err(format!(
                    "Publish '{branch}' to GitHub before opening a pull request."
                ));
            }
            branch
        }
    };
    let base = match base {
        Some(base) => base,
        None => {
            client
                .repo_info()
                .await
                .map_err(|e| e.to_string())?
                .default_branch
        }
    };
    client
        .create_pull_request(&NewPullRequest {
            title,
            body,
            base,
            head,
            draft: draft.unwrap_or(false),
        })
        .await
        .map_err(|e| e.to_string())
}

/// Edits title/body, or toggles draft (`draft: false` = ready for review).
#[tauri::command(rename_all = "camelCase")]
pub async fn scm_pr_update(
    root: String,
    number: u64,
    title: Option<String>,
    body: Option<String>,
    draft: Option<bool>,
    state: State<'_, DesktopState>,
) -> Result<PullRequest, String> {
    github_client(&state, Path::new(&root))
        .await?
        .update_pull_request(number, &PullRequestUpdate { title, body, draft })
        .await
        .map_err(|e| e.to_string())
}

/// Requests reviews; team reviewers are given as `team:<slug>`.
#[tauri::command(rename_all = "camelCase")]
pub async fn scm_pr_request_reviewers(
    root: String,
    number: u64,
    reviewers: Vec<String>,
    state: State<'_, DesktopState>,
) -> Result<PullRequest, String> {
    github_client(&state, Path::new(&root))
        .await?
        .request_reviewers(number, &reviewers)
        .await
        .map_err(|e| e.to_string())
}

/// Adds a comment; returns its URL.
#[tauri::command(rename_all = "camelCase")]
pub async fn scm_pr_comment(
    root: String,
    number: u64,
    body: String,
    state: State<'_, DesktopState>,
) -> Result<String, String> {
    github_client(&state, Path::new(&root))
        .await?
        .add_comment(number, &body)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
pub async fn scm_pr_merge(
    root: String,
    number: u64,
    method: MergeMethod,
    commit_title: Option<String>,
    commit_message: Option<String>,
    confirmed: bool,
    state: State<'_, DesktopState>,
) -> Result<MergeResult, String> {
    require_confirmation(confirmed)?;
    github_client(&state, Path::new(&root))
        .await?
        .merge_pull_request(
            number,
            method,
            commit_title.as_deref(),
            commit_message.as_deref(),
        )
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
pub async fn scm_pr_close(
    root: String,
    number: u64,
    confirmed: bool,
    state: State<'_, DesktopState>,
) -> Result<PullRequest, String> {
    require_confirmation(confirmed)?;
    github_client(&state, Path::new(&root))
        .await?
        .close_pull_request(number)
        .await
        .map_err(|e| e.to_string())
}

/// CI checks for a branch, tag, or SHA (default: the current branch).
#[tauri::command(rename_all = "camelCase")]
pub async fn scm_checks_for_branch(
    root: String,
    branch: Option<String>,
    state: State<'_, DesktopState>,
) -> Result<ChecksSummary, String> {
    let path = Path::new(&root);
    let git_ref = match branch {
        Some(branch) => branch,
        None => current_branch(&state, path).await?,
    };
    github_client(&state, path)
        .await?
        .checks_for_ref(&git_ref)
        .await
        .map_err(|e| e.to_string())
}

/// Re-runs the failed jobs of one GitHub Actions run.
#[tauri::command(rename_all = "camelCase")]
pub async fn scm_rerun_failed(
    root: String,
    run_id: u64,
    state: State<'_, DesktopState>,
) -> Result<(), String> {
    github_client(&state, Path::new(&root))
        .await?
        .rerun_failed_jobs(run_id)
        .await
        .map_err(|e| e.to_string())
}

/// Re-runs every failed Actions run for a ref (default: current branch)
/// whose attempt count is below `maxAttempts` (default 2).
#[tauri::command(rename_all = "camelCase")]
pub async fn scm_auto_retry_checks(
    root: String,
    branch: Option<String>,
    max_attempts: Option<u32>,
    state: State<'_, DesktopState>,
) -> Result<RetryReport, String> {
    let path = Path::new(&root);
    let git_ref = match branch {
        Some(branch) => branch,
        None => current_branch(&state, path).await?,
    };
    github_client(&state, path)
        .await?
        .auto_retry_failed_checks(&git_ref, max_attempts.unwrap_or(2))
        .await
        .map_err(|e| e.to_string())
}

/// Log tails of the failed jobs in an Actions run.
#[tauri::command(rename_all = "camelCase")]
pub async fn scm_failed_job_logs(
    root: String,
    run_id: u64,
    state: State<'_, DesktopState>,
) -> Result<Vec<JobLog>, String> {
    github_client(&state, Path::new(&root))
        .await?
        .failed_job_logs(run_id)
        .await
        .map_err(|e| e.to_string())
}
