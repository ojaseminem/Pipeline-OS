use std::{fs, process::Command};

use vantadeck_vcs::{GitProvider, LfsProbe, evaluate_lfs_health, evaluate_repo_size_health};

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

#[test]
fn flags_large_git_history_but_not_a_normal_sized_one() {
    assert!(evaluate_repo_size_health(50 * 1024 * 1024).is_empty());
    let issues = evaluate_repo_size_health(3 * 1024 * 1024 * 1024);
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].code, "REPO_HISTORY_LARGE");
}

#[test]
fn does_not_nag_about_lfs_when_the_project_has_no_large_files() {
    // A project with no LFS config and no large binaries doesn't need LFS at
    // all (e.g. a code-only repo). Mirrors GitHub Desktop: LFS is surfaced
    // based on large-file detection, not unconditionally on every repo.
    let issues = evaluate_lfs_health(&LfsProbe {
        installed: false,
        initialized: false,
        missing_objects: false,
        large_untracked_files: vec![],
    });
    assert!(
        issues.is_empty(),
        "expected no LFS health issues, got {issues:?}"
    );
}

#[test]
fn still_flags_missing_lfs_install_when_the_project_already_opted_in() {
    // `.gitattributes` already declares LFS patterns, so large binaries may
    // already be checked out as small pointer files (not caught by the
    // large-file scan) — Git LFS not being installed is still actionable.
    let issues = evaluate_lfs_health(&LfsProbe {
        installed: false,
        initialized: true,
        missing_objects: false,
        large_untracked_files: vec![],
    });
    let codes = issues
        .iter()
        .map(|issue| issue.code.as_str())
        .collect::<Vec<_>>();
    assert!(codes.contains(&"GIT_LFS_NOT_INSTALLED"));
    assert!(!codes.contains(&"GIT_LFS_NOT_INITIALIZED"));
}
