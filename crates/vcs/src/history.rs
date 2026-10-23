//! Branch, tag, and history operations with safety rails: deleting or
//! rewriting only what can't lose anyone's work, plus blame, per-file
//! history, cherry-pick/revert, and stash inspection.

use std::{collections::HashMap, path::Path};

use chrono::{DateTime, FixedOffset, TimeZone};
use serde::{Deserialize, Serialize};

use crate::{
    ChangedFile, GitCommit, GitProvider, LOG_FORMAT, MergeOutcome, VcsError, VcsOperationResult,
    parse_log_records,
};

/// A tag in the repository, newest first.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TagInfo {
    pub name: String,
    /// Short hash of the commit the tag points at.
    pub target: String,
    /// Annotated tags carry their own message and tagger.
    pub annotated: bool,
    /// Tag (annotated) or commit (lightweight) date, RFC 3339.
    pub date: String,
    /// Tag message subject (annotated) or the commit subject (lightweight).
    pub subject: String,
}

/// One line of `git blame`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BlameLine {
    /// 1-based line number in the current file.
    pub line: u32,
    pub commit: String,
    pub short_hash: String,
    pub author: String,
    pub author_email: String,
    /// Author date, RFC 3339. Empty for uncommitted lines.
    pub date: String,
    pub summary: String,
    /// Not yet committed (working-copy change).
    pub uncommitted: bool,
    pub content: String,
}

/// What a stash entry contains.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StashDetails {
    pub stash_ref: String,
    pub files: Vec<ChangedFile>,
    /// Unified diff of the stash (truncated past ~1 MB).
    pub patch: String,
    pub truncated: bool,
}

/// Which multi-step operation is waiting for the user, if any.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PendingOperation {
    Merge,
    CherryPick,
    Revert,
    Rebase,
}

const MAX_PATCH_BYTES: usize = 1024 * 1024;

/// Rejects values that git would parse as options (`-D`, `--force`…), which
/// would otherwise let a crafted branch/tag name change a command's meaning.
fn ensure_not_option(kind: &str, value: &str) -> Result<(), VcsError> {
    if value.trim().is_empty() {
        return Err(VcsError::Refused(format!("{kind} can't be empty")));
    }
    if value.starts_with('-') {
        return Err(VcsError::Refused(format!("{kind} can't start with '-'")));
    }
    Ok(())
}

impl GitProvider {
    async fn succeeds(&self, root: &Path, arguments: &[&str]) -> bool {
        self.run_raw(root, arguments)
            .await
            .is_ok_and(|output| output.status.success())
    }

    async fn stdout_of(&self, root: &Path, arguments: &[&str]) -> Option<String> {
        let output = self.run_raw(root, arguments).await.ok()?;
        output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
    }

    async fn count(&self, root: &Path, arguments: &[&str]) -> u32 {
        self.stdout_of(root, arguments)
            .await
            .and_then(|text| text.parse().ok())
            .unwrap_or(0)
    }

    /// The URL of a remote (`origin` by default), if configured.
    pub async fn remote_url(&self, root: &Path, remote: Option<&str>) -> Option<String> {
        let remote = remote.unwrap_or("origin");
        if remote.starts_with('-') {
            return None;
        }
        self.stdout_of(root, &["remote", "get-url", remote])
            .await
            .filter(|url| !url.is_empty())
    }

    /// The remote's default branch (from `refs/remotes/origin/HEAD`), e.g.
    /// `main`. `None` when the repo has no `origin` or never recorded it.
    pub async fn default_branch(&self, root: &Path) -> Option<String> {
        self.stdout_of(
            root,
            &[
                "symbolic-ref",
                "--quiet",
                "--short",
                "refs/remotes/origin/HEAD",
            ],
        )
        .await
        .and_then(|full| full.strip_prefix("origin/").map(str::to_owned))
    }

    async fn local_branch_exists(&self, root: &Path, name: &str) -> bool {
        self.succeeds(
            root,
            &["rev-parse", "-q", "--verify", &format!("refs/heads/{name}")],
        )
        .await
    }

    async fn remote_branch_exists(&self, root: &Path, name: &str) -> bool {
        self.succeeds(
            root,
            &[
                "rev-parse",
                "-q",
                "--verify",
                &format!("refs/remotes/origin/{name}"),
            ],
        )
        .await
    }

