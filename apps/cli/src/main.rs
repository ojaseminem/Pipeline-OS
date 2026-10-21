use std::{env, path::PathBuf, process::ExitCode};

use clap::Parser;
use directories::BaseDirs;
use vantadeck::{Cli, Command, execute, mcp};
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

    let storage = Storage::connect_path(&database_path).await?;
    Ok(ApplicationService::new(storage, GitProvider::new("git")))
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
