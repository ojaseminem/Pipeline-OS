use std::{fs, process::Command};

use vantadeck_vcs::{GitProvider, LfsProbe, evaluate_lfs_health};

fn git(root: &std::path::Path, arguments: &[&str]) {
    let output = Command::new("git")
        .current_dir(root)
        .args(arguments)
        .output()
        .expect("git command");
    assert!(
        output.status.success(),
        "git {:?}: {}",
        arguments,
        String::from_utf8_lossy(&output.stderr)
    );
}

fn repository() -> tempfile::TempDir {
    let root = tempfile::tempdir().expect("repository root");
    git(root.path(), &["init"]);
    git(root.path(), &["config", "user.name", "Vantadeck Test"]);
    git(
        root.path(),
        &["config", "user.email", "vantadeck@example.invalid"],
    );
    fs::write(root.path().join("tracked.txt"), "initial\n").expect("tracked file");
    git(root.path(), &["add", "tracked.txt"]);
    git(root.path(), &["commit", "-m", "initial"]);
    root
}

#[tokio::test]
async fn reads_status_from_real_repository() {
    let root = repository();
    fs::write(root.path().join("tracked.txt"), "changed\n").expect("modified file");
    fs::write(root.path().join("new.txt"), "new\n").expect("untracked file");

    let status = GitProvider::new("git")
        .status(root.path())
        .await
        .expect("git status");

    assert!(status.branch.is_some());
    assert_eq!(status.changed_files.len(), 2);
}

#[tokio::test]
async fn commit_all_records_worktree_changes() {
    let root = repository();
    fs::write(root.path().join("new.txt"), "new\n").expect("untracked file");
    let provider = GitProvider::new("git");

    provider
        .commit_all(root.path(), "Add new file")
        .await
        .expect("commit succeeds");

    assert!(
        provider
            .status(root.path())
            .await
            .expect("status")
            .changed_files
            .is_empty()
    );
}

#[test]
fn evaluates_lfs_probe_into_actionable_health_codes() {
    let issues = evaluate_lfs_health(&LfsProbe {
        installed: false,
        initialized: false,
        missing_objects: true,
        large_untracked_files: vec!["Art/hero.psd".into()],
    });
    let codes = issues
        .iter()
        .map(|issue| issue.code.as_str())
        .collect::<Vec<_>>();

    assert!(codes.contains(&"GIT_LFS_NOT_INSTALLED"));
    assert!(codes.contains(&"GIT_LFS_NOT_INITIALIZED"));
    assert!(codes.contains(&"GIT_LFS_MISSING_OBJECTS"));
    assert!(codes.contains(&"LARGE_FILE_NOT_TRACKED"));
}