    /// Deletes a branch locally and/or on `origin`, refusing anything that
    /// could lose work:
    ///
    /// - never the checked-out branch or the remote's default branch;
    /// - not a local branch with commits that exist nowhere else (not on any
    ///   remote, not merged into `HEAD`/the default branch) unless `force`;
    /// - not a remote branch with commits missing from the default branch
    ///   unless `force`;
    /// - remote deletion only when `remote_allowed` — the caller's hosting
    ///   permission check (GitHub role + branch protection) passed.
    pub async fn delete_branch(
        &self,
        root: &Path,
        name: &str,
        remote: bool,
        force: bool,
        remote_allowed: bool,
    ) -> Result<VcsOperationResult, VcsError> {
        ensure_not_option("branch name", name)?;
        if self.current_branch(root).await.as_deref() == Some(name) {
            return Err(VcsError::Refused(format!(
                "'{name}' is the current branch. Switch to another branch first."
            )));
        }
        let default = self.default_branch(root).await;
        let is_default = match &default {
            Some(default) => default == name,
            // No origin/HEAD: protect the conventional default names.
            None => matches!(name, "main" | "master"),
        };
        if is_default {
            return Err(VcsError::Refused(format!(
                "'{name}' is the repository's default branch and can't be deleted here."
            )));
        }
        let default_ref = default
            .as_deref()
            .map(|branch| format!("refs/remotes/origin/{branch}"));

        let local_exists = self.local_branch_exists(root, name).await;
        let remote_exists = remote && self.remote_branch_exists(root, name).await;
        if !local_exists && !remote_exists {
            return Err(VcsError::Refused(format!("branch '{name}' doesn't exist")));
        }

        if local_exists && !force {
            // Commits reachable only from this branch: on no remote, and not
            // merged into HEAD or the default branch.
            let local_ref = format!("refs/heads/{name}");
            let mut arguments = vec![
                "rev-list",
                "--count",
                local_ref.as_str(),
                "--not",
                "--remotes",
                "HEAD",
            ];
            if let Some(default_ref) = &default_ref {
                arguments.push(default_ref.as_str());
            }
            let only_here = self.count(root, &arguments).await;
            if only_here > 0 {
                return Err(VcsError::Refused(format!(
                    "'{name}' has {only_here} commit(s) that aren't pushed or merged anywhere. \
                     Push or merge it first, or delete with force to discard them."
                )));
            }
        }
        if remote && remote_exists {
            if !remote_allowed {
                return Err(VcsError::Refused(format!(
                    "You don't have permission to delete '{name}' on the remote."
                )));
            }
            if !force {
                let remote_ref = format!("refs/remotes/origin/{name}");
                let mut arguments = vec!["rev-list", "--count", remote_ref.as_str(), "--not"];
                match &default_ref {
                    Some(default_ref) => arguments.push(default_ref.as_str()),
                    None => arguments.push("HEAD"),
                }
                let unmerged = self.count(root, &arguments).await;
                if unmerged > 0 {
                    return Err(VcsError::Refused(format!(
                        "origin/{name} has {unmerged} commit(s) not merged into the default branch. \
                         Merge it first, or delete with force."
                    )));
                }
            }
        }

        let mut messages = Vec::new();
        let mut attempts = 1;
        if local_exists {
            let flag = if force { "-D" } else { "-d" };
            let result = self.operation(root, &["branch", flag, "--", name]).await?;
            messages.push(result.stdout);
        }
        if remote && remote_exists {
            let result = self
                .network_operation(root, &["push", "origin", "--delete", name])
                .await?;
            attempts = result.attempts;
            messages.push(format!("Deleted origin/{name}"));
        } else if remote {
            messages.push(format!(
                "origin/{name} doesn't exist; nothing to delete there"
            ));
        }
        Ok(VcsOperationResult {
            stdout: messages.join("\n"),
            stderr: String::new(),
            attempts,
        })
    }

