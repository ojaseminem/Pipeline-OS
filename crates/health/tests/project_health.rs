use vantadeck_health::{DiskSpaceCheck, HealthCheck, ProjectPathCheck};

#[tokio::test]
async fn reports_missing_project_path() {
    let root = tempfile::tempdir().expect("temp root");
    let missing = root.path().join("missing-project");
    let issues = ProjectPathCheck.run(&missing).await;

    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].code, "PROJECT_PATH_MISSING");
}

#[tokio::test]
async fn disk_space_check_is_silent_on_a_normal_dev_machine() {
    // A real temp directory on a CI/dev machine has far more than the low-
    // space threshold free; this is mostly a smoke test that the syscall
    // path doesn't panic or false-positive under normal conditions.
    let root = tempfile::tempdir().expect("temp root");
    let issues = DiskSpaceCheck.run(root.path()).await;
    assert!(
        issues.is_empty(),
        "expected no disk space issues, got {issues:?}"
    );
}
