pub mod mcp;

/// The app data folder shared with the desktop app (trust decisions live
/// there). Set once at startup by the binary.
static DATA_DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

pub fn set_data_dir(dir: PathBuf) {
    let _ = DATA_DIR.set(dir);
}

/// Task trust as recorded by the desktop app (or `tasks trust`).
pub fn trust_store() -> Option<tasks::TrustStore> {
    DATA_DIR.get().map(|dir| tasks::TrustStore::load(dir))
}

use std::{
    fs, io,
    path::PathBuf,
    sync::{Arc, atomic::AtomicBool},
};

use clap::{Parser, Subcommand};
use thiserror::Error;
use vantadeck_application::{ApplicationError, ApplicationService};
use vantadeck_domain::{ApiMessage, CliEnvelope};
use vantadeck_projects::{ProjectError, load_project};
use vantadeck_security::ArtifactVerificationError;
use vantadeck_studio::{
    StudioError, assets,
    issues::{self, IssueKind, IssuePriority, IssueStatus, NewIssue},
    organize::{self, FileMove, LayoutScope},
    tasks,
};

#[derive(Debug, Parser)]
#[command(
    name = "vantadeck",
    version,
    about = "Pipeline OS: local-first launcher and pipeline hub for creative projects",
    long_about = "Pipeline OS command line. Every command accepts --json for a stable, \
versioned envelope that scripts and AI agents can parse. Run `vantadeck mcp` to \
serve the same capabilities to MCP clients such as Claude Desktop or Claude Code."
)]
pub struct Cli {
    /// Emit a machine-readable JSON envelope instead of human output.
    #[arg(long, global = true)]
    pub json: bool,
    /// Which installed app's data to use: `stable` (PipelineOS) or `dev`
    /// (PipelineOS Dev). Overridden by VANTADECK_DATABASE_PATH.
    #[arg(long, global = true, default_value = "stable", value_parser = ["stable", "dev"])]
    pub channel: String,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Detect installed creative applications.
    Scan {
        #[command(subcommand)]
        command: ScanCommand,
    },
    /// Detected applications and machine-local version overrides.
    Apps {
        #[command(subcommand)]
        command: AppsCommand,
    },
    /// Import, inspect, launch, and version-control projects.
    Project {
        #[command(subcommand)]
        command: ProjectCommand,
    },
    /// Curated Tools Hub metadata.
    Tools {
        #[command(subcommand)]
        command: ToolsCommand,
    },
    /// Print an AI-ready Markdown brief of a project (engine, layout, tasks, issues).
    Context {
        path: PathBuf,
        /// Also write it to .pipelineos/context.md.
        #[arg(long)]
        write: bool,
    },
    /// Pipeline tasks: builds, cooks, renders, exports, tests.
    Tasks {
        #[command(subcommand)]
        command: TasksCommand,
    },
    /// The project's git-native issue board.
    Issues {
        #[command(subcommand)]
        command: IssuesCommand,
    },
    /// Creative asset library.
    Assets {
        #[command(subcommand)]
        command: AssetsCommand,
    },
    /// Folder layouts, and sorting a messy project's loose files into them.
    Organize {
        #[command(subcommand)]
        command: OrganizeCommand,
    },
    /// Serve Pipeline OS to AI agents over the Model Context Protocol (stdio).
    Mcp,
    /// Find AI apps (Claude, Codex, Cursor, VS Code, ...) and connect them to Pipeline OS.
    Ai {
        #[command(subcommand)]
        command: AiCommand,
    },
    /// Projects on this computer that aren't in Pipeline OS yet.
    Discover,
}