    /// Renames a local branch (the remote branch, if any, is left alone).
    pub async fn rename_branch(
        &self,
        root: &Path,
        old: &str,
        new: &str,
    ) -> Result<VcsOperationResult, VcsError> {
        ensure_not_option("branch name", old)?;
        ensure_not_option("new branch name", new)?;
        if !self
            .succeeds(root, &["check-ref-format", "--branch", new])
            .await
        {
            return Err(VcsError::Refused(format!(
                "'{new}' isn't a valid branch name"
            )));
        }
        if self.local_branch_exists(root, new).await {
            return Err(VcsError::Refused(format!("branch '{new}' already exists")));
        }
        self.operation(root, &["branch", "-m", old, new]).await
    }

    /// Creates a tag at `target` (default `HEAD`); annotated when a message
    /// is given.
    pub async fn create_tag(
        &self,
        root: &Path,
        name: &str,
        target: Option<&str>,
        message: Option<&str>,
    ) -> Result<VcsOperationResult, VcsError> {
        ensure_not_option("tag name", name)?;
        if !self
            .succeeds(root, &["check-ref-format", &format!("refs/tags/{name}")])
            .await
        {
            return Err(VcsError::Refused(format!(
                "'{name}' isn't a valid tag name"
            )));
        }
        let target = target.filter(|t| !t.trim().is_empty()).unwrap_or("HEAD");
        ensure_not_option("tag target", target)?;
        match message.filter(|m| !m.trim().is_empty()) {
            Some(message) => {
                self.operation(root, &["tag", "-a", name, "-m", message, target])
                    .await
            }
            None => self.operation(root, &["tag", name, target]).await,
        }
    }

    /// Tags, newest first.
    pub async fn list_tags(&self, root: &Path) -> Result<Vec<TagInfo>, VcsError> {
        let format = "%(refname:short)%1f%(objecttype)%1f%(*objectname:short)%1f%(objectname:short)%1f%(creatordate:iso-strict)%1f%(contents:subject)%1e";
        let output = self
            .run(
                root,
                &[
                    "for-each-ref",
                    "refs/tags",
                    "--sort=-creatordate",
                    &format!("--format={format}"),
                ],
            )
            .await?;
        Ok(parse_tags(&String::from_utf8_lossy(&output.stdout)))
    }

    /// Pushes one tag to `origin` (with network retry).
    pub async fn push_tag(&self, root: &Path, name: &str) -> Result<VcsOperationResult, VcsError> {
        ensure_not_option("tag name", name)?;
        self.network_operation(root, &["push", "origin", &format!("refs/tags/{name}")])
            .await
    }

    /// Which multi-step operation (merge, cherry-pick, revert, rebase) is
    /// paused waiting for conflict resolution.
    pub async fn pending_operation(&self, root: &Path) -> Option<PendingOperation> {
        let git_dir = self.git_dir(root).await?;
        if git_dir.join("rebase-merge").exists() || git_dir.join("rebase-apply").exists() {
            Some(PendingOperation::Rebase)
        } else if git_dir.join("CHERRY_PICK_HEAD").exists() {
            Some(PendingOperation::CherryPick)
        } else if git_dir.join("REVERT_HEAD").exists() {
            Some(PendingOperation::Revert)
        } else if git_dir.join("MERGE_HEAD").exists() {
            Some(PendingOperation::Merge)
        } else {
            None
        }
    }

    async fn outcome_of(&self, root: &Path, arguments: &[&str]) -> Result<MergeOutcome, VcsError> {
        let output = self.run_raw(root, arguments).await?;
        if output.status.success() {
            return Ok(MergeOutcome::Merged {
                message: String::from_utf8_lossy(&output.stdout).trim().to_owned(),
            });
        }
        if self.pending_operation(root).await.is_some() {
            return Ok(MergeOutcome::Conflicts {
                files: self.conflicted_files(root).await?,
            });
        }
        Err(VcsError::CommandFailed {
            command: format!("git {}", arguments.join(" ")),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        })
    }

    async fn is_merge_commit(&self, root: &Path, hash: &str) -> bool {
        self.stdout_of(root, &["rev-list", "--parents", "-n", "1", hash])
            .await
            .is_some_and(|line| line.split_whitespace().count() > 2)
    }

    /// Applies one commit onto the current branch. Conflicts are reported
    /// like a merge; finish with `continue_merge` or `abort_operation`.
    pub async fn cherry_pick(&self, root: &Path, hash: &str) -> Result<MergeOutcome, VcsError> {
        ensure_not_option("commit", hash)?;
        if self.is_merge_commit(root, hash).await {
            self.outcome_of(root, &["cherry-pick", "-m", "1", hash])
                .await
        } else {
            self.outcome_of(root, &["cherry-pick", hash]).await
        }
    }

