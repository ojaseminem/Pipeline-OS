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

#[tokio::test]
async fn lists_remote_branches_without_a_local_counterpart() {
    let remote_dir = tempfile::tempdir().expect("bare remote dir");
    git(remote_dir.path(), &["init", "--bare"]);

    let root = repository();
    let provider = GitProvider::new("git");
    git(
        root.path(),
        &[
            "remote",
            "add",
            "origin",
            &remote_dir.path().display().to_string(),
        ],
    );
    let base = current_branch(root.path());
    git(root.path(), &["push", "origin", &base]);
    git(root.path(), &["switch", "-c", "feature"]);
    fs::write(root.path().join("feature.txt"), "feature\n").expect("feature file");
    git(root.path(), &["add", "feature.txt"]);
    git(root.path(), &["commit", "-m", "add feature file"]);
    git(root.path(), &["push", "origin", "feature"]);
    git(root.path(), &["switch", &base]);
    git(root.path(), &["branch", "-D", "feature"]);

    let branches = provider.branches(root.path()).await.expect("list branches");
    let local: Vec<&str> = branches
        .iter()
        .filter(|b| b.remote.is_none())
        .map(|b| b.name.as_str())
        .collect();
    let remote: Vec<_> = branches.iter().filter(|b| b.remote.is_some()).collect();

    assert!(local.contains(&base.as_str()));
    assert!(!local.contains(&"feature"));
    assert_eq!(remote.len(), 1);
    assert_eq!(remote[0].name, "feature");
    assert_eq!(remote[0].remote.as_deref(), Some("origin"));
}

#[tokio::test]
async fn merges_a_remote_only_branch_by_short_name() {
    // Regression: `git merge feature` fails with "not something we can
    // merge" when only `origin/feature` exists — unlike switch/checkout,
    // merge has no DWIM fallback. merge_branch must resolve it itself.
    let remote_dir = tempfile::tempdir().expect("bare remote dir");
    git(remote_dir.path(), &["init", "--bare"]);

    let root = repository();
    let provider = GitProvider::new("git");
    git(
        root.path(),
        &[
            "remote",
            "add",
            "origin",
            &remote_dir.path().display().to_string(),
        ],
    );
    let base = current_branch(root.path());
    git(root.path(), &["push", "origin", &base]);
    git(root.path(), &["switch", "-c", "feature"]);
    fs::write(root.path().join("feature.txt"), "feature\n").expect("feature file");
    git(root.path(), &["add", "feature.txt"]);
    git(root.path(), &["commit", "-m", "add feature file"]);
    git(root.path(), &["push", "origin", "feature"]);
    git(root.path(), &["switch", &base]);
    git(root.path(), &["branch", "-D", "feature"]);

    let outcome = provider
        .merge_branch(root.path(), "feature")
        .await
        .expect("merge resolves the remote-only branch");

    assert!(matches!(outcome, MergeOutcome::Merged { .. }));
    assert!(root.path().join("feature.txt").is_file());
}

#[tokio::test]
async fn compares_ahead_behind_against_another_branch() {
    let root = repository();
    let provider = GitProvider::new("git");
    let base = current_branch(root.path());
    git(root.path(), &["switch", "-c", "feature"]);
    fs::write(root.path().join("feature.txt"), "feature\n").expect("feature file");
    git(root.path(), &["add", "feature.txt"]);
    git(root.path(), &["commit", "-m", "add feature file"]);
    git(root.path(), &["switch", &base]);
    fs::write(root.path().join("tracked.txt"), "main change\n").expect("main edit");
    git(root.path(), &["commit", "-am", "main edit"]);

    let comparison = provider
        .compare_branch(root.path(), "feature")
        .await
        .expect("compare branches");

    assert_eq!(comparison.ahead, 1);
    assert_eq!(comparison.behind, 1);
}

#[tokio::test]
async fn status_reports_no_upstream_until_the_branch_is_published() {
    let remote_dir = tempfile::tempdir().expect("bare remote dir");
    git(remote_dir.path(), &["init", "--bare"]);

    let root = repository();
    let provider = GitProvider::new("git");
    let base = current_branch(root.path());
    let status = provider.status(root.path()).await.expect("status");
    assert!(!status.has_upstream);
    assert!(status.last_fetched_at.is_none());

    git(
        root.path(),
        &[
            "remote",
            "add",
            "origin",
            &remote_dir.path().display().to_string(),
        ],
    );
    provider
        .publish_branch(root.path(), &base)
        .await
        .expect("publish branch");

    let status = provider.status(root.path()).await.expect("status");
    assert!(status.has_upstream);
}

