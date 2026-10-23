//! Integration tests for pre-commit checks, branch safety, history
//! operations, and network retry against real temporary repositories. Each
//! test skips itself when `git` isn't installed.

use std::{fs, path::Path, process::Command};

use vantadeck_vcs::{FixKind, GitProvider, MergeOutcome, VcsError};

fn git_available() -> bool {
    Command::new("git")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
}

fn git(root: &Path, arguments: &[&str]) -> String {
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
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

fn configure(root: &Path) {
    git(root, &["config", "user.name", "Pipeline Test"]);
    git(root, &["config", "user.email", "pipeline@example.invalid"]);
    git(root, &["config", "commit.gpgsign", "false"]);
}

fn repository() -> tempfile::TempDir {
    let root = tempfile::tempdir().expect("repository root");
    git(root.path(), &["init", "-b", "main"]);
    configure(root.path());
    fs::write(root.path().join("tracked.txt"), "initial\n").expect("tracked file");
    git(root.path(), &["add", "tracked.txt"]);
    git(root.path(), &["commit", "-m", "initial"]);
    root
}

fn commit_file(root: &Path, name: &str, content: &str, message: &str) {
    fs::write(root.join(name), content).expect("write file");
    git(root, &["add", name]);
    git(root, &["commit", "-m", message]);
}

/// A clone of a bare remote, so pushed/unpushed and origin/HEAD are real.
fn clone_with_remote() -> (tempfile::TempDir, tempfile::TempDir) {
    let remote = tempfile::tempdir().expect("remote");
    git(remote.path(), &["init", "--bare", "-b", "main"]);
    let seed = repository();
    let url = remote.path().display().to_string();
    git(seed.path(), &["remote", "add", "origin", &url]);
    git(seed.path(), &["push", "-u", "origin", "main"]);
    let work = tempfile::tempdir().expect("work");
    git(work.path(), &["clone", &url, "."]);
    configure(work.path());
    (remote, work)
}

#[tokio::test]
async fn pre_commit_checks_flag_unity_mistakes_and_fixes_apply() {
    if !git_available() {
        return;
    }
    let root = repository();
    let path = root.path();
    fs::create_dir_all(path.join("Assets")).unwrap();
    fs::create_dir_all(path.join("ProjectSettings")).unwrap();
    fs::create_dir_all(path.join("Library/ShaderCache")).unwrap();
    fs::write(path.join("Library/ShaderCache/a.bin"), "cache").unwrap();
    fs::write(path.join("Assets/Hero.prefab"), "%YAML 1.1\n").unwrap();
    fs::write(path.join("Assets/Orphan.mat.meta"), "guid: 1\n").unwrap();
    fs::write(path.join(".env"), "DB_PASSWORD=hunter2hunter2\n").unwrap();
    fs::write(
        path.join("merge.cs"),
        "a\n<<<<<<< HEAD\nb\n=======\nc\n>>>>>>> other\n",
    )
    .unwrap();
    // Windows can't create this name at all, which is what the check warns about.
    if !cfg!(windows) {
        fs::write(path.join("what?.txt"), "x").unwrap();
    }
    let big = fs::File::create(path.join("Assets/Huge.bin")).unwrap();
    big.set_len(60 * 1024 * 1024).unwrap();
    fs::write(path.join("Assets/Huge.bin.meta"), "guid: 2\n").unwrap();

    let provider = GitProvider::new("git");
    let findings = provider.pre_commit_checks(path, None).await.unwrap();
    let ids: Vec<&str> = findings.iter().map(|f| f.id.as_str()).collect();
    for expected in [
        "generated-folders",
        "missing-gitignore",
        "unity-missing-meta",
        "unity-orphan-meta",
        "secrets",
        "conflict-markers",
        "windows-invalid-name",
        "large-file",
    ] {
        if cfg!(windows) && expected == "windows-invalid-name" {
            continue;
        }
        assert!(ids.contains(&expected), "missing {expected} in {ids:?}");
    }
    let serialized = serde_json_free(&findings);
    assert!(
        !serialized.contains("hunter2hunter2"),
        "secret value leaked"
    );
    let secrets = findings.iter().find(|f| f.id == "secrets").unwrap();
    assert_eq!(
        secrets.locations,
        vec![".env:1 (password/secret in .env)".to_owned()]
    );

    // Selective checks only look at the given paths.
    let only = provider
        .pre_commit_checks(path, Some(&["merge.cs".to_owned()]))
        .await
        .unwrap();
    assert!(only.iter().any(|f| f.id == "conflict-markers"));
    assert!(!only.iter().any(|f| f.id == "secrets"));

    // Fix: add the Unity .gitignore template; Library/ disappears from changes.
    let gitignore_fix = findings
        .iter()
        .find(|f| f.id == "missing-gitignore")
        .and_then(|f| f.fix.clone())
        .unwrap();
    provider.apply_fix(path, &gitignore_fix).await.unwrap();
    let after = provider.pre_commit_checks(path, None).await.unwrap();
    assert!(!after.iter().any(|f| f.id == "missing-gitignore"));
    assert!(!after.iter().any(|f| f.id == "generated-folders"));

    // Fix: untrack a generated folder that was already committed.
    git(path, &["add", "-f", "Library"]);
    let untrack = FixKind::UntrackPaths {
        paths: vec!["Library".into()],
        gitignore: vec!["/[Ll]ibrary/".into()],
    };
    provider.apply_fix(path, &untrack).await.unwrap();
    assert!(git(path, &["ls-files", "--", "Library"]).is_empty());
    assert!(path.join("Library/ShaderCache/a.bin").exists());

    // Unsafe fix paths are refused.
    let escape = FixKind::UntrackPaths {
        paths: vec!["../outside".into()],
        gitignore: Vec::new(),
    };
    assert!(provider.apply_fix(path, &escape).await.is_err());
}

/// Cheap debug serialization (the vcs crate has no serde_json dependency).
fn serde_json_free<T: std::fmt::Debug>(value: &T) -> String {
    format!("{value:?}")
}

#[tokio::test]
async fn lfs_fix_tracks_patterns() {
    if !git_available()
        || !Command::new("git")
            .args(["lfs", "version"])
            .output()
            .is_ok_and(|o| o.status.success())
    {
        return;
    }
    let root = repository();
    let fix = FixKind::TrackWithLfs {
        patterns: vec!["*.fbx".into()],
        paths: Vec::new(),
    };
    GitProvider::new("git")
        .apply_fix(root.path(), &fix)
        .await
        .unwrap();
    let attributes = fs::read_to_string(root.path().join(".gitattributes")).unwrap();
    assert!(attributes.contains("*.fbx filter=lfs"));
}

#[tokio::test]
async fn delete_branch_safety_rules() {
    if !git_available() {
        return;
    }
    let (_remote, work) = clone_with_remote();
    let path = work.path();
    let provider = GitProvider::new("git");
    assert_eq!(provider.default_branch(path).await.as_deref(), Some("main"));

    // Current branch and default branch are always refused.
    let refused = provider
        .delete_branch(path, "main", false, true, true)
        .await;
    assert!(matches!(refused, Err(VcsError::Refused(_))));

    // Unmerged, unpushed work is refused without force.
    git(path, &["switch", "-c", "feature"]);
    commit_file(path, "feature.txt", "f\n", "feature work");
    git(path, &["switch", "main"]);
    let refused = provider
        .delete_branch(path, "feature", false, false, false)
        .await;
    assert!(
        matches!(&refused, Err(VcsError::Refused(message)) if message.contains("aren't pushed")),
        "{refused:?}"
    );
    provider
        .delete_branch(path, "feature", false, true, false)
        .await
        .expect("force delete");

    // A fully merged branch deletes without force.
    git(path, &["branch", "merged"]);
    provider
        .delete_branch(path, "merged", false, false, false)
        .await
        .expect("delete merged");

    // Remote deletion needs the permission check to pass.
    git(path, &["switch", "-c", "shared"]);
    git(path, &["push", "-u", "origin", "shared"]);
    git(path, &["switch", "main"]);
    let refused = provider
        .delete_branch(path, "shared", true, false, false)
        .await;
    assert!(matches!(refused, Err(VcsError::Refused(_))));
    provider
        .delete_branch(path, "shared", true, false, true)
        .await
        .expect("delete local + remote");
    assert!(git(path, &["ls-remote", "--heads", "origin", "shared"]).is_empty());
}

#[tokio::test]
async fn rewriting_pushed_commits_is_refused() {
    if !git_available() {
        return;
    }
    let (_remote, work) = clone_with_remote();
    let path = work.path();
    let provider = GitProvider::new("git");
    assert!(provider.head_is_pushed(path).await);
    assert!(matches!(
        provider.amend_last_commit_message(path, "new").await,
        Err(VcsError::Refused(_))
    ));
    assert!(matches!(
        provider.undo_last_commit(path).await,
        Err(VcsError::Refused(_))
    ));

    commit_file(path, "local.txt", "l\n", "local only");
    provider
        .amend_last_commit_message(path, "better message")
        .await
        .unwrap();
    assert_eq!(git(path, &["log", "-1", "--format=%s"]), "better message");
    provider.undo_last_commit(path).await.unwrap();
    assert_eq!(git(path, &["log", "-1", "--format=%s"]), "initial");
    assert!(git(path, &["diff", "--cached", "--name-only"]).contains("local.txt"));
}

#[tokio::test]
async fn tags_rename_history_blame_and_stash() {
    if !git_available() {
        return;
    }
    let root = repository();
    let path = root.path();
    let provider = GitProvider::new("git");

    provider
        .create_tag(path, "v1.0", None, Some("First release"))
        .await
        .unwrap();
    provider
        .create_tag(path, "light", None, None)
        .await
        .unwrap();
    assert!(provider.create_tag(path, "-bad", None, None).await.is_err());
    let tags = provider.list_tags(path).await.unwrap();
    let v1 = tags.iter().find(|t| t.name == "v1.0").unwrap();
    assert!(v1.annotated);
    assert_eq!(v1.subject, "First release");
    assert!(tags.iter().any(|t| t.name == "light" && !t.annotated));

    git(path, &["branch", "old-name"]);
    provider
        .rename_branch(path, "old-name", "new-name")
        .await
        .unwrap();
    assert!(git(path, &["branch", "--list", "new-name"]).contains("new-name"));
    assert!(
        provider
            .rename_branch(path, "new-name", "bad..name")
            .await
            .is_err()
    );

    commit_file(path, "tracked.txt", "initial\nsecond\n", "add second line");
    let history = provider
        .file_history(path, "tracked.txt", 10)
        .await
        .unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].subject, "add second line");

    let blame = provider.blame(path, "tracked.txt").await.unwrap();
    assert_eq!(blame.len(), 2);
    assert_eq!(blame[0].summary, "initial");
    assert_eq!(blame[1].summary, "add second line");
    assert_eq!(blame[1].author, "Pipeline Test");
    assert_eq!(blame[1].content, "second");

    fs::write(path.join("tracked.txt"), "stashed change\n").unwrap();
    git(path, &["stash", "push", "-m", "wip"]);
    let stash = provider.stash_show(path, None).await.unwrap();
    assert_eq!(stash.files.len(), 1);
    assert_eq!(stash.files[0].path, "tracked.txt");
    assert!(stash.patch.contains("+stashed change"));
}

