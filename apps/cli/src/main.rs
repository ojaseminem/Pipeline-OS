use std::{env, path::PathBuf, process::ExitCode};

use clap::Parser;
use directories::{BaseDirs, ProjectDirs};
use vantadeck::{Cli, Command, execute, mcp, set_data_dir};
use vantadeck_application::ApplicationService;
use vantadeck_domain::CliEnvelope;
use vantadeck_storage::Storage;
use vantadeck_vcs::GitProvider;

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    let json = cli.json;
    let service = match open_service(&cli.channel).await {
        Ok(service) => service,
        Err(error) => {
            eprintln!("Vantadeck storage initialization failed: {error}");
            return ExitCode::from(1);
        }
    };
    if matches!(cli.command, Command::Mcp) {
        return match mcp::serve(&service).await {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("MCP server stopped: {error}");
                ExitCode::from(1)
            }
        };
    }
    match execute(cli, &service).await {
        Ok(envelope) => {
            print_envelope(json, &envelope);
            ExitCode::SUCCESS
        }
        Err(error) => {
            let envelope =
                CliEnvelope::<serde_json::Value>::failure(error.command(), error.api_message());
            print_envelope(json, &envelope);
            ExitCode::from(2)
        }
    }
}

/// Opens the same database the desktop app uses, so the CLI and MCP server
/// see the user's registered projects and detected apps. Tauri stores it in
/// the platform data dir under the app identifier (dev builds use their own).
async fn open_service(channel: &str) -> Result<ApplicationService, Box<dyn std::error::Error>> {
    let database_path = if let Some(path) = env::var_os("VANTADECK_DATABASE_PATH") {
        PathBuf::from(path)
    } else {
        let identifier = if channel == "dev" {
            "com.vantadeck.launcher.dev"
        } else {
            "com.vantadeck.launcher"
        };
        let base = BaseDirs::new().ok_or("operating-system data directory is unavailable")?;
        base.data_dir().join(identifier).join("vantadeck.sqlite3")
    };
    if let Some(dir) = database_path.parent() {
        set_data_dir(dir.to_path_buf());
        if channel != "dev" && env::var_os("VANTADECK_DATABASE_PATH").is_none() {
            adopt_legacy_database(&database_path, dir)?;
        }
    }

    let storage = Storage::connect_path(&database_path).await?;
    Ok(ApplicationService::new(storage, GitProvider::new("git")))
}

/// 0.4.x CLIs kept their own database. When the shared one doesn't exist yet,
/// start it from a copy of the old one so CLI-only registrations survive; the
/// old file is left in place.
/// A failed copy is an error: carrying on would create an empty shared
/// database and the old registrations would never be adopted.
fn adopt_legacy_database(
    database_path: &std::path::Path,
    dir: &std::path::Path,
) -> std::io::Result<()> {
    if database_path.exists() {
        return Ok(());
    }
    let Some(legacy) = ProjectDirs::from("org", "Vantadeck", "Vantadeck")
        .map(|dirs| dirs.data_local_dir().join("vantadeck.db"))
    else {
        return Ok(());
    };
    if legacy.is_file() {
        std::fs::create_dir_all(dir)?;
        // Copy beside the target, then rename, so an interrupted copy never
        // leaves a partial database that stops the next attempt.
        let staging = database_path.with_extension("sqlite3.migrating");
        let copied =
            std::fs::copy(&legacy, &staging).and_then(|_| std::fs::rename(&staging, database_path));
        if let Err(error) = copied {
            let _ = std::fs::remove_file(&staging);
            return Err(error);
        }
    }
    Ok(())
}

fn print_envelope(json: bool, envelope: &CliEnvelope<serde_json::Value>) {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(envelope).expect("CLI envelope serialization")
        );
        return;
    }
    if envelope.success {
        print_human_value(
            envelope.data.as_ref().unwrap_or(&serde_json::Value::Null),
            0,
        );
        return;
    }
    for error in &envelope.errors {
        eprintln!("[{}] {}", error.code, error.message);
        if let Some(remediation) = &error.remediation {
            eprintln!("  Next: {remediation}");
        }
    }
}

fn print_human_value(value: &serde_json::Value, indent: usize) {
    let padding = " ".repeat(indent);
    match value {
        serde_json::Value::Object(fields) => {
            for (key, value) in fields {
                if value.is_array() || value.is_object() {
                    println!("{padding}{key}:");
                    print_human_value(value, indent + 2);
                } else {
                    println!("{padding}{key}: {}", human_scalar(value));
                }
            }
        }
        serde_json::Value::Array(items) => {
            if items.is_empty() {
                println!("{padding}(none)");
            }
            for item in items {
                if item.is_array() || item.is_object() {
                    println!("{padding}-");
                    print_human_value(item, indent + 2);
                } else {
                    println!("{padding}- {}", human_scalar(item));
                }
            }
        }
        scalar => println!("{padding}{}", human_scalar(scalar)),
    }
}

fn human_scalar(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => "none".into(),
        serde_json::Value::String(value) => value.clone(),
        other => other.to_string(),
    }
}
