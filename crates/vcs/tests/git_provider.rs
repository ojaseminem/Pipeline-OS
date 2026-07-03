use std::{fs, process::Command};

use vantadeck_vcs::{
    ConflictResolution, GitProvider, LfsProbe, MergeOutcome, evaluate_lfs_health,
    evaluate_repo_size_health,
};

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

fn current_branch(root: &std::path::Path) -> String {
    let output = Command::new("git")
        .current_dir(root)
        .args(["branch", "--show-current"])
        .output()
        .expect("git command");
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
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

#[tokio::test]
async fn merges_a_branch_cleanly_when_there_is_no_conflict() {
    let root = repository();
    let provider = GitProvider::new("git");
    let base = current_branch(root.path());
    git(root.path(), &["switch", "-c", "feature"]);
    fs::write(root.path().join("feature.txt"), "feature\n").expect("feature file");
    git(root.path(), &["add", "feature.txt"]);
    git(root.path(), &["commit", "-m", "add feature file"]);
    git(root.path(), &["switch", &base]);

    let outcome = provider
        .merge_branch(root.path(), "feature")
        .await
        .expect("merge succeeds");

    assert!(matches!(outcome, MergeOutcome::Merged { .. }));
    assert!(root.path().join("feature.txt").is_file());
    assert!(!provider.merge_status(root.path()).await.in_progress);
}

#[tokio::test]
async fn merge_conflicts_are_reported_and_resolvable() {
    let root = repository();
    let provider = GitProvider::new("git");
    let base = current_branch(root.path());
    git(root.path(), &["switch", "-c", "feature"]);
    fs::write(root.path().join("tracked.txt"), "feature change\n").expect("feature edit");
    git(root.path(), &["commit", "-am", "feature edit"]);
    git(root.path(), &["switch", &base]);
    fs::write(root.path().join("tracked.txt"), "main change\n").expect("main edit");
    git(root.path(), &["commit", "-am", "main edit"]);

    let outcome = provider
        .merge_branch(root.path(), "feature")
        .await
        .expect("merge runs");
    let files = match outcome {
        MergeOutcome::Conflicts { files } => files,
        MergeOutcome::Merged { .. } => panic!("expected a conflict"),
    };
    assert_eq!(files, vec!["tracked.txt".to_string()]);

    let status = provider.merge_status(root.path()).await;
    assert!(status.in_progress);
    assert_eq!(status.conflicted_files, vec!["tracked.txt".to_string()]);

    provider
        .resolve_conflict(root.path(), "tracked.txt", ConflictResolution::Ours)
        .await
        .expect("resolve conflict");
    assert!(
        provider
            .merge_status(root.path())
            .await
            .conflicted_files
            .is_empty()
    );

    provider
        .continue_merge(root.path())
        .await
        .expect("commit the merge");
    assert!(!provider.merge_status(root.path()).await.in_progress);
}

#[tokio::test]
async fn abort_merge_restores_pre_merge_state() {
    let root = repository();
    let provider = GitProvider::new("git");
    let base = current_branch(root.path());
    git(root.path(), &["switch", "-c", "feature"]);
    fs::write(root.path().join("tracked.txt"), "feature change\n").expect("feature edit");
    git(root.path(), &["commit", "-am", "feature edit"]);
    git(root.path(), &["switch", &base]);
    fs::write(root.path().join("tracked.txt"), "main change\n").expect("main edit");
    git(root.path(), &["commit", "-am", "main edit"]);

    provider
        .merge_branch(root.path(), "feature")
        .await
        .expect("merge runs");
    assert!(provider.merge_status(root.path()).await.in_progress);

    provider
        .abort_merge(root.path())
        .await
        .expect("abort merge");

    assert!(!provider.merge_status(root.path()).await.in_progress);
    let content = fs::read_to_string(root.path().join("tracked.txt")).expect("tracked file");
    // Compare with normalized line endings — Windows git (core.autocrlf) may
    // check the file out with CRLF regardless of what we wrote.
    assert_eq!(content.replace("\r\n", "\n"), "main change\n");
}
