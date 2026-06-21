use clap::Parser;
use vantadeck::{AppsCommand, Cli, Command, ProjectCommand, ScanCommand, ToolsCommand, VcsCommand};

#[test]
fn parses_json_project_health_command() {
    let cli = Cli::try_parse_from([
        "vantadeck",
        "--json",
        "project",
        "health",
        "D:/Projects/Voidline",
    ])
    .expect("valid command");

    assert!(cli.json);
    assert!(matches!(
        cli.command,
        Command::Project {
            command: ProjectCommand::Health { .. }
        }
    ));
}

#[test]
fn parses_project_management_and_tools_hub_commands() {
    let pin = Cli::try_parse_from(["vantadeck", "project", "pin", "D:/Projects/Voidline"])
        .expect("pin command");
    assert!(matches!(
        pin.command,
        Command::Project {
            command: ProjectCommand::Pin { pinned: true, .. }
        }
    ));

    let launch = Cli::try_parse_from([
        "vantadeck",
        "project",
        "launch",
        "D:/Projects/Voidline",
        "editor",
    ])
    .expect("launch command");
    assert!(matches!(
        launch.command,
        Command::Project {
            command: ProjectCommand::Launch { .. }
        }
    ));

    let tools = Cli::try_parse_from([
        "vantadeck",
        "tools",
        "cache",
        "https://tools.vantadeck.org/v1/index.json",
        "--file",
        "index.json",
    ])
    .expect("tools cache command");
    assert!(matches!(
        tools.command,
        Command::Tools {
            command: ToolsCommand::Cache { .. }
        }
    ));
}

#[test]
fn branch_switch_requires_an_explicit_confirmation_flag() {
    let switch = Cli::try_parse_from([
        "vantadeck",
        "project",
        "vcs",
        "D:/Projects/Voidline",
        "switch",
        "--branch",
        "develop",
        "--yes",
    ])
    .expect("switch command");
    assert!(matches!(
        switch.command,
        Command::Project {
            command: ProjectCommand::Vcs {
                command: VcsCommand::Switch { yes: true, .. },
                ..
            }
        }
    ));
}

#[test]
fn parses_app_scan_and_manual_override_commands() {
    let scan = Cli::try_parse_from(["vantadeck", "scan", "apps", "--root", "D:/Creative Apps"])
        .expect("scan command");
    assert!(matches!(
        scan.command,
        Command::Scan {
            command: ScanCommand::Apps { .. }
        }
    ));

    let override_command = Cli::try_parse_from([
        "vantadeck",
        "apps",
        "override",
        "blender",
        "--version",
        "4.2.3",
        "--path",
        "D:/Portable/Blender/blender.exe",
    ])
    .expect("override command");
    assert!(matches!(
        override_command.command,
        Command::Apps {
            command: AppsCommand::Override { .. }
        }
    ));
}

#[test]
fn parses_project_import_and_confirmation_gated_vcs_commands() {
    let import = Cli::try_parse_from([
        "vantadeck",
        "project",
        "import",
        "D:/Projects/Voidline",
        "--name",
        "Voidline",
    ])
    .expect("import command");
    assert!(matches!(
        import.command,
        Command::Project {
            command: ProjectCommand::Import { .. }
        }
    ));

    let sync = Cli::try_parse_from([
        "vantadeck",
        "project",
        "vcs",
        "D:/Projects/Voidline",
        "sync",
        "--yes",
    ])
    .expect("sync command");
    assert!(matches!(
        sync.command,
        Command::Project {
            command: ProjectCommand::Vcs {
                command: VcsCommand::Sync { yes: true },
                ..
            }
        }
    ));
}