    /// Creates a new commit undoing `hash` (safe for pushed history).
    pub async fn revert_commit(&self, root: &Path, hash: &str) -> Result<MergeOutcome, VcsError> {
        ensure_not_option("commit", hash)?;
        if self.is_merge_commit(root, hash).await {
            self.outcome_of(root, &["revert", "--no-edit", "-m", "1", hash])
                .await
        } else {
            self.outcome_of(root, &["revert", "--no-edit", hash]).await
        }
    }

    /// Abandons whichever operation is paused (merge, cherry-pick, revert,
    /// rebase), restoring the previous state.
    pub async fn abort_operation(&self, root: &Path) -> Result<VcsOperationResult, VcsError> {
        match self.pending_operation(root).await {
            Some(PendingOperation::CherryPick) => {
                self.operation(root, &["cherry-pick", "--abort"]).await
            }
            Some(PendingOperation::Revert) => self.operation(root, &["revert", "--abort"]).await,
            Some(PendingOperation::Rebase) => self.operation(root, &["rebase", "--abort"]).await,
            Some(PendingOperation::Merge) => self.abort_merge(root).await,
            None => Err(VcsError::Refused("nothing to abort".into())),
        }
    }

    /// Whether `HEAD` is already on any remote-tracking branch (i.e. pushed),
    /// in which case rewriting it would diverge from teammates' history.
    pub async fn head_is_pushed(&self, root: &Path) -> bool {
        self.stdout_of(root, &["branch", "-r", "--contains", "HEAD"])
            .await
            .is_some_and(|text| !text.trim().is_empty())
    }

    async fn ensure_head_rewritable(&self, root: &Path, what: &str) -> Result<(), VcsError> {
        if !self
            .succeeds(root, &["rev-parse", "-q", "--verify", "HEAD"])
            .await
        {
            return Err(VcsError::Refused("there are no commits yet".into()));
        }
        if self.pending_operation(root).await.is_some() {
            return Err(VcsError::Refused(format!(
                "finish or abort the current merge before you {what}"
            )));
        }
        if self.head_is_pushed(root).await {
            return Err(VcsError::Refused(format!(
                "the last commit is already pushed, so you can't {what} without rewriting shared \
                 history. Revert it instead."
            )));
        }
        Ok(())
    }

    /// Rewrites the last commit's message, leaving its content alone (staged
    /// changes are not folded in). Only for commits that aren't pushed yet.
    pub async fn amend_last_commit_message(
        &self,
        root: &Path,
        message: &str,
    ) -> Result<VcsOperationResult, VcsError> {
        if message.trim().is_empty() {
            return Err(VcsError::EmptyCommitMessage);
        }
        self.ensure_head_rewritable(root, "amend it").await?;
        self.operation(root, &["commit", "--amend", "--only", "-m", message])
            .await
    }

    /// Undoes the last commit but keeps its changes staged (`reset --soft`).
    /// Only for commits that aren't pushed yet.
    pub async fn undo_last_commit(&self, root: &Path) -> Result<VcsOperationResult, VcsError> {
        self.ensure_head_rewritable(root, "undo it").await?;
        if !self
            .succeeds(root, &["rev-parse", "-q", "--verify", "HEAD~1"])
            .await
        {
            return Err(VcsError::Refused(
                "this is the repository's first commit; there's nothing to undo to".into(),
            ));
        }
        let subject = self
            .stdout_of(root, &["log", "-1", "--format=%s"])
            .await
            .unwrap_or_default();
        self.run(root, &["reset", "--soft", "HEAD~1"]).await?;
        Ok(VcsOperationResult {
            stdout: format!("Undid \"{subject}\"; its changes are staged."),
            stderr: String::new(),
            attempts: 1,
        })
    }

    /// Who last changed each line of `path`.
    pub async fn blame(&self, root: &Path, path: &str) -> Result<Vec<BlameLine>, VcsError> {
        let output = self
            .run(root, &["blame", "--porcelain", "--", path])
            .await?;
        Ok(parse_blame_porcelain(&String::from_utf8_lossy(
            &output.stdout,
        )))
    }

