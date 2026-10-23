use clap::Parser;
use serde_json::{Value, json};
use vantadeck::{Cli, execute, mcp};
use vantadeck_application::ApplicationService;
use vantadeck_storage::Storage;
use vantadeck_vcs::GitProvider;

async fn service() -> ApplicationService {
    let storage = Storage::connect("sqlite::memory:").await.expect("storage");
    ApplicationService::new(storage, GitProvider::new("git"))
}

async fn run(service: &ApplicationService, args: &[&str]) -> Value {
    let mut argv = vec!["vantadeck", "--json"];
    argv.extend_from_slice(args);
    let cli = Cli::try_parse_from(argv).expect("parse");
    let envelope = execute(cli, service).await.expect("execute");
    assert!(envelope.success);
    envelope.data.expect("data")
}

#[tokio::test]
async fn issues_lifecycle_and_context_through_the_cli() {
    let root = tempfile::tempdir().expect("root");
    let path = root.path().to_str().expect("utf8");
    let service = service().await;
    run(&service, &["project", "import", path, "--name", "Voidline"]).await;

    let created = run(
        &service,
        &[
            "issues",
            "add",
            path,
            "--title",
            "Hero concept",
            "--kind",
            "art",
            "--priority",
            "high",
        ],
    )
    .await;
    let id = created["id"].as_str().expect("id").to_owned();
    run(&service, &["issues", "move", path, &id, "in progress"]).await;

    let listed = run(
        &service,
        &["issues", "list", path, "--status", "in-progress"],
    )
    .await;
    assert_eq!(listed["issues"].as_array().expect("issues").len(), 1);
    let everywhere = run(&service, &["issues", "list", "--all"]).await;
    assert_eq!(everywhere[0]["projectName"], "Voidline");
    assert_eq!(everywhere[0]["title"], "Hero concept");

    let context = run(&service, &["context", path, "--write"]).await;
    assert!(
        context["markdown"]
            .as_str()
            .expect("md")
            .contains("Hero concept")
    );
    assert!(root.path().join(".pipelineos/context.md").is_file());
}

#[tokio::test]
async fn task_run_requires_yes_and_reports_missing_tasks() {
    let root = tempfile::tempdir().expect("root");
    let path = root.path().to_str().expect("utf8");
    let service = service().await;
    run(&service, &["project", "import", path]).await;
    let cli = Cli::try_parse_from(["vantadeck", "tasks", "run", path, "build"]).expect("parse");
    assert!(execute(cli, &service).await.is_err(), "needs --yes");
    let cli = Cli::try_parse_from([
        "vantadeck",
        "--json",
        "tasks",
        "run",
        path,
        "build",
        "--yes",
    ])
    .expect("parse");
    let error = execute(cli, &service).await.expect_err("no such task");
    assert_eq!(error.api_message().code, "NOT_FOUND");
    let templates = run(&service, &["tasks", "templates"]).await;
    assert!(templates.as_array().expect("templates").len() >= 5);
}

#[cfg(unix)]
#[tokio::test]
async fn runs_a_project_task_end_to_end() {
    let root = tempfile::tempdir().expect("root");
    let path = root.path().to_str().expect("utf8");
    let service = service().await;
    run(&service, &["project", "import", path]).await;
    vantadeck_studio::tasks::save_tasks(
        root.path(),
        &[vantadeck_studio::tasks::TaskDefinition {
            id: "hello".into(),
            name: "Hello".into(),
            kind: Default::default(),
            description: None,
            program: "sh".into(),
            args: vec!["-c".into(), "echo building ${projectName}".into()],
            cwd: None,
            env: Default::default(),
        }],
    )
    .expect("save tasks");
    let data = run(&service, &["tasks", "run", path, "hello", "--yes"]).await;
    assert_eq!(data["outcome"]["status"], "succeeded");
    assert!(
        data["log"][0]
            .as_str()
            .expect("line")
            .starts_with("building ")
    );
}

async fn rpc(service: &ApplicationService, id: u64, method: &str, params: Value) -> Value {
    mcp::handle_message(
        service,
        &json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }),
    )
    .await
    .expect("response")
}

