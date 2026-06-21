use clap::Parser;
use vantadeck::{Cli, CliExecutionError, execute};
use vantadeck_application::ApplicationService;
use vantadeck_storage::Storage;
use vantadeck_vcs::GitProvider;

#[tokio::test]
async fn imports_project_through_shared_service() {
    let root = tempfile::tempdir().expect("project root");
    let storage = Storage::connect("sqlite::memory:").await.expect("storage");
    let service = ApplicationService::new(storage, GitProvider::new("git"));
    let cli = Cli::try_parse_from([
        "vantadeck",
        "--json",
        "project",
        "import",
        root.path().to_str().expect("UTF-8 path"),
        "--name",
        "Voidline",
    ])
    .expect("import command");

    let envelope = execute(cli, &service).await.expect("execute import");

    assert!(envelope.success);
    assert_eq!(envelope.command, "project.import");
    assert!(root.path().join(".vantadeck/project.toml").is_file());
}

#[tokio::test]
async fn rejects_remote_mutation_without_yes() {
    let storage = Storage::connect("sqlite::memory:").await.expect("storage");
    let service = ApplicationService::new(storage, GitProvider::new("git"));
    let cli = Cli::try_parse_from([
        "vantadeck",
        "project",
        "vcs",
        "D:/Projects/Voidline",
        "sync",
    ])
    .expect("sync command");

    let error = execute(cli, &service)
        .await
        .expect_err("confirmation is required");

    assert!(matches!(
        error,
        CliExecutionError::ConfirmationRequired { .. }
    ));
}
