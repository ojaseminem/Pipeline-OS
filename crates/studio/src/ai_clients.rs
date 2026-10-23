//! Detects AI apps and agent CLIs on this machine and connects them to the
//! Pipeline OS MCP server in one step (Claude Desktop, Claude Code, Cursor,
//! Windsurf, VS Code, Codex, Gemini CLI).
//!
//! Edits are careful with files people maintain by hand: existing entries and
//! key order are preserved, a `.pipelineos-backup` copy is written first, and
//! files that can't be parsed (e.g. JSON with comments) are left untouched
//! with an explanation instead of being rewritten.

use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::StudioError;

/// Name the server is registered under in every client.
pub const SERVER_NAME: &str = "pipeline-os";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Format {
    /// `{ "mcpServers": { name: { command, args } } }`
    McpServers,
    /// `~/.claude.json`: same, with `"type": "stdio"`.
    ClaudeCode,
    /// VS Code `mcp.json`: `{ "servers": { name: { type, command, args } } }`
    VsCode,
    /// Codex `config.toml`: `[mcp_servers.name] command = …, args = […]`
    CodexToml,
}

#[derive(Debug, Clone)]
struct ClientSpec {
    id: &'static str,
    name: &'static str,
    /// Presence of this path means the client is installed.
    detect: PathBuf,
    config: PathBuf,
    format: Format,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AiClientStatus {
    pub id: String,
    pub name: String,
    pub installed: bool,
    pub configured: bool,
    pub config_path: String,
    /// Why the client couldn't be read or configured, if so.
    pub problem: Option<String>,
}

/// The command an MCP client should run to reach Pipeline OS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerCommand {
    pub command: String,
    pub args: Vec<String>,
}

/// Per-OS locations, rooted at `home` (and `%APPDATA%` on Windows).
#[derive(Debug, Clone)]
pub struct ClientLocations {
    pub home: PathBuf,
    /// Windows %APPDATA%, macOS ~/Library/Application Support, Linux ~/.config.
    pub app_config: PathBuf,
}

impl ClientLocations {
    pub fn system() -> Option<Self> {
        let home = PathBuf::from(std::env::var_os(if cfg!(windows) {
            "USERPROFILE"
        } else {
            "HOME"
        })?);
        let app_config = if cfg!(windows) {
            std::env::var_os("APPDATA")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join("AppData/Roaming"))
        } else if cfg!(target_os = "macos") {
            home.join("Library/Application Support")
        } else {
            std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".config"))
        };
        Some(Self { home, app_config })
    }

    fn clients(&self) -> Vec<ClientSpec> {
        let home = &self.home;
        let cfg = &self.app_config;
        vec![
            ClientSpec {
                id: "claude-desktop",
                name: "Claude Desktop",
                detect: cfg.join("Claude"),
                config: cfg.join("Claude/claude_desktop_config.json"),
                format: Format::McpServers,
            },
            ClientSpec {
                id: "claude-code",
                name: "Claude Code",
                detect: home.join(".claude"),
                config: home.join(".claude.json"),
                format: Format::ClaudeCode,
            },
            ClientSpec {
                id: "cursor",
                name: "Cursor",
                detect: home.join(".cursor"),
                config: home.join(".cursor/mcp.json"),
                format: Format::McpServers,
            },
            ClientSpec {
                id: "windsurf",
                name: "Windsurf",
                detect: home.join(".codeium/windsurf"),
                config: home.join(".codeium/windsurf/mcp_config.json"),
                format: Format::McpServers,
            },
            ClientSpec {
                id: "vscode",
                name: "VS Code (Copilot)",
                detect: cfg.join("Code/User"),
                config: cfg.join("Code/User/mcp.json"),
                format: Format::VsCode,
            },
            ClientSpec {
                id: "codex",
                name: "Codex CLI",
                detect: home.join(".codex"),
                config: home.join(".codex/config.toml"),
                format: Format::CodexToml,
            },
            ClientSpec {
                id: "gemini-cli",
                name: "Gemini CLI",
                detect: home.join(".gemini"),
                config: home.join(".gemini/settings.json"),
                format: Format::McpServers,
            },
        ]
    }
}