#[tokio::test]
async fn mcp_session_lists_tools_and_drives_issues() {
    let root = tempfile::tempdir().expect("root");
    let path = root.path().to_str().expect("utf8");
    let service = service().await;
    run(&service, &["project", "import", path, "--name", "Helix"]).await;

    let init = rpc(
        &service,
        1,
        "initialize",
        json!({ "protocolVersion": "2025-06-18" }),
    )
    .await;
    assert_eq!(init["result"]["serverInfo"]["name"], "pipeline-os");
    assert!(
        mcp::handle_message(
            &service,
            &json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })
        )
        .await
        .is_none(),
        "notifications get no response"
    );

    let tools = rpc(&service, 2, "tools/list", json!({})).await;
    let names: Vec<_> = tools["result"]["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .map(|t| t["name"].as_str().expect("name").to_owned())
        .collect();
    for expected in [
        "list_projects",
        "project_brief",
        "run_task",
        "create_issue",
        "list_assets",
    ] {
        assert!(names.contains(&expected.to_owned()), "{expected} missing");
    }

    let projects = rpc(
        &service,
        3,
        "tools/call",
        json!({ "name": "list_projects", "arguments": {} }),
    )
    .await;
    assert_eq!(projects["result"]["isError"], false);
    assert_eq!(
        projects["result"]["structuredContent"]["projects"][0]["name"],
        "Helix"
    );

    let created = rpc(
        &service,
        4,
        "tools/call",
        json!({ "name": "create_issue", "arguments": { "path": path, "title": "Fix jump", "kind": "bug" } }),
    )
    .await;
    let id = created["result"]["structuredContent"]["id"]
        .as_str()
        .expect("id")
        .to_owned();
    let updated = rpc(
        &service,
        5,
        "tools/call",
        json!({ "name": "update_issue", "arguments": { "path": path, "id": id, "status": "done" } }),
    )
    .await;
    assert_eq!(updated["result"]["structuredContent"]["status"], "done");

    let brief = rpc(
        &service,
        6,
        "tools/call",
        json!({ "name": "project_brief", "arguments": { "path": path } }),
    )
    .await;
    assert!(
        brief["result"]["content"][0]["text"]
            .as_str()
            .expect("text")
            .contains("# Helix")
    );

    let bad = rpc(
        &service,
        7,
        "tools/call",
        json!({ "name": "nope", "arguments": {} }),
    )
    .await;
    assert_eq!(bad["result"]["isError"], true);
    let unknown = rpc(&service, 8, "resources/list", json!({})).await;
    assert_eq!(unknown["error"]["code"], -32601);
}

#[cfg(unix)]
#[tokio::test]
async fn mcp_only_runs_trusted_tasks() {
    let data = tempfile::tempdir().expect("data dir");
    vantadeck::set_data_dir(data.path().to_path_buf());
    let root = tempfile::tempdir().expect("root");
    let path = root.path().to_str().expect("utf8");
    let service = service().await;
    run(&service, &["project", "import", path]).await;
    let task = |program: &str| vantadeck_studio::tasks::TaskDefinition {
        id: "hello".into(),
        name: "Hello".into(),
        kind: Default::default(),
        description: None,
        program: program.into(),
        args: vec!["-c".into(), "echo hi".into()],
        cwd: None,
        env: Default::default(),
    };
    vantadeck_studio::tasks::save_tasks(root.path(), &[task("sh")]).expect("save tasks");
    let call = json!({ "name": "run_task", "arguments": { "path": path, "task_id": "hello" } });

    let refused = rpc(&service, 1, "tools/call", call.clone()).await;
    assert_eq!(refused["result"]["isError"], true);
    assert!(
        refused["result"]["content"][0]["text"]
            .as_str()
            .expect("text")
            .contains("trust")
    );

    let trusted = run(&service, &["tasks", "trust", path, "--yes"]).await;
    assert!(
        trusted["trusted"][0]["commandLine"]
            .as_str()
            .expect("line")
            .starts_with("sh")
    );
    let ran = rpc(&service, 2, "tools/call", call.clone()).await;
    assert_eq!(ran["result"]["isError"], false, "{ran}");
    assert_eq!(
        ran["result"]["structuredContent"]["outcome"]["status"],
        "succeeded"
    );

    // Any change to the commands revokes trust.
    vantadeck_studio::tasks::save_tasks(root.path(), &[task("bash")]).expect("save tasks");
    let again = rpc(&service, 3, "tools/call", call).await;
    assert_eq!(again["result"]["isError"], true);
}