#[tokio::test]
async fn fetch_updates_last_fetched_at() {
    let remote_dir = tempfile::tempdir().expect("bare remote dir");
    git(remote_dir.path(), &["init", "--bare"]);

    let root = repository();
    let provider = GitProvider::new("git");
    let base = current_branch(root.path());
    git(
        root.path(),
        &[
            "remote",
            "add",
            "origin",
            &remote_dir.path().display().to_string(),
        ],
    );
    provider
        .publish_branch(root.path(), &base)
        .await
        .expect("publish branch");

    provider.fetch(root.path()).await.expect("fetch");

    let status = provider.status(root.path()).await.expect("status");
    assert!(status.last_fetched_at.is_some());
}

#[tokio::test]
async fn pull_merges_cleanly_when_fast_forwardable() {
    let remote_dir = tempfile::tempdir().expect("bare remote dir");
    git(remote_dir.path(), &["init", "--bare"]);

    let root = repository();
    let provider = GitProvider::new("git");
    let base = current_branch(root.path());
    git(
        root.path(),
        &[
            "remote",
            "add",
            "origin",
            &remote_dir.path().display().to_string(),
        ],
    );
    provider
        .publish_branch(root.path(), &base)
        .await
        .expect("publish branch");

    // A second clone pushes a new commit that our first checkout doesn't have yet.
    let other_root = tempfile::tempdir().expect("second clone dir");
    git(
        other_root.path(),
        &["clone", &remote_dir.path().display().to_string(), "."],
    );
    git(
        other_root.path(),
        &["config", "user.name", "Vantadeck Test"],
    );
    git(
        other_root.path(),
        &["config", "user.email", "vantadeck@example.invalid"],
    );
    fs::write(other_root.path().join("other.txt"), "from elsewhere\n").expect("other file");
    git(other_root.path(), &["add", "other.txt"]);
    git(other_root.path(), &["commit", "-m", "add other file"]);
    git(other_root.path(), &["push", "origin", &base]);

    let outcome = provider.pull(root.path()).await.expect("pull succeeds");
    assert!(matches!(outcome, MergeOutcome::Merged { .. }));
    assert!(root.path().join("other.txt").is_file());
}

#[tokio::test]
async fn stash_pop_targets_a_specific_entry_by_ref() {
    let root = repository();
    let provider = GitProvider::new("git");

    fs::write(root.path().join("tracked.txt"), "first change\n").expect("first edit");
    provider
        .stash_push(root.path(), "first")
        .await
        .expect("stash first");
    fs::write(root.path().join("tracked.txt"), "second change\n").expect("second edit");
    provider
        .stash_push(root.path(), "second")
        .await
        .expect("stash second");

    let entries = provider.stash_list(root.path()).await.expect("stash list");
    assert_eq!(entries.len(), 2);
    // Newest first: stash@{0} is "second", stash@{1} is "first".
    let oldest_ref = entries[1].split('\u{1f}').next().expect("stash ref field");

    provider
        .stash_pop(root.path(), Some(oldest_ref))
        .await
        .expect("pop the older stash by ref");

    let content = fs::read_to_string(root.path().join("tracked.txt")).expect("tracked contents");
    // Compare with normalized line endings — Windows git (core.autocrlf) may
    // check the file out with CRLF regardless of what we wrote.
    assert_eq!(content.replace("\r\n", "\n"), "first change\n");
    let remaining = provider.stash_list(root.path()).await.expect("stash list");
    assert_eq!(remaining.len(), 1);
}

#[tokio::test]
async fn stash_drop_deletes_without_applying() {
    let root = repository();
    let provider = GitProvider::new("git");

    fs::write(root.path().join("tracked.txt"), "throwaway\n").expect("edit");
    provider
        .stash_push(root.path(), "throwaway")
        .await
        .expect("stash");
    let entries = provider.stash_list(root.path()).await.expect("stash list");
    assert_eq!(entries.len(), 1);
    let stash_ref = entries[0].split('\u{1f}').next().expect("stash ref field");

    provider
        .stash_drop(root.path(), stash_ref)
        .await
        .expect("drop stash");

    let content = fs::read_to_string(root.path().join("tracked.txt")).expect("tracked contents");
    assert_eq!(content.replace("\r\n", "\n"), "initial\n");
    let remaining = provider.stash_list(root.path()).await.expect("stash list");
    assert!(remaining.is_empty());
}
