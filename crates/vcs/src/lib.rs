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
        Err(VcsError::CommandFailed {
            command: format!("git {}", arguments.join(" ")),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        })
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
    if !probe.installed {
        issues.push(health_issue(
            "GIT_LFS_NOT_INSTALLED",
            HealthSeverity::Error,
            "Git LFS is not installed",
            "Install Git LFS before syncing repositories that use LFS.",
        ));
    }
    if !probe.initialized {
        issues.push(health_issue(
            "GIT_LFS_NOT_INITIALIZED",
            HealthSeverity::Warning,
            "Git LFS is not configured",
            "Add reviewed LFS patterns to .gitattributes.",
        ));
    }
    if probe.missing_objects {
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