#[tokio::test]
async fn cherry_pick_and_revert() {
    if !git_available() {
        return;
    }
    let root = repository();
    let path = root.path();
    let provider = GitProvider::new("git");
    git(path, &["switch", "-c", "topic"]);
    commit_file(path, "topic.txt", "t\n", "topic change");
    let hash = git(path, &["rev-parse", "HEAD"]);
    git(path, &["switch", "main"]);
    let outcome = provider.cherry_pick(path, &hash).await.unwrap();
    assert!(matches!(outcome, MergeOutcome::Merged { .. }));
    assert!(path.join("topic.txt").exists());

    let picked = git(path, &["rev-parse", "HEAD"]);
    let outcome = provider.revert_commit(path, &picked).await.unwrap();
    assert!(matches!(outcome, MergeOutcome::Merged { .. }));
    assert!(!path.join("topic.txt").exists());

    // A conflicting cherry-pick reports conflicts and can be aborted.
    git(path, &["switch", "topic"]);
    commit_file(path, "tracked.txt", "topic version\n", "topic edit");
    let conflicting = git(path, &["rev-parse", "HEAD"]);
    git(path, &["switch", "main"]);
    commit_file(path, "tracked.txt", "main version\n", "main edit");
    let outcome = provider.cherry_pick(path, &conflicting).await.unwrap();
    assert!(
        matches!(&outcome, MergeOutcome::Conflicts { files } if files == &["tracked.txt".to_owned()])
    );
    assert!(provider.mark_resolved(path, "tracked.txt").await.is_err());
    provider.abort_operation(path).await.unwrap();
    assert!(provider.pending_operation(path).await.is_none());
}