#[derive(Debug, Subcommand)]
pub enum AiCommand {
    /// Which AI apps and agent CLIs are installed and connected.
    Status,
    /// Connect AI apps to the Pipeline OS MCP server.
    Setup {
        /// Client ids (claude-desktop, claude-code, cursor, windsurf, vscode,
        /// codex, gemini-cli). Default: every installed client.
        #[arg(long = "client")]
        clients: Vec<String>,
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Debug, Subcommand)]
pub enum TasksCommand {
    List {
        path: PathBuf,
    },
    /// Built-in task templates for common engines and DCCs.
    Templates,
    /// Show the exact command a task would run, without running it.
    Plan {
        path: PathBuf,
        id: String,
    },
    Run {
        path: PathBuf,
        id: String,
        #[arg(long)]
        yes: bool,
    },
    /// Review the project's task commands and trust them on this machine, so
    /// AI agents (MCP) may run them. Prints every command line.
    Trust {
        path: PathBuf,
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Debug, Subcommand)]
pub enum IssuesCommand {
    List {
        /// Project path; omit with --all to list open issues across projects.
        path: Option<PathBuf>,
        #[arg(long)]
        status: Option<String>,
        #[arg(long)]
        all: bool,
    },
    Show {
        path: PathBuf,
        id: String,
    },
    Add {
        path: PathBuf,
        #[arg(long)]
        title: String,
        #[arg(long, default_value = "")]
        body: String,
        #[arg(long, value_parser = ["task", "bug", "feature", "art", "design"])]
        kind: Option<String>,
        #[arg(long, value_parser = ["none", "low", "medium", "high", "urgent"])]
        priority: Option<String>,
        #[arg(long)]
        assignee: Option<String>,
        #[arg(long = "label")]
        labels: Vec<String>,
    },
    /// Pull issues from the project's connected tracker into the board.
    Sync {
        path: PathBuf,
    },
    /// Move an issue to backlog, todo, in-progress, review, or done.
    Move {
        path: PathBuf,
        id: String,
        status: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum AssetsCommand {
    List {
        path: PathBuf,
        #[arg(long, default_value_t = 200)]
        limit: usize,
    },
    Layouts,
    ApplyLayout {
        path: PathBuf,
        layout: String,
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Debug, Subcommand)]
pub enum OrganizeCommand {
    /// Layouts available to a project: its own, this machine's, and built-in.
    Layouts {
        /// Project folder (adds the project's own layouts).
        path: Option<PathBuf>,
    },
    /// Print a layout as JSON, ready to edit or share.
    Show {
        layout: String,
        #[arg(long)]
        project: Option<PathBuf>,
    },
    /// Add a layout from a .json or .txt file to this machine's library, or to
    /// a project with --project.
    Import {
        file: PathBuf,
        #[arg(long)]
        project: Option<PathBuf>,
    },
    /// Preview which loose files a layout would move. Changes nothing.
    Plan { path: PathBuf, layout: String },
    /// Create the layout's folders and move every file the plan lists.
    Apply {
        path: PathBuf,
        layout: String,
        /// Only create the folders; move nothing.
        #[arg(long)]
        folders_only: bool,
        #[arg(long)]
        yes: bool,
    },
    /// Put the files from the last organize back where they were.
    Undo {
        path: PathBuf,
        #[arg(long)]
        yes: bool,
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
    Studio {
        command: &'static str,
        #[source]
        source: StudioError,
    },
    #[error("{command}: {message}")]
    Usage {
        command: &'static str,
        message: String,
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
            | Self::Studio { command, .. }
            | Self::Usage { command, .. }
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
            Self::Usage { .. } => (
                "INVALID_ARGUMENTS",
                Some("Run the command with --help for usage.".into()),
            ),
            Self::Studio {
                source: StudioError::NotFound { .. },
                ..
            }
            | Self::Application {
                source: ApplicationError::Studio(StudioError::NotFound { .. }),
                ..
            } => ("NOT_FOUND", Some("Check the id; list items first.".into())),
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
    let json = cli.json;
    match cli.command {
        Command::Mcp => Err(CliExecutionError::Usage {
            command: "mcp",
            message: "the MCP server runs from the binary entry point, not execute()".into(),
        }),
        Command::Context { path, write } => {
            const COMMAND: &str = "context";
            let brief = service.project_brief(&path).await.map_err(app(COMMAND))?;
            let written = if write {
                Some(
                    vantadeck_studio::context::write_brief(&path, &brief)
                        .map_err(studio(COMMAND))?,
                )
            } else {
                None
            };
            if !json {
                println!("{brief}");
            }
            success(
                COMMAND,
                serde_json::json!({ "markdown": brief, "writtenTo": written }),
            )
        }
        Command::Tasks { command } => execute_tasks(command, service, json).await,
        Command::Ai { command } => execute_ai(command),
        Command::Discover => {
            const COMMAND: &str = "discover";
            let known: Vec<PathBuf> = service
                .registered_projects()
                .await
                .map_err(app(COMMAND))?
                .into_iter()
                .map(|project| project.root)
                .collect();
            let found = tokio::task::spawn_blocking(move || {
                vantadeck_studio::discovery::discover(
                    &vantadeck_studio::discovery::DiscoverySources::system(),
                    &known,
                )
            })
            .await
            .map_err(|e| CliExecutionError::Usage {
                command: COMMAND,
                message: e.to_string(),
            })?;
            success(COMMAND, found)
        }
        Command::Issues { command } => execute_issues(command, service).await,
        Command::Assets { command } => execute_assets(command),
        Command::Organize { command } => execute_organize(command),
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
        } => {
            const COMMAND: &str = "apps.list";
            let mut apps = Vec::new();
            for id in KNOWN_APP_IDS {
                let installations = service.detected_installations(id).await.map_err(|source| {
                    CliExecutionError::Application {
                        command: COMMAND,
                        source,
                    }
                })?;
                apps.push(serde_json::json!({ "id": id, "installations": installations }));
            }
            success(COMMAND, apps)
        }
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

/// App ids whose installations `apps list` reports (bundled manifests).
pub const KNOWN_APP_IDS: &[&str] = &[
    "unity",
    "unreal-engine",
    "godot",
    "blender",
    "maya",
    "houdini",
    "substance-painter",
    "zbrush",
    "vscode",
    "rider",
    "git",
    "git-lfs",
    "perforce",
];

fn app(command: &'static str) -> impl Fn(ApplicationError) -> CliExecutionError {
    move |source| CliExecutionError::Application { command, source }
}

fn studio(command: &'static str) -> impl Fn(StudioError) -> CliExecutionError {
    move |source| CliExecutionError::Studio { command, source }
}

async fn execute_tasks(
    command: TasksCommand,
    service: &ApplicationService,
    json: bool,
) -> Result<CliEnvelope<serde_json::Value>, CliExecutionError> {
    match command {
        TasksCommand::List { path } => {
            const COMMAND: &str = "tasks.list";
            success(COMMAND, tasks::load_tasks(&path).map_err(studio(COMMAND))?)
        }
        TasksCommand::Templates => success("tasks.templates", tasks::task_templates()),
        TasksCommand::Plan { path, id } => {
            const COMMAND: &str = "tasks.plan";
            let (task, resolved) = service
                .resolve_project_task(&path, &id)
                .await
                .map_err(app(COMMAND))?;
            success(
                COMMAND,
                serde_json::json!({ "task": task, "resolved": resolved, "commandLine": resolved.display() }),
            )
        }
        TasksCommand::Trust { path, yes } => {
            const COMMAND: &str = "tasks.trust";
            let definitions = tasks::load_tasks(&path).map_err(studio(COMMAND))?;
            let mut plans = Vec::new();
            for task in &definitions {
                let line = match service.resolve_project_task(&path, &task.id).await {
                    Ok((_, resolved)) => resolved.display(),
                    Err(_) => std::iter::once(task.program.clone())
                        .chain(task.args.clone())
                        .collect::<Vec<_>>()
                        .join(" "),
                };
                if !json {
                    println!("{}: {line}", task.id);
                }
                plans.push(serde_json::json!({ "id": task.id, "commandLine": line }));
            }
            require_confirmation(COMMAND, yes)?;
            let fingerprint =
                tasks::tasks_fingerprint(&path).ok_or_else(|| CliExecutionError::Usage {
                    command: COMMAND,
                    message: "this project has no .pipelineos/tasks.toml".into(),
                })?;
            let mut store = trust_store().ok_or_else(|| CliExecutionError::Usage {
                command: COMMAND,
                message: "the app data folder is unavailable".into(),
            })?;
            store.trust(&path, &fingerprint).map_err(studio(COMMAND))?;
            success(COMMAND, serde_json::json!({ "trusted": plans }))
        }
        TasksCommand::Run { path, id, yes } => {
            const COMMAND: &str = "tasks.run";
            require_confirmation(COMMAND, yes)?;
            let (task, resolved) = service
                .resolve_project_task(&path, &id)
                .await
                .map_err(app(COMMAND))?;
            // `--yes` confirms this run; it doesn't vouch for commands someone
            // else committed. Checked after resolving, so a file swapped
            // mid-resolve is caught too.
            if !trust_store().is_some_and(|store| store.is_trusted(&path)) {
                return Err(CliExecutionError::Usage {
                    command: COMMAND,
                    message: format!(
                        "this project's tasks haven't been trusted on this machine; review them, then run `vantadeck tasks trust \"{}\" --yes`",
                        path.display()
                    ),
                });
            }
            let outcome = run_resolved_task(resolved, !json)
                .await
                .map_err(studio(COMMAND))?;
            success(
                COMMAND,
                serde_json::json!({ "task": task.id, "outcome": outcome.0, "log": outcome.1 }),
            )
        }
    }
}

/// Runs a task on a blocking thread. When `echo` is set each line is printed
/// as it arrives; the last 200 lines are always returned.
pub async fn run_resolved_task(
    resolved: tasks::ResolvedTask,
    echo: bool,
) -> Result<(tasks::RunOutcome, Vec<String>), StudioError> {
    tokio::task::spawn_blocking(move || {
        let mut tail = tasks::LogTail::new(200);
        let outcome = tasks::run_task(
            &resolved,
            Arc::new(AtomicBool::new(false)),
            |stream, line| {
                if echo {
                    match stream {
                        tasks::OutputStream::Stdout => println!("{line}"),
                        tasks::OutputStream::Stderr => eprintln!("{line}"),
                    }
                }
                tail.push(line);
            },
        )?;
        Ok((outcome, tail.lines.into_iter().collect()))
    })
    .await
    .map_err(|e| StudioError::Invalid(format!("task thread failed: {e}")))?
}

fn execute_ai(command: AiCommand) -> Result<CliEnvelope<serde_json::Value>, CliExecutionError> {
    use vantadeck_studio::ai_clients::{self, ClientLocations, ServerCommand};
    let locations = ClientLocations::system().ok_or(CliExecutionError::Usage {
        command: "ai",
        message: "couldn't find the home folder".into(),
    })?;
    match command {
        AiCommand::Status => success(
            "ai.status",
            serde_json::json!({
                "clients": ai_clients::detect_clients(&locations),
                "agentClis": ai_clients::installed_agent_clis(&locations),
            }),
        ),
        AiCommand::Setup { clients, yes } => {
            const COMMAND: &str = "ai.setup";
            require_confirmation(COMMAND, yes)?;
            let exe = std::env::current_exe().map_err(|source| CliExecutionError::Io {
                command: COMMAND,
                source,
            })?;
            let mut args = vec!["mcp".to_owned()];
            if DATA_DIR
                .get()
                .is_some_and(|dir| dir.to_string_lossy().ends_with(".dev"))
            {
                args.insert(0, "--channel=dev".to_owned());
            }
            let server = ServerCommand {
                command: exe.display().to_string(),
                args,
            };
            let targets: Vec<String> = if clients.is_empty() {
                ai_clients::detect_clients(&locations)
                    .into_iter()
                    .filter(|client| client.installed)
                    .map(|client| client.id)
                    .collect()
            } else {
                clients
            };
            let results: Vec<_> = targets
                .into_iter()
                .map(
                    |id| match ai_clients::configure_client(&locations, &id, &server) {
                        Ok(()) => serde_json::json!({ "id": id, "ok": true }),
                        Err(error) => {
                            serde_json::json!({ "id": id, "ok": false, "error": error.to_string() })
                        }
                    },
                )
                .collect();
            success(COMMAND, results)
        }
    }
}

async fn execute_issues(
    command: IssuesCommand,
    service: &ApplicationService,
) -> Result<CliEnvelope<serde_json::Value>, CliExecutionError> {
    match command {
        IssuesCommand::List { path, status, all } => {
            const COMMAND: &str = "issues.list";
            let status = parse_status(COMMAND, status.as_deref())?;
            if all || path.is_none() {
                let issues = service
                    .open_issues_everywhere()
                    .await
                    .map_err(app(COMMAND))?;
                let issues: Vec<_> = issues
                    .into_iter()
                    .filter(|i| status.is_none_or(|s| i.issue.status == s))
                    .collect();
                return success(COMMAND, issues);
            }
            let path = path.expect("checked above");
            let (issues, problems) = issues::list_issues(&path).map_err(studio(COMMAND))?;
            let issues: Vec<_> = issues
                .into_iter()
                .filter(|i| status.is_none_or(|s| i.status == s))
                .collect();
            success(
                COMMAND,
                serde_json::json!({ "issues": issues, "problems": problems }),
            )
        }
        IssuesCommand::Show { path, id } => {
            const COMMAND: &str = "issues.show";
            success(
                COMMAND,
                issues::get_issue(&path, &id).map_err(studio(COMMAND))?,
            )
        }
        IssuesCommand::Add {
            path,
            title,
            body,
            kind,
            priority,
            assignee,
            labels,
        } => {
            const COMMAND: &str = "issues.add";
            let new = NewIssue {
                title,
                body,
                kind: kind
                    .as_deref()
                    .map(parse_enum::<IssueKind>)
                    .transpose()
                    .map_err(usage(COMMAND))?,
                priority: priority
                    .as_deref()
                    .map(parse_enum::<IssuePriority>)
                    .transpose()
                    .map_err(usage(COMMAND))?,
                assignee,
                labels,
                ..Default::default()
            };
            success(
                COMMAND,
                issues::create_issue(&path, new).map_err(studio(COMMAND))?,
            )
        }
        IssuesCommand::Sync { path } => {
            const COMMAND: &str = "issues.sync";
            success(
                COMMAND,
                vantadeck_studio::trackers::sync_tracker(&path)
                    .await
                    .map_err(studio(COMMAND))?,
            )
        }
        IssuesCommand::Move { path, id, status } => {
            const COMMAND: &str = "issues.move";
            let status = parse_status(COMMAND, Some(&status))?.expect("status given");
            success(
                COMMAND,
                issues::set_issue_status(&path, &id, status).map_err(studio(COMMAND))?,
            )
        }
    }
}

fn execute_assets(
    command: AssetsCommand,
) -> Result<CliEnvelope<serde_json::Value>, CliExecutionError> {
    match command {
        AssetsCommand::List { path, limit } => {
            const COMMAND: &str = "assets.list";
            success(
                COMMAND,
                assets::index_assets(&path, limit).map_err(studio(COMMAND))?,
            )
        }
        AssetsCommand::Layouts => {
            const COMMAND: &str = "assets.layouts";
            success(
                COMMAND,
                organize::list_layouts(None, DATA_DIR.get().map(PathBuf::as_path))
                    .map_err(studio(COMMAND))?,
            )
        }
        AssetsCommand::ApplyLayout { path, layout, yes } => {
            const COMMAND: &str = "assets.apply-layout";
            require_confirmation(COMMAND, yes)?;
            let layout =
                organize::find_layout(Some(&path), DATA_DIR.get().map(PathBuf::as_path), &layout)
                    .map_err(studio(COMMAND))?;
            success(
                COMMAND,
                organize::create_folders(&path, &layout).map_err(studio(COMMAND))?,
            )
        }
    }
}

fn execute_organize(
    command: OrganizeCommand,
) -> Result<CliEnvelope<serde_json::Value>, CliExecutionError> {
    let user_dir = DATA_DIR.get().map(PathBuf::as_path);
    let need_user_dir = |command: &'static str| {
        user_dir.ok_or(CliExecutionError::Usage {
            command,
            message: "this machine's Pipeline OS data folder isn't available".into(),
        })
    };
    match command {
        OrganizeCommand::Layouts { path } => {
            const COMMAND: &str = "organize.layouts";
            success(
                COMMAND,
                organize::list_layouts(path.as_deref(), user_dir).map_err(studio(COMMAND))?,
            )
        }
        OrganizeCommand::Show { layout, project } => {
            const COMMAND: &str = "organize.show";
            success(
                COMMAND,
                organize::find_layout(project.as_deref(), user_dir, &layout)
                    .map_err(studio(COMMAND))?,
            )
        }
        OrganizeCommand::Import { file, project } => {
            const COMMAND: &str = "organize.import";
            let text = fs::read_to_string(&file).map_err(|source| CliExecutionError::Io {
                command: COMMAND,
                source,
            })?;
            let layout = organize::parse_layout(&text).map_err(studio(COMMAND))?;
            let scope = if project.is_some() {
                LayoutScope::Project
            } else {
                LayoutScope::User
            };
            let saved = organize::save_layout(project.as_deref(), user_dir, scope, &layout)
                .map_err(studio(COMMAND))?;
            success(
                COMMAND,
                serde_json::json!({ "layout": layout, "file": saved }),
            )
        }
        OrganizeCommand::Plan { path, layout } => {
            const COMMAND: &str = "organize.plan";
            let layout =
                organize::find_layout(Some(&path), user_dir, &layout).map_err(studio(COMMAND))?;
            success(
                COMMAND,
                organize::plan_organize(&path, &layout).map_err(studio(COMMAND))?,
            )
        }
        OrganizeCommand::Apply {
            path,
            layout,
            folders_only,
            yes,
        } => {
            const COMMAND: &str = "organize.apply";
            require_confirmation(COMMAND, yes)?;
            let layout =
                organize::find_layout(Some(&path), user_dir, &layout).map_err(studio(COMMAND))?;
            if folders_only {
                return success(
                    COMMAND,
                    organize::create_folders(&path, &layout).map_err(studio(COMMAND))?,
                );
            }
            let plan = organize::plan_organize(&path, &layout).map_err(studio(COMMAND))?;
            let moves: Vec<FileMove> = plan
                .moves
                .into_iter()
                .map(|planned| FileMove {
                    from: planned.from,
                    to: planned.to,
                })
                .collect();
            success(
                COMMAND,
                organize::apply_organize(&path, need_user_dir(COMMAND)?, &layout, &moves)
                    .map_err(studio(COMMAND))?,
            )
        }
        OrganizeCommand::Undo { path, yes } => {
            const COMMAND: &str = "organize.undo";
            require_confirmation(COMMAND, yes)?;
            success(
                COMMAND,
                organize::undo_organize(&path, need_user_dir(COMMAND)?).map_err(studio(COMMAND))?,
            )
        }
    }
}

fn usage(command: &'static str) -> impl Fn(String) -> CliExecutionError {
    move |message| CliExecutionError::Usage { command, message }
}

fn parse_enum<T: serde::de::DeserializeOwned>(value: &str) -> Result<T, String> {
    serde_json::from_value(serde_json::Value::String(value.to_owned()))
        .map_err(|_| format!("unsupported value {value:?}"))
}

fn parse_status(
    command: &'static str,
    value: Option<&str>,
) -> Result<Option<IssueStatus>, CliExecutionError> {
    value
        .map(|value| {
            IssueStatus::parse(value).ok_or_else(|| CliExecutionError::Usage {
                command,
                message: format!(
                    "unknown status {value:?}; use backlog, todo, in-progress, review, or done"
                ),
            })
        })
        .transpose()
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
