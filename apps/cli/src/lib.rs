use std::{fs, io, path::PathBuf};

use clap::{Parser, Subcommand};
use thiserror::Error;
use vantadeck_application::{ApplicationError, ApplicationService};
use vantadeck_domain::{ApiMessage, CliEnvelope};
use vantadeck_projects::{ProjectError, load_project};
use vantadeck_security::ArtifactVerificationError;

#[derive(Debug, Parser)]
#[command(name = "vantadeck", version, about = "Local-first creative launcher")]
pub struct Cli {
    #[arg(long, global = true)]
    pub json: bool,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Scan {
        #[command(subcommand)]
        command: ScanCommand,
    },
    Apps {
        #[command(subcommand)]
        command: AppsCommand,
    },
    Project {
        #[command(subcommand)]
        command: ProjectCommand,
    },
    Tools {
        #[command(subcommand)]
        command: ToolsCommand,
    },
}

#[derive(Debug, Subcommand)]
pub enum AppsCommand {
    List,
    Override {
        app_id: String,
        #[arg(long)]
        version: String,
        #[arg(long)]
        path: PathBuf,
    },
}

#[derive(Debug, Subcommand)]
pub enum ScanCommand {
    Apps {
        #[arg(long = "root")]
        roots: Vec<PathBuf>,
        #[arg(long, default_value = "manifests/apps")]
        manifest_dir: PathBuf,
    },
}

#[derive(Debug, Subcommand)]
pub enum ProjectCommand {
    Import {
        path: PathBuf,
        #[arg(long)]
        name: Option<String>,
    },
    Show {
        path: PathBuf,
    },
    Health {
        path: PathBuf,
    },
    List {
        #[arg(long, default_value = "")]
        query: String,
        #[arg(long, default_value_t = 100)]
        limit: u32,
    },
    Pin {
        path: PathBuf,
        #[arg(long, default_value_t = true)]
        pinned: bool,
    },
    Launch {
        path: PathBuf,
        profile: String,
    },
    Vcs {
        path: PathBuf,
        #[command(subcommand)]
        command: VcsCommand,
    },
}

#[derive(Debug, Subcommand)]
pub enum VcsCommand {
    Status,
    Sync {
        #[arg(long)]
        yes: bool,
    },
    Commit {
        #[arg(long)]
        message: String,
        #[arg(long)]
        yes: bool,
    },
    Push {
        #[arg(long)]
        yes: bool,
    },
    Switch {
        #[arg(long)]
        branch: String,
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Debug, Subcommand)]
pub enum ToolsCommand {
    List {
        source: String,
    },
    Cache {
        source: String,
        #[arg(long)]
        file: PathBuf,
        #[arg(long)]
        etag: Option<String>,
    },
    Verify {
        file: PathBuf,
        #[arg(long)]
        sha256: String,
    },
}

#[derive(Debug, Error)]
pub enum CliExecutionError {
    #[error("{command} requires explicit confirmation with --yes")]
    ConfirmationRequired { command: &'static str },
    #[error("{command} failed: {source}")]
    Application {
        command: &'static str,
        #[source]
        source: ApplicationError,
    },
    #[error("{command} failed: {source}")]
    Project {
        command: &'static str,
        #[source]
        source: ProjectError,
    },
    #[error("{command} contains an invalid version: {source}")]
    Version {
        command: &'static str,
        #[source]
        source: semver::Error,
    },
    #[error("{command} output could not be encoded: {source}")]
    Serialization {
        command: &'static str,
        #[source]
        source: serde_json::Error,
    },
    #[error("{command} failed: {source}")]
    Io {
        command: &'static str,
        #[source]
        source: io::Error,
    },
    #[error("{command} failed: {source}")]
    Artifact {
        command: &'static str,
        #[source]
        source: ArtifactVerificationError,
    },
}

impl CliExecutionError {
    pub fn command(&self) -> &'static str {
        match self {
            Self::ConfirmationRequired { command }
            | Self::Application { command, .. }
            | Self::Project { command, .. }
            | Self::Version { command, .. }
            | Self::Serialization { command, .. }
            | Self::Io { command, .. }
            | Self::Artifact { command, .. } => command,
        }
    }