fn read_json(path: &Path) -> Result<Option<Value>, String> {
    match std::fs::read_to_string(path) {
        Ok(raw) if raw.trim().is_empty() => Ok(Some(json!({}))),
        Ok(raw) => serde_json::from_str(&raw).map(Some).map_err(|e| {
            format!(
                "{} isn't plain JSON ({e}); add Pipeline OS to it by hand",
                path.display()
            )
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("couldn't read {}: {e}", path.display())),
    }
}

fn servers_key(format: Format) -> &'static str {
    if format == Format::VsCode {
        "servers"
    } else {
        "mcpServers"
    }
}

fn is_configured(spec: &ClientSpec) -> Result<bool, String> {
    if spec.format == Format::CodexToml {
        return Ok(match std::fs::read_to_string(&spec.config) {
            Ok(raw) => raw
                .parse::<toml_edit::DocumentMut>()
                .map_err(|e| format!("{} isn't valid TOML ({e})", spec.config.display()))?
                .get("mcp_servers")
                .and_then(|servers| servers.get(SERVER_NAME))
                .is_some(),
            Err(_) => false,
        });
    }
    Ok(read_json(&spec.config)?
        .and_then(|value| {
            value
                .get(servers_key(spec.format))?
                .get(SERVER_NAME)
                .cloned()
        })
        .is_some())
}

/// Every supported client with whether it's installed and already connected.
pub fn detect_clients(locations: &ClientLocations) -> Vec<AiClientStatus> {
    locations
        .clients()
        .into_iter()
        .map(|spec| {
            let installed = spec.detect.exists();
            let (configured, problem) = match is_configured(&spec) {
                Ok(configured) => (configured, None),
                Err(problem) => (false, Some(problem)),
            };
            AiClientStatus {
                id: spec.id.into(),
                name: spec.name.into(),
                installed,
                configured,
                config_path: spec.config.display().to_string(),
                problem,
            }
        })
        .collect()
}

fn backup(path: &Path) {
    if path.is_file() {
        let _ = std::fs::copy(
            path,
            path.with_extension(format!(
                "{}pipelineos-backup",
                path.extension()
                    .map(|e| format!("{}.", e.to_string_lossy()))
                    .unwrap_or_default()
            )),
        );
    }
}

/// Adds (or updates) the Pipeline OS server in one client's configuration.
pub fn configure_client(
    locations: &ClientLocations,
    id: &str,
    server: &ServerCommand,
) -> Result<(), StudioError> {
    let spec = locations
        .clients()
        .into_iter()
        .find(|spec| spec.id == id)
        .ok_or_else(|| StudioError::NotFound {
            kind: "AI client",
            id: id.to_owned(),
        })?;
    if let Some(parent) = spec.config.parent() {
        std::fs::create_dir_all(parent).map_err(|e| StudioError::io(parent, e))?;
    }
    if spec.format == Format::CodexToml {
        let raw = std::fs::read_to_string(&spec.config).unwrap_or_default();
        let mut doc = raw.parse::<toml_edit::DocumentMut>().map_err(|e| {
            StudioError::Invalid(format!("{} isn't valid TOML ({e})", spec.config.display()))
        })?;
        if !doc.contains_table("mcp_servers") {
            let mut table = toml_edit::Table::new();
            table.set_implicit(true);
            doc["mcp_servers"] = toml_edit::Item::Table(table);
        }
        let mut entry = toml_edit::Table::new();
        entry["command"] = toml_edit::value(server.command.clone());
        let mut args = toml_edit::Array::new();
        for arg in &server.args {
            args.push(arg.as_str());
        }
        entry["args"] = toml_edit::value(args);
        doc["mcp_servers"][SERVER_NAME] = toml_edit::Item::Table(entry);
        backup(&spec.config);
        return std::fs::write(&spec.config, doc.to_string())
            .map_err(|e| StudioError::io(&spec.config, e));
    }
    let mut root = read_json(&spec.config)
        .map_err(StudioError::Invalid)?
        .unwrap_or_else(|| json!({}));
    let Some(object) = root.as_object_mut() else {
        return Err(StudioError::Invalid(format!(
            "{} isn't a JSON object",
            spec.config.display()
        )));
    };
    let entry = match spec.format {
        Format::McpServers => json!({ "command": server.command, "args": server.args }),
        Format::ClaudeCode => {
            json!({ "type": "stdio", "command": server.command, "args": server.args, "env": {} })
        }
        Format::VsCode => {
            json!({ "type": "stdio", "command": server.command, "args": server.args })
        }
        Format::CodexToml => unreachable!(),
    };
    let servers = object
        .entry(servers_key(spec.format))
        .or_insert_with(|| Value::Object(Map::new()));
    let Some(servers) = servers.as_object_mut() else {
        return Err(StudioError::Invalid(format!(
            "{} has an unexpected `{}` value",
            spec.config.display(),
            servers_key(spec.format)
        )));
    };
    servers.insert(SERVER_NAME.into(), entry);
    backup(&spec.config);
    let text =
        serde_json::to_string_pretty(&root).map_err(|e| StudioError::Invalid(e.to_string()))?;
    std::fs::write(&spec.config, format!("{text}\n")).map_err(|e| StudioError::io(&spec.config, e))
}

