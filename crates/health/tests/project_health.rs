use vantadeck_health::{HealthCheck, ProjectPathCheck};

#[tokio::test]
async fn reports_missing_project_path() {
    let root = tempfile::tempdir().expect("temp root");
    let missing = root.path().join("missing-project");
    let issues = ProjectPathCheck.run(&missing).await;

    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].code, "PROJECT_PATH_MISSING");
}