    pub fn api_message(&self) -> ApiMessage {
        let (code, remediation) = match self {
            Self::ConfirmationRequired { .. } => (
                "CONFIRMATION_REQUIRED",
                Some("Review the operation and retry with --yes.".into()),
            ),
            Self::Version { .. } => (
                "INVALID_VERSION",
                Some("Use a semantic version such as 4.2.3.".into()),
            ),
            Self::Artifact { .. } => (
                "ARTIFACT_VERIFICATION_FAILED",
                Some("Delete the artifact and obtain it again from its reviewed source.".into()),
            ),
            _ => (
                "OPERATION_FAILED",
                Some("Review the error and local configuration, then retry.".into()),
            ),
        };
        ApiMessage {
            code: code.into(),
            message: self.to_string(),
            remediation,
        }
    }
}

pub async fn execute(
    cli: Cli,
    service: &ApplicationService,
) -> Result<CliEnvelope<serde_json::Value>, CliExecutionError> {
    match cli.command {
        Command::Scan {
            command:
                ScanCommand::Apps {
                    roots,
                    manifest_dir,
                },
        } => {
            const COMMAND: &str = "scan.apps";
            let data = service
                .scan_apps(&manifest_dir, &roots)
                .await
                .map_err(|source| CliExecutionError::Application {
                    command: COMMAND,
                    source,
                })?;
            success(COMMAND, data)
        }
        Command::Apps {
            command: AppsCommand::List,
        } => success(
            "apps.list",
            vec![
                "blender",
                "unreal-engine",
                "unity",
                "godot",
                "vscode",
                "rider",
                "git",
                "git-lfs",
                "perforce",
            ],
        ),
        Command::Apps {
            command:
                AppsCommand::Override {
                    app_id,
                    version,
                    path,
                },
        } => {
            const COMMAND: &str = "apps.override";
            let version = version
                .parse()
                .map_err(|source| CliExecutionError::Version {
                    command: COMMAND,
                    source,
                })?;
            service
                .set_manual_override(&app_id, version, &path)
                .await
                .map_err(|source| CliExecutionError::Application {
                    command: COMMAND,
                    source,
                })?;
            success(
                COMMAND,
                serde_json::json!({ "appId": app_id, "path": path }),
            )
        }
        Command::Project {
            command: ProjectCommand::Import { path, name },
        } => {
            const COMMAND: &str = "project.import";
            let data = service
                .import_project(&path, name.as_deref())
                .await
                .map_err(|source| CliExecutionError::Application {
                    command: COMMAND,
                    source,
                })?;
            success(COMMAND, data)
        }
        Command::Project {
            command: ProjectCommand::Show { path },
        } => {
            const COMMAND: &str = "project.show";
            let data = load_project(&path).map_err(|source| CliExecutionError::Project {
                command: COMMAND,
                source,
            })?;
            success(COMMAND, data)
        }
        Command::Project {
            command: ProjectCommand::Health { path },
        } => success("project.health", service.project_health(&path).await),
        Command::Project {
            command: ProjectCommand::List { query, limit },
        } => {
            const COMMAND: &str = "project.list";
            let data = service
                .search_projects(&query, limit)
                .await
                .map_err(|source| CliExecutionError::Application {
                    command: COMMAND,
                    source,
                })?;
            success(COMMAND, data)
        }
        Command::Project {
            command: ProjectCommand::Pin { path, pinned },
        } => {
            const COMMAND: &str = "project.pin";
            service
                .set_project_pinned(&path, pinned)
                .await
                .map_err(|source| CliExecutionError::Application {
                    command: COMMAND,
                    source,
                })?;
            success(
                COMMAND,
                serde_json::json!({ "path": path, "pinned": pinned }),
            )
        }
        Command::Project {
            command: ProjectCommand::Launch { path, profile },
        } => {
            const COMMAND: &str = "project.launch";
            let data = service
                .launch_project_profile(&path, &profile)
                .await
                .map_err(|source| CliExecutionError::Application {
                    command: COMMAND,
                    source,
                })?;
            success(COMMAND, data)
        }
        Command::Project {
            command:
                ProjectCommand::Vcs {
                    path,
                    command: VcsCommand::Status,
                },
        } => {
            const COMMAND: &str = "project.vcs.status";
            let data = service.vcs_status(&path).await.map_err(|source| {
                CliExecutionError::Application {
                    command: COMMAND,
                    source,
                }
            })?;
            success(COMMAND, data)
        }
        Command::Project {
            command:
                ProjectCommand::Vcs {
                    path,
                    command: VcsCommand::Sync { yes },
                },
        } => {
            const COMMAND: &str = "project.vcs.sync";
            require_confirmation(COMMAND, yes)?;
            let data = service.vcs_sync(&path, true).await.map_err(|source| {
                CliExecutionError::Application {
                    command: COMMAND,
                    source,
                }
            })?;
            success(COMMAND, data)
        }
        Command::Project {
            command:
                ProjectCommand::Vcs {
                    path,
                    command: VcsCommand::Commit { message, yes },
                },
        } => {
            const COMMAND: &str = "project.vcs.commit";
            require_confirmation(COMMAND, yes)?;
            let data = service
                .vcs_commit(&path, &message, true)
                .await
                .map_err(|source| CliExecutionError::Application {
                    command: COMMAND,
                    source,
                })?;
            success(COMMAND, data)
        }
        Command::Project {
            command:
                ProjectCommand::Vcs {
                    path,
                    command: VcsCommand::Push { yes },
                },
        } => {
            const COMMAND: &str = "project.vcs.push";
            require_confirmation(COMMAND, yes)?;
            let data = service.vcs_push(&path, true).await.map_err(|source| {
                CliExecutionError::Application {
                    command: COMMAND,
                    source,
                }
            })?;
            success(COMMAND, data)
        }
        Command::Project {
            command:
                ProjectCommand::Vcs {
                    path,
                    command: VcsCommand::Switch { branch, yes },
                },
        } => {
            const COMMAND: &str = "project.vcs.switch";
            require_confirmation(COMMAND, yes)?;
            let data = service
                .vcs_switch_branch(&path, &branch, true)
                .await
                .map_err(|source| CliExecutionError::Application {
                    command: COMMAND,
                    source,
                })?;
            success(COMMAND, data)
        }
        Command::Tools {
            command: ToolsCommand::List { source },
        } => {
            const COMMAND: &str = "tools.list";
            let data = service.cached_tools(&source).await.map_err(|source| {
                CliExecutionError::Application {
                    command: COMMAND,
                    source,
                }
            })?;
            success(COMMAND, data)
        }
        Command::Tools {
            command: ToolsCommand::Cache { source, file, etag },
        } => {
            const COMMAND: &str = "tools.cache";
            let content = fs::read_to_string(file).map_err(|source| CliExecutionError::Io {
                command: COMMAND,
                source,
            })?;
            let data = service
                .cache_tool_index(&source, &content, etag.as_deref())
                .await
                .map_err(|source| CliExecutionError::Application {
                    command: COMMAND,
                    source,
                })?;
            success(COMMAND, data)
        }
        Command::Tools {
            command: ToolsCommand::Verify { file, sha256 },
        } => {
            const COMMAND: &str = "tools.verify";
            vantadeck_security::verify_sha256(&file, &sha256).map_err(|source| {
                CliExecutionError::Artifact {
                    command: COMMAND,
                    source,
                }
            })?;
            success(
                COMMAND,
                serde_json::json!({ "file": file, "verified": true }),
            )
        }
    }
}

fn success<T: serde::Serialize>(
    command: &'static str,
    data: T,
) -> Result<CliEnvelope<serde_json::Value>, CliExecutionError> {
    let value = serde_json::to_value(data)
        .map_err(|source| CliExecutionError::Serialization { command, source })?;
    Ok(CliEnvelope::success(command, value))
}

fn require_confirmation(command: &'static str, confirmed: bool) -> Result<(), CliExecutionError> {
    if confirmed {
        Ok(())
    } else {
        Err(CliExecutionError::ConfirmationRequired { command })
    }
}