#[cfg(unix)]
fn fake_git(dir: &Path, failures: u32, stderr: &str) -> std::path::PathBuf {
    let script = dir.join("fake-git.sh");
    let counter = dir.join("count");
    let body = format!(
        "#!/bin/sh\nif [ \"$1\" = fetch ]; then\n  n=$(cat '{counter}' 2>/dev/null || echo 0)\n  n=$((n+1))\n  echo $n > '{counter}'\n  if [ $n -le {failures} ]; then echo \"{stderr}\" >&2; exit 128; fi\nfi\nexec git \"$@\"\n",
        counter = counter.display()
    );
    // Written by `install`, never held open here: a test child spawned
    // meanwhile could inherit the handle and make the script "Text file busy".
    let staging = dir.join("fake-git.txt");
    fs::write(&staging, body).unwrap();
    let status = std::process::Command::new("install")
        .args(["-m", "755"])
        .arg(&staging)
        .arg(&script)
        .status()
        .unwrap();
    assert!(status.success());
    script
}

#[cfg(unix)]
#[tokio::test]
async fn network_operations_retry_transient_failures_only() {
    if !git_available() {
        return;
    }
    let (_remote, work) = clone_with_remote();
    let scripts = tempfile::tempdir().unwrap();

    let flaky = fake_git(
        scripts.path(),
        2,
        "fatal: unable to access 'https://example.invalid/': Could not resolve host: example.invalid",
    );
    let provider = GitProvider::new(&flaky).with_retry_backoff(std::time::Duration::ZERO);
    let result = provider
        .fetch(work.path())
        .await
        .expect("fetch after retries");
    assert_eq!(result.attempts, 3);

    let auth_dir = tempfile::tempdir().unwrap();
    let auth = fake_git(
        auth_dir.path(),
        5,
        "fatal: Authentication failed for 'https://example.invalid/'",
    );
    let provider = GitProvider::new(&auth).with_retry_backoff(std::time::Duration::ZERO);
    assert!(provider.fetch(work.path()).await.is_err());
    let count = fs::read_to_string(auth_dir.path().join("count")).unwrap();
    assert_eq!(count.trim(), "1", "auth failures must not be retried");
}