/// Removes the Pipeline OS server from one client's configuration.
pub fn unconfigure_client(locations: &ClientLocations, id: &str) -> Result<(), StudioError> {
    let spec = locations
        .clients()
        .into_iter()
        .find(|spec| spec.id == id)
        .ok_or_else(|| StudioError::NotFound {
            kind: "AI client",
            id: id.to_owned(),
        })?;
    if spec.format == Format::CodexToml {
        let Ok(raw) = std::fs::read_to_string(&spec.config) else {
            return Ok(());
        };
        let mut doc = raw
            .parse::<toml_edit::DocumentMut>()
            .map_err(|e| StudioError::Invalid(e.to_string()))?;
        if let Some(servers) = doc
            .get_mut("mcp_servers")
            .and_then(toml_edit::Item::as_table_like_mut)
        {
            servers.remove(SERVER_NAME);
        }
        backup(&spec.config);
        return std::fs::write(&spec.config, doc.to_string())
            .map_err(|e| StudioError::io(&spec.config, e));
    }
    let Some(mut root) = read_json(&spec.config).map_err(StudioError::Invalid)? else {
        return Ok(());
    };
    if let Some(servers) = root
        .get_mut(servers_key(spec.format))
        .and_then(Value::as_object_mut)
    {
        servers.shift_remove(SERVER_NAME);
    }
    backup(&spec.config);
    let text =
        serde_json::to_string_pretty(&root).map_err(|e| StudioError::Invalid(e.to_string()))?;
    std::fs::write(&spec.config, format!("{text}\n")).map_err(|e| StudioError::io(&spec.config, e))
}

/// Agent CLIs Pipeline OS can launch in a project terminal.
pub const AGENT_CLIS: &[(&str, &str)] = &[
    ("claude", "Claude Code"),
    ("codex", "Codex CLI"),
    ("gemini", "Gemini CLI"),
    ("aider", "Aider"),
];