    /// Commits that touched `path`, following renames, newest first.
    pub async fn file_history(
        &self,
        root: &Path,
        path: &str,
        limit: u32,
    ) -> Result<Vec<GitCommit>, VcsError> {
        let output = self
            .run(
                root,
                &[
                    "log",
                    "--follow",
                    &format!("-n{}", limit.clamp(1, 1000)),
                    "--date=short",
                    LOG_FORMAT,
                    "--",
                    path,
                ],
            )
            .await?;
        Ok(parse_log_records(&String::from_utf8_lossy(&output.stdout)))
    }

    /// Files and patch of a stash entry (default `stash@{0}`), including
    /// untracked files when this Git supports it.
    pub async fn stash_show(
        &self,
        root: &Path,
        stash_ref: Option<&str>,
    ) -> Result<StashDetails, VcsError> {
        let stash_ref = stash_ref.unwrap_or("stash@{0}");
        if !stash_ref.starts_with("stash@{") {
            return Err(VcsError::Refused(format!(
                "'{stash_ref}' isn't a stash entry"
            )));
        }
        let names = match self
            .run(
                root,
                &[
                    "stash",
                    "show",
                    "--include-untracked",
                    "--name-status",
                    stash_ref,
                ],
            )
            .await
        {
            Ok(output) => output,
            // Git < 2.32 has no --include-untracked for `stash show`.
            Err(_) => {
                self.run(root, &["stash", "show", "--name-status", stash_ref])
                    .await?
            }
        };
        let files = String::from_utf8_lossy(&names.stdout)
            .lines()
            .filter_map(|line| {
                let (status, path) = line.split_once('\t')?;
                let path = path.rsplit('\t').next().unwrap_or(path);
                Some(ChangedFile {
                    status: status.chars().next().map(String::from).unwrap_or_default(),
                    path: path.to_owned(),
                })
            })
            .collect();
        let patch_output = match self
            .run(
                root,
                &["stash", "show", "-p", "--include-untracked", stash_ref],
            )
            .await
        {
            Ok(output) => output,
            Err(_) => self.run(root, &["stash", "show", "-p", stash_ref]).await?,
        };
        let mut patch = String::from_utf8_lossy(&patch_output.stdout).into_owned();
        let truncated = patch.len() > MAX_PATCH_BYTES;
        if truncated {
            let mut cut = MAX_PATCH_BYTES;
            while !patch.is_char_boundary(cut) {
                cut -= 1;
            }
            patch.truncate(cut);
        }
        Ok(StashDetails {
            stash_ref: stash_ref.to_owned(),
            files,
            patch,
            truncated,
        })
    }
}

fn parse_tags(text: &str) -> Vec<TagInfo> {
    text.split('\u{1e}')
        .filter_map(|record| {
            let record = record.trim_start_matches('\n');
            if record.trim().is_empty() {
                return None;
            }
            let fields: Vec<&str> = record.split('\u{1f}').collect();
            if fields.len() < 6 {
                return None;
            }
            let annotated = fields[1] == "tag";
            let target = if annotated && !fields[2].is_empty() {
                fields[2]
            } else {
                fields[3]
            };
            Some(TagInfo {
                name: fields[0].to_owned(),
                target: target.to_owned(),
                annotated,
                date: fields[4].to_owned(),
                subject: fields[5].trim().to_owned(),
            })
        })
        .collect()
}

#[derive(Default, Clone)]
struct BlameCommit {
    author: String,
    author_email: String,
    time: Option<i64>,
    tz: String,
    summary: String,
}

