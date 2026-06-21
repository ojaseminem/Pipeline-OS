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
    pub changed_files: Vec<ChangedFile>,
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

    pub async fn lfs_probe(&self, root: &Path, large_file_threshold: u64) -> LfsProbe {
        let installed = self
            .run_raw(root, &["lfs", "version"])
            .await
            .is_ok_and(|output| output.status.success());
        let initialized = std::fs::read_to_string(root.join(".gitattributes"))
            .is_ok_and(|attributes| attributes.contains("filter=lfs"));
        let missing_objects = installed
            && initialized
            && self
                .run_raw(root, &["lfs", "fsck"])
                .await
                .is_ok_and(|output| !output.status.success());
        let mut large_untracked_files = Vec::new();
        for entry in WalkDir::new(root)
            .follow_links(false)
            .into_iter()
            .filter_entry(|entry| {
                let name = entry.file_name().to_string_lossy();
                name != ".git" && name != ".vantadeck"
            })
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_file())
        {
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
        changed_files: Vec::new(),
    };
    for line in input.lines() {
        if let Some(branch) = line.strip_prefix("# branch.head ") {
            status.branch = (branch != "(detached)").then(|| branch.to_owned());
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
        }
    }
    Ok(status)
}
