use std::{
    io,
    path::{Path, PathBuf},
    process::Output,
};

use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::process::Command;
use vantadeck_domain::{HealthIssue, HealthSeverity};
use walkdir::WalkDir;

mod p4;
pub use p4::*;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VcsStatus {
    pub branch: Option<String>,
    #[serde(default)]
    pub ahead: u32,
    #[serde(default)]
    pub behind: u32,
    pub changed_files: Vec<ChangedFile>,
}

/// Starter ignore rules for a fresh creative-project repo. Keeps `.vantadeck/`
/// tracked (it's the portable project config) while excluding engine caches and
/// build output. Written only when the project has no `.gitignore` yet.
const DEFAULT_GITIGNORE: &str = "# Engine caches and build output\n\
Library/\n\
Temp/\n\
Logs/\n\
obj/\n\
bin/\n\
.vs/\n\
Binaries/\n\
Intermediate/\n\
Saved/\n\
DerivedDataCache/\n\
node_modules/\n\
__pycache__/\n\
*.tmp\n";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GitCommit {
    pub hash: String,
    pub short_hash: String,
    pub author: String,
    pub date: String,
    pub subject: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChangedFile {
    pub path: String,
    pub status: String,
}

#[derive(Debug, Error)]
pub enum VcsError {
    #[error("invalid git porcelain record: {0}")]
    InvalidRecord(String),
    #[error("version-control command failed to start: {0}")]
    Io(#[from] io::Error),
    #[error("version-control command `{command}` failed: {stderr}")]
    CommandFailed { command: String, stderr: String },
    #[error("commit message cannot be empty")]
    EmptyCommitMessage,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VcsOperationResult {
    pub stdout: String,
    pub stderr: String,
}

#[async_trait]
pub trait VersionControlProvider: Send + Sync {
    async fn detect(&self, root: &Path) -> bool;
    async fn status(&self, root: &Path) -> Result<VcsStatus, VcsError>;
    async fn sync(&self, root: &Path) -> Result<VcsOperationResult, VcsError>;
    async fn commit(&self, root: &Path, message: &str) -> Result<VcsOperationResult, VcsError>;
    async fn push(&self, root: &Path) -> Result<VcsOperationResult, VcsError>;
}

#[derive(Debug, Clone)]
pub struct GitProvider {
    binary: PathBuf,
}

impl GitProvider {
    pub fn new(binary: impl Into<PathBuf>) -> Self {
        Self {
            binary: binary.into(),
        }
    }

    pub async fn status(&self, root: &Path) -> Result<VcsStatus, VcsError> {
        let output = self
            .run(
                root,
                &[
                    "status",
                    "--porcelain=v2",
                    "--branch",
                    "--untracked-files=all",
                ],
            )
            .await?;
        parse_git_porcelain_v2(&String::from_utf8_lossy(&output.stdout))
    }

    pub async fn commit_all(
        &self,
        root: &Path,
        message: &str,
    ) -> Result<VcsOperationResult, VcsError> {
        if message.trim().is_empty() {
            return Err(VcsError::EmptyCommitMessage);
        }
        self.run(root, &["add", "-A"]).await?;
        self.operation(root, &["commit", "-m", message]).await
    }

    pub async fn sync(&self, root: &Path) -> Result<VcsOperationResult, VcsError> {
        self.operation(root, &["pull", "--ff-only"]).await
    }

    pub async fn push(&self, root: &Path) -> Result<VcsOperationResult, VcsError> {
        self.operation(root, &["push"]).await
    }

    pub async fn switch_branch(
        &self,
        root: &Path,
        branch: &str,
    ) -> Result<VcsOperationResult, VcsError> {
        self.operation(root, &["switch", branch]).await
    }

    pub async fn create_branch(
        &self,
        root: &Path,
        branch: &str,
    ) -> Result<VcsOperationResult, VcsError> {
        self.operation(root, &["switch", "-c", branch]).await
    }

    /// Local branch names for the repository.
    pub async fn branches(&self, root: &Path) -> Result<Vec<String>, VcsError> {
        let output = self
            .run(root, &["branch", "--format=%(refname:short)"])
            .await?;
        Ok(String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(|line| line.trim().to_string())
            .filter(|line| !line.is_empty())
            .collect())
    }

    /// Recent commit history.
    pub async fn log(&self, root: &Path, limit: u32) -> Result<Vec<GitCommit>, VcsError> {
        // Unit separator (\x1f) between fields, record separator (\x1e) between commits.
        let format = "--pretty=format:%H%x1f%h%x1f%an%x1f%ad%x1f%s%x1e";
        let output = self
            .run(
                root,
                &["log", &format!("-n{limit}"), "--date=short", format],
            )
            .await?;
        let text = String::from_utf8_lossy(&output.stdout);
        Ok(text
            .split('\u{1e}')
            .filter_map(|record| {
                let record = record.trim_start_matches('\n');
                if record.trim().is_empty() {
                    return None;
                }
                let fields: Vec<&str> = record.split('\u{1f}').collect();
                if fields.len() < 5 {
                    return None;
                }
                Some(GitCommit {
                    hash: fields[0].to_owned(),
                    short_hash: fields[1].to_owned(),
                    author: fields[2].to_owned(),
                    date: fields[3].to_owned(),
                    subject: fields[4].to_owned(),
                })
            })
            .collect())
    }

    /// Diff for a single path (working tree vs HEAD; falls back for untracked).
    pub async fn diff(&self, root: &Path, path: &str) -> Result<String, VcsError> {
        let output = self.run_raw(root, &["diff", "HEAD", "--", path]).await?;
        let diff = String::from_utf8_lossy(&output.stdout).into_owned();
        if !diff.trim().is_empty() {
            return Ok(diff);
        }
        // Untracked/new file: show its current contents as an addition-style view.
        let untracked = self
            .run_raw(root, &["diff", "--no-index", "--", "/dev/null", path])
            .await
            .map(|out| String::from_utf8_lossy(&out.stdout).into_owned())
            .unwrap_or_default();
        Ok(untracked)
    }

    /// Diff for a single path as introduced by a specific commit (vs its parent).
    pub async fn commit_diff(
        &self,
        root: &Path,
        hash: &str,
        path: &str,
    ) -> Result<String, VcsError> {
        let output = self
            .run(root, &["show", "--format=", hash, "--", path])
            .await?;
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    /// Files changed by a single commit, with their status letters (A/M/D/R…).
    pub async fn commit_files(
        &self,
        root: &Path,
        hash: &str,
    ) -> Result<Vec<ChangedFile>, VcsError> {
        let output = self
            .run(
                root,
                &["show", "--name-status", "--pretty=format:", "-z", hash],
            )
            .await?;
        // With -z, records are NUL-separated. A rename/copy entry is three fields
        // (status, old, new); everything else is two (status, path).
        let text = String::from_utf8_lossy(&output.stdout);
        let mut fields = text.split('\u{0}').filter(|field| !field.is_empty());
        let mut files = Vec::new();
        while let Some(status) = fields.next() {
            let renamed = status.starts_with('R') || status.starts_with('C');
            if renamed {
                fields.next();
            }
            if let Some(path) = fields.next() {
                files.push(ChangedFile {
                    status: status.chars().next().map(String::from).unwrap_or_default(),
                    path: path.to_owned(),
                });
            }
        }
        Ok(files)
    }

    /// Discards local changes to a single path: reverts a tracked file to HEAD
    /// (unstaging and restoring the working copy), or deletes an untracked file.
    /// Destructive — callers must confirm with the user first.
    pub async fn discard_path(
        &self,
        root: &Path,
        path: &str,
    ) -> Result<VcsOperationResult, VcsError> {
        let tracked = self
            .run_raw(root, &["ls-files", "--error-unmatch", "--", path])
            .await
            .map(|output| output.status.success())
            .unwrap_or(false);
        if tracked {
            self.run(
                root,
                &[
                    "restore",
                    "--staged",
                    "--worktree",
                    "--source=HEAD",
                    "--",
                    path,
                ],
            )
            .await
            .map(|output| VcsOperationResult {
                stdout: String::from_utf8_lossy(&output.stdout).trim().to_owned(),
                stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
            })
        } else {
            let target = root.join(path);
            std::fs::remove_file(&target)?;
            Ok(VcsOperationResult {
                stdout: format!("Deleted untracked {path}"),
                stderr: String::new(),
            })
        }
    }

    /// Stashes all local changes (including untracked files) under a labeled
    /// message, so they can be identified and restored later — used to let a
    /// user "leave changes behind" when switching branches.
    pub async fn stash_push(
        &self,
        root: &Path,
        message: &str,
    ) -> Result<VcsOperationResult, VcsError> {
        self.operation(
            root,
            &["stash", "push", "--include-untracked", "-m", message],
        )
        .await
    }

    /// Re-applies and drops the most recent stash — used to "bring changes"
    /// along after switching branches.
    pub async fn stash_pop(&self, root: &Path) -> Result<VcsOperationResult, VcsError> {
        self.operation(root, &["stash", "pop"]).await
    }

    /// Stash entries as `stash@{n} <message>` lines, newest first.
    pub async fn stash_list(&self, root: &Path) -> Result<Vec<String>, VcsError> {
        let output = self
            .run(root, &["stash", "list", "--format=%gd\u{1f}%s"])
            .await?;
        Ok(String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::to_owned)
            .filter(|line| !line.is_empty())
            .collect())
    }

    /// Whether there are any uncommitted changes (staged, unstaged, or
    /// untracked) in the working tree.
    pub async fn has_uncommitted_changes(&self, root: &Path) -> Result<bool, VcsError> {
        Ok(!self.status(root).await?.changed_files.is_empty())
    }

    /// Whether a usable `git` is available on this machine.
    pub async fn is_installed(&self) -> bool {
        self.run_raw(std::path::Path::new("."), &["--version"])
            .await
            .is_ok_and(|output| output.status.success())
    }

    /// Initializes a Git repository in `root`: ensures a commit identity, writes a
    /// starter `.gitignore` if missing, makes an initial commit, and — when a
    /// `remote` URL is given — adds it as `origin` and pushes. Authentication for
    /// the push is handled by the user's Git credential helper (browser sign-in);
    /// this never handles passwords.
    pub async fn init_repo(
        &self,
        root: &Path,
        remote: Option<&str>,
    ) -> Result<VcsOperationResult, VcsError> {
        // Ensure a commit identity exists so the initial commit doesn't fail on a
        // fresh machine; set a machine-local one only if none is configured.
        let has_identity = self
            .run_raw(root, &["config", "user.email"])
            .await
            .map(|output| output.status.success() && !output.stdout.is_empty())
            .unwrap_or(false);
        if !has_identity {
            let user = std::env::var("USERNAME")
                .or_else(|_| std::env::var("USER"))
                .unwrap_or_else(|_| "Pipeline OS".into());
            let email = format!("{}@users.noreply.github.com", user.replace(' ', ""));
            let _ = self.run(root, &["config", "user.name", &user]).await;
            let _ = self.run(root, &["config", "user.email", &email]).await;
        }
        // Initialize with `main` as the default branch (fall back for older Git).
        if self.run(root, &["init", "-b", "main"]).await.is_err() {
            self.run(root, &["init"]).await?;
            let _ = self
                .run(root, &["symbolic-ref", "HEAD", "refs/heads/main"])
                .await;
        }
        let gitignore = root.join(".gitignore");
        if !gitignore.exists() {
            let _ = std::fs::write(&gitignore, DEFAULT_GITIGNORE);
        }
        self.run(root, &["add", "-A"]).await?;
        // Tolerate "nothing to commit" for an otherwise-empty project.
        let _ = self.run(root, &["commit", "-m", "Initial commit"]).await;
        if let Some(url) = remote.filter(|value| !value.trim().is_empty()) {
            let _ = self.run(root, &["remote", "remove", "origin"]).await;
            self.run(root, &["remote", "add", "origin", url]).await?;
            self.run(root, &["push", "-u", "origin", "HEAD"]).await?;
        }
        Ok(VcsOperationResult {
            stdout: "Repository initialized".into(),
            stderr: String::new(),
        })
    }

    /// The current branch name (cheap; `HEAD` when detached). Used to annotate
    /// project summaries without running a full status.
    pub async fn current_branch(&self, root: &Path) -> Option<String> {
        let output = self
            .run_raw(root, &["rev-parse", "--abbrev-ref", "HEAD"])
            .await
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let branch = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        (!branch.is_empty()).then_some(branch)
    }

    /// Stages and commits only the given paths.
    pub async fn commit_paths(
        &self,
        root: &Path,
        message: &str,
        paths: &[String],
    ) -> Result<VcsOperationResult, VcsError> {
        if message.trim().is_empty() {
            return Err(VcsError::EmptyCommitMessage);
        }
        if paths.is_empty() {
            return self.commit_all(root, message).await;
        }
        let mut add_args = vec!["add", "--"];
        add_args.extend(paths.iter().map(String::as_str));
        self.run(root, &add_args).await?;
        let mut commit_args = vec!["commit", "-m", message, "--"];
        commit_args.extend(paths.iter().map(String::as_str));
        self.operation(root, &commit_args).await
    }

    pub async fn lfs_probe(&self, root: &Path, large_file_threshold: u64) -> LfsProbe {
        let installed = self
            .run_raw(root, &["lfs", "version"])
            .await
            .is_ok_and(|output| output.status.success());
        let initialized = std::fs::read_to_string(root.join(".gitattributes"))
            .is_ok_and(|attributes| attributes.contains("filter=lfs"));
        // Note: we deliberately do not run `git lfs fsck` here. It is very
        // expensive on large repositories and spawns a storm of child console
        // processes; object integrity belongs in an explicit, user-initiated check.
        let missing_objects = false;
        let mut large_untracked_files = Vec::new();
        // Cap the number of large files we inspect so health stays fast and never
        // floods a big project (e.g. a game engine project) with subprocesses.
        const MAX_LARGE_FILES: usize = 25;
        for entry in WalkDir::new(root)
            .follow_links(false)
            .into_iter()
            .filter_entry(|entry| {
                let name = entry.file_name().to_string_lossy();
                !matches!(
                    name.as_ref(),
                    ".git"
                        | ".vantadeck"
                        | "node_modules"
                        | "Library"
                        | "Temp"
                        | "Logs"
                        | "obj"
                        | "bin"
                        | ".vs"
                        | "Intermediate"
                        | "Saved"
                        | "Binaries"
                        | "DerivedDataCache"
                )
            })
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_file())
        {
            if large_untracked_files.len() >= MAX_LARGE_FILES {
                break;
            }
            if entry
                .metadata()
                .map_or(true, |metadata| metadata.len() < large_file_threshold)
            {
                continue;
            }
            let relative = entry.path().strip_prefix(root).unwrap_or(entry.path());
            let path_argument = relative.to_string_lossy();
            let tracked = self
                .run_raw(
                    root,
                    &["check-attr", "filter", "--", path_argument.as_ref()],
                )
                .await
                .is_ok_and(|output| {
                    output.status.success()
                        && String::from_utf8_lossy(&output.stdout).contains(": filter: lfs")
                });
            if !tracked {
                large_untracked_files.push(path_argument.replace('\\', "/"));
            }
        }
        LfsProbe {
            installed,
            initialized,
            missing_objects,
            large_untracked_files,
        }
    }

    /// Total size in bytes of the `.git` directory — a proxy for repository
    /// history bloat (large blobs committed directly instead of via LFS,
    /// unpruned history, etc.). Runs on a blocking thread since it walks the
    /// whole directory tree.
    pub async fn repo_size_bytes(&self, root: &Path) -> u64 {
        let git_dir = root.join(".git");
        tokio::task::spawn_blocking(move || {
            if !git_dir.is_dir() {
                return 0;
            }
            WalkDir::new(&git_dir)
                .follow_links(false)
                .into_iter()
                .filter_map(Result::ok)
                .filter(|entry| entry.file_type().is_file())
                .filter_map(|entry| entry.metadata().ok())
                .map(|metadata| metadata.len())
                .sum()
        })
        .await
        .unwrap_or(0)
    }

    async fn operation(
        &self,
        root: &Path,
        arguments: &[&str],
    ) -> Result<VcsOperationResult, VcsError> {
        let output = self.run(root, arguments).await?;
        Ok(VcsOperationResult {
            stdout: String::from_utf8_lossy(&output.stdout).trim().to_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        })
    }

    async fn run(&self, root: &Path, arguments: &[&str]) -> Result<Output, VcsError> {
        let output = self.run_raw(root, arguments).await?;
        if output.status.success() {
            return Ok(output);
        }
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        // A handful of Git failures stem from a stale/corrupted local ref state
        // (most commonly `refs/remotes/origin/HEAD` left pointing at a bad
        // object after an interrupted clone or a remote default-branch rename)
        // rather than anything the user did. These are silently self-healable:
        // repair the ref state and retry once before surfacing an error.
        if Self::looks_like_corrupt_ref_error(&stderr) {
            self.repair_corrupt_refs(root).await;
            let retry = self.run_raw(root, arguments).await?;
            if retry.status.success() {
                return Ok(retry);
            }
            return Err(VcsError::CommandFailed {
                command: format!("git {}", arguments.join(" ")),
                stderr: String::from_utf8_lossy(&retry.stderr).trim().to_owned(),
            });
        }
        Err(VcsError::CommandFailed {
            command: format!("git {}", arguments.join(" ")),
            stderr,
        })
    }

    /// Whether a Git failure looks like stale/corrupted local ref state rather
    /// than a real error (auth, conflicts, etc.) worth surfacing as-is.
    fn looks_like_corrupt_ref_error(stderr: &str) -> bool {
        stderr.contains("bad object refs/remotes/")
            || stderr.contains("did not send all necessary objects")
            || stderr.contains("unable to resolve reference")
            || (stderr.contains("fatal:") && stderr.contains("ambiguous argument 'HEAD'"))
            || stderr.contains("HEAD: not a valid SHA1")
    }

    /// Best-effort, silent repair of common corrupted-ref states: removes a
    /// bad `refs/remotes/origin/HEAD` (found zeroed-out or otherwise invalid
    /// after some interrupted operations) and reconstructs it from the
    /// remote's actual default branch. Never surfaces failures to the caller —
    /// worst case, the retried command fails again with its original error.
    async fn repair_corrupt_refs(&self, root: &Path) {
        let git_dir = self
            .run_raw(root, &["rev-parse", "--git-common-dir"])
            .await
            .ok()
            .filter(|output| output.status.success())
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned());
        if let Some(git_dir) = git_dir {
            let git_dir = if Path::new(&git_dir).is_absolute() {
                PathBuf::from(git_dir)
            } else {
                root.join(git_dir)
            };
            let bad_head = git_dir
                .join("refs")
                .join("remotes")
                .join("origin")
                .join("HEAD");
            if bad_head.exists() {
                let _ = std::fs::remove_file(&bad_head);
            }
        }
        let _ = self
            .run_raw(root, &["remote", "set-head", "origin", "-a"])
            .await;
    }

    async fn run_raw(&self, root: &Path, arguments: &[&str]) -> io::Result<Output> {
        let mut command = Command::new(&self.binary);
        command.current_dir(root).args(arguments);
        // Never block on an interactive credential/terminal prompt: fail fast
        // instead, so a repo needing auth can't hang status/health scans.
        command
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GCM_INTERACTIVE", "never");
        // Run git without flashing a console window on Windows.
        #[cfg(windows)]
        command.creation_flags(0x0800_0000);
        command.output().await
    }
}

#[async_trait]
impl VersionControlProvider for GitProvider {
    async fn detect(&self, root: &Path) -> bool {
        self.run_raw(root, &["rev-parse", "--is-inside-work-tree"])
            .await
            .is_ok_and(|output| output.status.success())
    }

    async fn status(&self, root: &Path) -> Result<VcsStatus, VcsError> {
        GitProvider::status(self, root).await
    }

    async fn sync(&self, root: &Path) -> Result<VcsOperationResult, VcsError> {
        GitProvider::sync(self, root).await
    }

    async fn commit(&self, root: &Path, message: &str) -> Result<VcsOperationResult, VcsError> {
        self.commit_all(root, message).await
    }

    async fn push(&self, root: &Path) -> Result<VcsOperationResult, VcsError> {
        GitProvider::push(self, root).await
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LfsProbe {
    pub installed: bool,
    pub initialized: bool,
    pub missing_objects: bool,
    pub large_untracked_files: Vec<String>,
}

pub fn evaluate_lfs_health(probe: &LfsProbe) -> Vec<HealthIssue> {
    let mut issues = Vec::new();
    // A project only "needs" LFS if it has already opted in (`.gitattributes`
    // declares an lfs filter) or it has large files that should be tracked by
    // one. This mirrors how GitHub Desktop decides whether to surface LFS at
    // all: it prompts based on large-file detection, not a blanket "you
    // don't have LFS configured" warning on every repository regardless of
    // whether it has anything LFS would ever touch.
    let needs_lfs = probe.initialized || !probe.large_untracked_files.is_empty();
    if needs_lfs && !probe.installed {
        issues.push(health_issue(
            "GIT_LFS_NOT_INSTALLED",
            HealthSeverity::Error,
            "Git LFS is not installed",
            "Install Git LFS before syncing repositories that use LFS.",
        ));
    }
    if !probe.initialized && !probe.large_untracked_files.is_empty() {
        issues.push(health_issue(
            "GIT_LFS_NOT_INITIALIZED",
            HealthSeverity::Warning,
            "Large files aren't tracked by Git LFS yet",
            "This project has large files that most remotes reject or discourage in normal Git history. Add LFS patterns to .gitattributes and run `git lfs track`.",
        ));
    }
    if needs_lfs && probe.missing_objects {
        issues.push(health_issue(
            "GIT_LFS_MISSING_OBJECTS",
            HealthSeverity::Error,
            "Git LFS objects are missing",
            "Run git lfs fetch and inspect git lfs fsck output.",
        ));
    }
    if !probe.large_untracked_files.is_empty() {
        issues.push(HealthIssue {
            code: "LARGE_FILE_NOT_TRACKED".into(),
            severity: HealthSeverity::Warning,
            title: "Large files are not tracked by Git LFS".into(),
            detail: probe.large_untracked_files.join(", "),
            remediation: Some("Review and add appropriate Git LFS patterns.".into()),
            checked_at: Utc::now(),
        });
    }
    issues
}

const LARGE_REPO_HISTORY_BYTES: u64 = 2 * 1024 * 1024 * 1024; // 2 GiB

/// Flags a bloated `.git` directory — usually large binaries committed
/// directly into history instead of through LFS, which makes every clone,
/// fetch, and checkout slower over time and can't be fixed by discarding the
/// working tree (the bloat lives in history).
pub fn evaluate_repo_size_health(size_bytes: u64) -> Vec<HealthIssue> {
    if size_bytes < LARGE_REPO_HISTORY_BYTES {
        return Vec::new();
    }
    vec![HealthIssue {
        code: "REPO_HISTORY_LARGE".into(),
        severity: HealthSeverity::Warning,
        title: "Git history is large".into(),
        detail: format!(
            "The .git folder is {:.1} GB, which slows down clones, fetches, and checkouts.",
            size_bytes as f64 / (1024.0 * 1024.0 * 1024.0)
        ),
        remediation: Some(
            "Track large binaries with Git LFS going forward, and consider rewriting history (e.g. git filter-repo) to remove existing large blobs if this repo will be cloned often.".into(),
        ),
        checked_at: Utc::now(),
    }]
}

fn health_issue(
    code: &str,
    severity: HealthSeverity,
    title: &str,
    remediation: &str,
) -> HealthIssue {
    HealthIssue {
        code: code.into(),
        severity,
        title: title.into(),
        detail: title.into(),
        remediation: Some(remediation.into()),
        checked_at: Utc::now(),
    }
}

pub fn parse_git_porcelain_v2(input: &str) -> Result<VcsStatus, VcsError> {
    let mut status = VcsStatus {
        branch: None,
        ahead: 0,
        behind: 0,
        changed_files: Vec::new(),
    };
    for line in input.lines() {
        if let Some(branch) = line.strip_prefix("# branch.head ") {
            status.branch = (branch != "(detached)").then(|| branch.to_owned());
        } else if let Some(ab) = line.strip_prefix("# branch.ab ") {
            // Format: "+<ahead> -<behind>"
            for token in ab.split_whitespace() {
                if let Some(value) = token.strip_prefix('+') {
                    status.ahead = value.parse().unwrap_or(0);
                } else if let Some(value) = token.strip_prefix('-') {
                    status.behind = value.parse().unwrap_or(0);
                }
            }
        } else if let Some(path) = line.strip_prefix("? ") {
            status.changed_files.push(ChangedFile {
                path: path.to_owned(),
                status: "untracked".into(),
            });
        } else if line.starts_with("1 ") {
            let fields: Vec<_> = line.splitn(9, ' ').collect();
            if fields.len() != 9 {
                return Err(VcsError::InvalidRecord(line.to_owned()));
            }
            status.changed_files.push(ChangedFile {
                status: fields[1].to_owned(),
                path: fields[8].to_owned(),
            });
        } else if line.starts_with("2 ") {
            // Renamed/copied entry. The final field is "<newPath>\t<origPath>";
            // we surface the new path with its XY status.
            let fields: Vec<_> = line.splitn(10, ' ').collect();
            if fields.len() != 10 {
                return Err(VcsError::InvalidRecord(line.to_owned()));
            }
            let path = fields[9].split('\t').next().unwrap_or(fields[9]);
            status.changed_files.push(ChangedFile {
                status: fields[1].to_owned(),
                path: path.to_owned(),
            });
        }
    }
    Ok(status)
}