/// Parses `git blame --porcelain` output. Each line group starts with
/// `<sha> <orig-line> <final-line> [<count>]`; commit headers (author,
/// author-time, summary…) appear only the first time a commit is seen, and
/// the line's content follows a leading tab.
pub fn parse_blame_porcelain(text: &str) -> Vec<BlameLine> {
    let mut commits: HashMap<String, BlameCommit> = HashMap::new();
    let mut lines = Vec::new();
    let mut current: Option<(String, u32)> = None;
    for raw in text.lines() {
        if let Some(content) = raw.strip_prefix('\t') {
            let Some((sha, line)) = current.take() else {
                continue;
            };
            let info = commits.get(&sha).cloned().unwrap_or_default();
            let uncommitted = sha.chars().all(|c| c == '0');
            lines.push(BlameLine {
                line,
                short_hash: sha.chars().take(7).collect(),
                author: info.author,
                author_email: info.author_email,
                date: if uncommitted {
                    String::new()
                } else {
                    format_blame_time(info.time, &info.tz)
                },
                summary: info.summary,
                uncommitted,
                content: content.to_owned(),
                commit: sha,
            });
            continue;
        }
        let mut parts = raw.split(' ');
        let first = parts.next().unwrap_or("");
        if first.len() >= 40 && first.chars().all(|c| c.is_ascii_hexdigit()) {
            let final_line = parts.nth(1).and_then(|v| v.parse().ok()).unwrap_or(0);
            commits.entry(first.to_owned()).or_default();
            current = Some((first.to_owned(), final_line));
            continue;
        }
        let Some((sha, _)) = &current else {
            continue;
        };
        let Some(entry) = commits.get_mut(sha) else {
            continue;
        };
        let (key, value) = raw.split_once(' ').unwrap_or((raw, ""));
        match key {
            "author" => entry.author = value.to_owned(),
            "author-mail" => {
                entry.author_email = value.trim_matches(|c| c == '<' || c == '>').to_owned()
            }
            "author-time" => entry.time = value.parse().ok(),
            "author-tz" => entry.tz = value.to_owned(),
            "summary" => entry.summary = value.to_owned(),
            _ => {}
        }
    }
    lines
}

fn format_blame_time(time: Option<i64>, tz: &str) -> String {
    let Some(time) = time else {
        return String::new();
    };
    // tz is "+HHMM"/"-HHMM".
    let offset = (tz.len() == 5)
        .then(|| {
            let sign = if tz.starts_with('-') { -1 } else { 1 };
            let hours: i32 = tz[1..3].parse().ok()?;
            let minutes: i32 = tz[3..5].parse().ok()?;
            FixedOffset::east_opt(sign * (hours * 3600 + minutes * 60))
        })
        .flatten()
        .unwrap_or_else(|| FixedOffset::east_opt(0).expect("zero offset"));
    offset
        .timestamp_opt(time, 0)
        .single()
        .map(|date: DateTime<FixedOffset>| date.to_rfc3339())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_blame_porcelain_with_repeated_commits() {
        let sha = "a".repeat(40);
        let zero = "0".repeat(40);
        let text = format!(
            "{sha} 1 1 2\nauthor Ada\nauthor-mail <ada@example.com>\nauthor-time 1700000000\nauthor-tz +0100\nsummary First\nfilename f.txt\n\tline one\n{sha} 2 2\n\tline two\n{zero} 3 3 1\nauthor Not Committed Yet\nauthor-mail <not.committed.yet>\nauthor-time 1700000100\nauthor-tz +0000\nsummary Version of f.txt from f.txt\nfilename f.txt\n\tline three\n"
        );
        let lines = parse_blame_porcelain(&text);
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].author, "Ada");
        assert_eq!(lines[0].author_email, "ada@example.com");
        assert_eq!(lines[0].date, "2023-11-14T23:13:20+01:00");
        assert_eq!(lines[1].line, 2);
        assert_eq!(lines[1].summary, "First");
        assert_eq!(lines[1].content, "line two");
        assert!(lines[2].uncommitted);
        assert!(lines[2].date.is_empty());
    }

    #[test]
    fn parses_tag_records() {
        let text = "v1.0\u{1f}tag\u{1f}abc1234\u{1f}fff0000\u{1f}2024-01-01T00:00:00+00:00\u{1f}Release 1.0\u{1e}\nv0.9\u{1f}commit\u{1f}\u{1f}def5678\u{1f}2023-12-01T00:00:00+00:00\u{1f}Fix stuff\u{1e}";
        let tags = parse_tags(text);
        assert_eq!(tags.len(), 2);
        assert!(tags[0].annotated);
        assert_eq!(tags[0].target, "abc1234");
        assert!(!tags[1].annotated);
        assert_eq!(tags[1].target, "def5678");
    }

    #[test]
    fn option_like_names_are_refused() {
        assert!(ensure_not_option("branch", "-D").is_err());
        assert!(ensure_not_option("branch", "").is_err());
        assert!(ensure_not_option("branch", "feature/x").is_ok());
    }
}