/// Which agent CLIs are installed (on PATH or in their usual install dirs).
pub fn installed_agent_clis(locations: &ClientLocations) -> Vec<String> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect())
        .unwrap_or_default();
    for extra in [
        ".local/bin",
        ".claude/local",
        ".npm-global/bin",
        ".bun/bin",
        ".cargo/bin",
        "AppData/Roaming/npm",
    ] {
        dirs.push(locations.home.join(extra));
    }
    dirs.extend(["/opt/homebrew/bin", "/usr/local/bin"].map(PathBuf::from));
    AGENT_CLIS
        .iter()
        .filter(|(binary, _)| {
            dirs.iter().any(|dir| {
                ["", ".exe", ".cmd"]
                    .iter()
                    .any(|ext| dir.join(format!("{binary}{ext}")).is_file())
            })
        })
        .map(|(binary, _)| (*binary).to_owned())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn locations() -> (tempfile::TempDir, ClientLocations) {
        let dir = tempfile::tempdir().unwrap();
        let locations = ClientLocations {
            home: dir.path().join("home"),
            app_config: dir.path().join("config"),
        };
        std::fs::create_dir_all(&locations.home).unwrap();
        (dir, locations)
    }

    fn server() -> ServerCommand {
        ServerCommand {
            command: "/Applications/PipelineOS.app/Contents/MacOS/vantadeck".into(),
            args: vec!["mcp".into()],
        }
    }

    fn status<'a>(all: &'a [AiClientStatus], id: &str) -> &'a AiClientStatus {
        all.iter().find(|c| c.id == id).unwrap()
    }

    #[test]
    fn detects_installed_clients_and_configures_json_without_losing_entries() {
        let (_dir, loc) = locations();
        let config = loc.app_config.join("Claude/claude_desktop_config.json");
        std::fs::create_dir_all(config.parent().unwrap()).unwrap();
        std::fs::write(
            &config,
            r#"{"theme":"dark","mcpServers":{"other":{"command":"x"}}}"#,
        )
        .unwrap();

        let before = detect_clients(&loc);
        assert!(status(&before, "claude-desktop").installed);
        assert!(!status(&before, "claude-desktop").configured);
        assert!(!status(&before, "cursor").installed);

        configure_client(&loc, "claude-desktop", &server()).unwrap();
        let written: Value =
            serde_json::from_str(&std::fs::read_to_string(&config).unwrap()).unwrap();
        assert_eq!(written["theme"], "dark");
        assert_eq!(written["mcpServers"]["other"]["command"], "x");
        assert_eq!(written["mcpServers"]["pipeline-os"]["args"][0], "mcp");
        assert!(
            config.with_extension("json.pipelineos-backup").is_file(),
            "backup written"
        );
        assert!(status(&detect_clients(&loc), "claude-desktop").configured);

        unconfigure_client(&loc, "claude-desktop").unwrap();
        let after: Value =
            serde_json::from_str(&std::fs::read_to_string(&config).unwrap()).unwrap();
        assert!(after["mcpServers"].get("pipeline-os").is_none());
        assert_eq!(after["mcpServers"]["other"]["command"], "x");
    }

    #[test]
    fn writes_each_client_format() {
        let (_dir, loc) = locations();
        for id in ["claude-code", "vscode", "codex", "cursor"] {
            configure_client(&loc, id, &server()).unwrap();
        }
        let claude: Value =
            serde_json::from_str(&std::fs::read_to_string(loc.home.join(".claude.json")).unwrap())
                .unwrap();
        assert_eq!(claude["mcpServers"]["pipeline-os"]["type"], "stdio");
        let vscode: Value = serde_json::from_str(
            &std::fs::read_to_string(loc.app_config.join("Code/User/mcp.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            vscode["servers"]["pipeline-os"]["command"],
            server().command
        );
        let codex = std::fs::read_to_string(loc.home.join(".codex/config.toml")).unwrap();
        assert!(codex.contains("[mcp_servers.pipeline-os]"), "{codex}");
        assert!(codex.contains("args = [\"mcp\"]"), "{codex}");
        let statuses = detect_clients(&loc);
        for id in ["claude-code", "vscode", "codex", "cursor"] {
            assert!(status(&statuses, id).configured, "{id}");
        }
    }

    #[test]
    fn codex_toml_edits_keep_comments_and_other_servers() {
        let (_dir, loc) = locations();
        let path = loc.home.join(".codex/config.toml");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            "# my settings\nmodel = \"o4\"\n\n[mcp_servers.other]\ncommand = \"x\"\n",
        )
        .unwrap();
        configure_client(&loc, "codex", &server()).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.starts_with("# my settings"), "{text}");
        assert!(
            text.contains("[mcp_servers.other]") && text.contains("[mcp_servers.pipeline-os]"),
            "{text}"
        );
        unconfigure_client(&loc, "codex").unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(
            !text.contains("pipeline-os") && text.contains("[mcp_servers.other]"),
            "{text}"
        );
    }

    #[test]
    fn refuses_to_rewrite_files_it_cannot_parse() {
        let (_dir, loc) = locations();
        let path = loc.app_config.join("Code/User/mcp.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let jsonc = "{\n  // my servers\n  \"servers\": {}\n}\n";
        std::fs::write(&path, jsonc).unwrap();
        assert!(configure_client(&loc, "vscode", &server()).is_err());
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            jsonc,
            "file untouched"
        );
        assert!(status(&detect_clients(&loc), "vscode").problem.is_some());
    }
}
