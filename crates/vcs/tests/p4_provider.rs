use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use vantadeck_vcs::{
    Confirmation, LockOperation, P4CancellationToken, P4ConnectionConfig, P4DiagnosisState,
    P4Error, P4Provider, ReconcileMode, classify_p4_failure, discover_p4_executable,
    parse_p4_changelist_created, parse_p4_info, parse_p4_opened, parse_p4_reconcile,
    parse_p4_status,
};

#[test]
fn parses_tagged_info_and_reports_configuration() {
    let info = parse_p4_info(
        "... userName alice\n... clientName alice-main\n... clientRoot C:\\work\n... serverAddress ssl:perforce.example:1666\n... serverVersion P4D/LINUX26X86_64/2025.1/123456\n",
    )
    .expect("valid info");

    assert_eq!(info.user.as_deref(), Some("alice"));
    assert_eq!(info.client.as_deref(), Some("alice-main"));
    assert_eq!(info.client_root.as_deref(), Some("C:\\work"));
    assert_eq!(
        info.server_address.as_deref(),
        Some("ssl:perforce.example:1666")
    );
    assert!(info.is_configured());
}

#[test]
fn constructs_process_invocation_as_discrete_arguments() {
    let provider = P4Provider::new("custom-p4").with_config(P4ConnectionConfig {
        port: Some("ssl:example:1666".into()),
        user: Some("alice smith".into()),
        client: Some("alice-main".into()),
        charset: None,
    });
    let invocation =
        provider.command_spec(Path::new("C:\\workspace"), &["sync", "file name.uasset"]);

    assert_eq!(invocation.executable, PathBuf::from("custom-p4"));
    assert_eq!(
        invocation.arguments,
        vec![
            "-p",
            "ssl:example:1666",
            "-u",
            "alice smith",
            "-c",
            "alice-main",
            "sync",
            "file name.uasset"
        ]
    );
    assert_eq!(invocation.working_directory, PathBuf::from("C:\\workspace"));
}

#[test]
fn parses_status_records_without_losing_paths_with_spaces() {
    let status = parse_p4_status(
        "... depotFile //depot/Game/Content/Hero Mesh.uasset\n... clientFile C:\\work\\Content\\Hero Mesh.uasset\n... action reconcile\n\n... depotFile //depot/Game/new.txt\n... clientFile C:\\work\\new.txt\n... action add\n",
    )
    .expect("valid status");

    assert_eq!(status.len(), 2);
    assert_eq!(
        status[0].depot_path,
        "//depot/Game/Content/Hero Mesh.uasset"
    );
    assert_eq!(status[0].action, "reconcile");
    assert_eq!(status[1].action, "add");
}

#[test]
fn parses_opened_and_lock_metadata() {
    let opened = parse_p4_opened(
        "... depotFile //depot/a.uasset\n... clientFile C:\\work\\a.uasset\n... action edit\n... change 42\n... user alice\n... client alice-main\n... ourLock 1\n\n... depotFile //depot/b.uasset\n... action edit\n... otherOpen0 bob@build-client\n... otherAction0 edit\n... otherLock0 \n... otherOpen1 carol@artist-client\n... otherLock1 1\n",
    )
    .expect("valid opened output");

    assert_eq!(opened.len(), 2);
    assert!(opened[0].locked_by_me);
    assert_eq!(
        opened[1].locked_by,
        vec!["bob@build-client", "carol@artist-client"]
    );
}

#[test]
fn parses_reconcile_preview_records() {
    let preview = parse_p4_reconcile(
        "... depotFile //depot/new.bin\n... clientFile C:\\work\\new.bin\n... action add\n\n... depotFile //depot/old.bin\n... action delete\n",
    )
    .expect("valid reconcile output");

    assert_eq!(preview.len(), 2);
    assert_eq!(preview[0].action, "add");
    assert_eq!(preview[1].action, "delete");
}

#[test]
fn rejects_malformed_and_error_tagged_output() {
    assert!(parse_p4_status("not tagged output\n").is_err());
    let error = parse_p4_status(
        "... code error\n... severity 3\n... generic 17\n... data Perforce password invalid or unset.\n... customField retained\n",
    )
    .expect_err("tagged errors must not become empty success");
    let P4Error::CommandFailed { tagged, .. } = error else {
        panic!("expected structured command failure");
    };
    assert_eq!(tagged[0].code.as_deref(), Some("error"));
    assert_eq!(tagged[0].severity, Some(3));
    assert_eq!(tagged[0].generic, Some(17));
    assert_eq!(tagged[0].fields["customField"], vec!["retained"]);
}

#[test]
fn classifies_common_perforce_failures() {
    assert_eq!(
        classify_p4_failure("Perforce password (P4PASSWD) invalid or unset."),
        P4DiagnosisState::AuthenticationExpired
    );
    assert_eq!(
        classify_p4_failure("The authenticity of 'ssl:server:1666' can't be established."),
        P4DiagnosisState::SslTrustRequired
    );
    assert_eq!(
        classify_p4_failure("Client 'missing' unknown - use 'client' command to create it."),
        P4DiagnosisState::ConfigurationInvalid
    );
    assert_eq!(
        classify_p4_failure("Connect to server failed; check $P4PORT."),
        P4DiagnosisState::ServerUnavailable
    );
}

#[test]
fn parses_only_specific_created_changelist_response() {
    assert_eq!(
        parse_p4_changelist_created("Change 123 created.\n"),
        Some(123)
    );
    assert_eq!(
        parse_p4_changelist_created("2026 warnings; no change created"),
        None
    );
    assert_eq!(parse_p4_changelist_created("Change 123 updated."), None);
}

#[test]
fn discovers_p4_in_known_locations() {
    let directory = tempfile::tempdir().expect("temp directory");
    let executable = directory
        .path()
        .join(if cfg!(windows) { "p4.exe" } else { "p4" });
    std::fs::write(&executable, b"fake").expect("fake executable");
    // On Unix, discovery requires the execute bit; set it so the fake binary is
    // recognised the same way a real one would be.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(&executable).expect("metadata").permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&executable, permissions).expect("chmod");
    }

    assert_eq!(
        discover_p4_executable(
            std::slice::from_ref(&executable),
            Some(std::ffi::OsStr::new("")),
        ),
        Some(executable)
    );
}

#[test]
fn constructs_exact_lock_and_unlock_arguments() {
    let provider = P4Provider::new("p4");
    let paths = ["Content/Hero Mesh.uasset", "Content/A.uasset"];
    let lock = provider.lock_command_spec(Path::new("."), LockOperation::Lock, &paths);
    let unlock = provider.lock_command_spec(Path::new("."), LockOperation::Unlock, &paths);

    assert_eq!(lock.arguments, vec!["lock", "--", paths[0], paths[1]]);
    assert_eq!(unlock.arguments, vec!["unlock", "--", paths[0], paths[1]]);
    assert!(provider.capabilities().lock);
    assert!(provider.capabilities().unlock);
}

#[tokio::test]
async fn caller_cancellation_is_distinct_from_timeout() {
    let executable = sleeping_executable();
    let provider = P4Provider::new(&executable).with_timeout(Duration::from_secs(10));
    let cancellation = P4CancellationToken::new();
    let cancel = cancellation.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        cancel.cancel();
    });

    let error = provider
        .status_with_cancellation(Path::new("."), &cancellation)
        .await
        .expect_err("operation must cancel");
    assert!(matches!(error, P4Error::Cancelled));

    let timeout_provider = P4Provider::new(executable).with_timeout(Duration::from_millis(20));
    let error = timeout_provider
        .status(Path::new("."))
        .await
        .expect_err("operation must time out");
    assert!(matches!(error, P4Error::Timeout(_)));
}

#[tokio::test]
async fn diagnosis_runs_info_then_login_status_and_classifies_expired_auth() {
    let executable = diagnostic_executable();
    let diagnosis = P4Provider::new(executable)
        .diagnose(Path::new("."))
        .await
        .expect("diagnosis completes");

    assert_eq!(diagnosis.state, P4DiagnosisState::AuthenticationExpired);
    assert_eq!(
        diagnosis.info.expect("parsed info").client.as_deref(),
        Some("alice-main")
    );
}

#[tokio::test]
async fn diagnosis_classifies_missing_binary() {
    let diagnosis = P4Provider::new("certainly-not-installed-p4")
        .diagnose(Path::new("."))
        .await
        .expect("missing executable is a diagnosis state");
    assert_eq!(diagnosis.state, P4DiagnosisState::ExecutableNotFound);
}

#[tokio::test]
async fn nonzero_exit_preserves_status_streams_and_tagged_fields() {
    let executable = failing_executable();
    let error = P4Provider::new(executable)
        .status(Path::new("."))
        .await
        .expect_err("fake command fails");
    let P4Error::CommandFailed {
        status,
        stdout,
        stderr,
        tagged,
        ..
    } = error
    else {
        panic!("expected command failure");
    };
    assert_eq!(status, Some(7));
    assert!(stdout.contains("... generic 17"));
    assert!(stderr.contains("transport detail"));
    assert_eq!(tagged[0].generic, Some(17));
}

#[tokio::test]
async fn lock_and_unlock_require_confirmation_before_spawning() {
    let provider = P4Provider::new("certainly-not-installed-p4");
    for operation in [LockOperation::Lock, LockOperation::Unlock] {
        let error = provider
            .lock_files(
                Path::new("."),
                operation,
                &["Content/A.uasset"],
                Confirmation::NotConfirmed,
            )
            .await
            .expect_err("confirmation required");
        assert!(matches!(error, P4Error::ConfirmationRequired { .. }));
    }
}

#[tokio::test]
async fn confirmed_create_changelist_captures_created_response() {
    let change = P4Provider::new(changelist_executable())
        .create_changelist(Path::new("."), "Release work", Confirmation::Confirmed)
        .await
        .expect("created response captured from piped stdout");
    assert_eq!(change, 321);
}

#[tokio::test]
async fn changelist_stdin_phase_honors_cancellation_and_timeout() {
    let description = "x".repeat(4 * 1024 * 1024);
    let cancellation = P4CancellationToken::new();
    let cancel = cancellation.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(30)).await;
        cancel.cancel();
    });
    let error = P4Provider::new(input_stall_executable())
        .with_timeout(Duration::from_secs(10))
        .with_cancellation(cancellation)
        .create_changelist(Path::new("."), &description, Confirmation::Confirmed)
        .await
        .expect_err("blocked stdin write must cancel");
    assert!(matches!(error, P4Error::Cancelled));

    let error = P4Provider::new(input_stall_executable())
        .with_timeout(Duration::from_millis(30))
        .create_changelist(Path::new("."), &description, Confirmation::Confirmed)
        .await
        .expect_err("blocked stdin write must time out");
    assert!(matches!(error, P4Error::Timeout(_)));
}

#[tokio::test]
async fn diagnosis_propagates_caller_cancellation() {
    let cancellation = P4CancellationToken::new();
    cancellation.cancel();
    let error = P4Provider::new(sleeping_executable())
        .with_cancellation(cancellation)
        .diagnose(Path::new("."))
        .await
        .expect_err("diagnosis cancellation must remain distinguishable");
    assert!(matches!(error, P4Error::Cancelled));
}

#[tokio::test]
async fn executes_exact_lock_unlock_and_submit_arguments() {
    let (executable, log) = argument_logging_executable();
    let provider = P4Provider::new(executable);
    for operation in [LockOperation::Lock, LockOperation::Unlock] {
        provider
            .lock_files(
                Path::new("."),
                operation,
                &["Content/Hero Mesh.uasset"],
                Confirmation::Confirmed,
            )
            .await
            .expect("lock operation executes");
    }
    provider
        .submit_changelist(Path::new("."), 42, Confirmation::Confirmed)
        .await
        .expect("submit executes");

    let log = std::fs::read_to_string(log)
        .expect("argument log")
        .replace('\r', "");
    let path_argument = if cfg!(windows) {
        "\"Content/Hero Mesh.uasset\""
    } else {
        "Content/Hero Mesh.uasset"
    };
    assert_eq!(
        log.lines().map(str::to_owned).collect::<Vec<_>>(),
        vec![
            format!("lock -- {path_argument}"),
            format!("unlock -- {path_argument}"),
            "submit -c 42".to_owned()
        ]
    );
}

#[cfg(unix)]
#[test]
fn discovery_rejects_non_executable_unix_files() {
    use std::os::unix::fs::PermissionsExt;
    let directory = tempfile::tempdir().expect("temp directory");
    let candidate = directory.path().join("p4");
    std::fs::write(&candidate, "not executable").expect("candidate");
    std::fs::set_permissions(&candidate, std::fs::Permissions::from_mode(0o644))
        .expect("permissions");
    assert_eq!(
        discover_p4_executable(
            std::slice::from_ref(&candidate),
            Some(std::ffi::OsStr::new("")),
        ),
        None
    );
}

#[test]
fn changelist_parser_continues_after_malformed_change_tokens() {
    assert_eq!(
        parse_p4_changelist_created("Change nope ignored. Change 456 created."),
        Some(456)
    );
}

fn sleeping_executable() -> PathBuf {
    let directory = tempfile::tempdir().expect("temp directory").keep();
    #[cfg(windows)]
    {
        let path = directory.join("sleep.cmd");
        std::fs::write(
            &path,
            "@echo off\r\nfor /L %%i in (1,1,100000000) do @rem\r\n",
        )
        .expect("script");
        path
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let path = directory.join("sleep.sh");
        std::fs::write(&path, "#!/bin/sh\nsleep 5\n").expect("script");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
            .expect("permissions");
        path
    }
}

fn diagnostic_executable() -> PathBuf {
    fake_executable(
        "if [ \"$1\" = \"-V\" ]; then echo 'Rev. P4/TEST/2026.1'; exit 0; fi\nif [ \"$1\" = \"-ztag\" ] && [ \"$2\" = \"info\" ]; then printf '%s\\n' '... userName alice' '... clientName alice-main' '... clientRoot /work' '... serverAddress ssl:server:1666' '... serverVersion P4D/TEST'; exit 0; fi\nif [ \"$1\" = \"-ztag\" ] && [ \"$2\" = \"login\" ] && [ \"$3\" = \"-s\" ]; then printf '%s\\n' '... code error' '... severity 3' '... generic 17' '... data Your session has expired, please login again.'; exit 1; fi\necho unexpected command >&2\nexit 2\n",
        "if \"%1\"==\"-V\" (echo Rev. P4/TEST/2026.1& exit /b 0)\r\nif \"%1 %2\"==\"-ztag info\" (echo ... userName alice& echo ... clientName alice-main& echo ... clientRoot C:\\work& echo ... serverAddress ssl:server:1666& echo ... serverVersion P4D/TEST& exit /b 0)\r\nif \"%1 %2 %3\"==\"-ztag login -s\" (echo ... code error& echo ... severity 3& echo ... generic 17& echo ... data Your session has expired, please login again.& exit /b 1)\r\necho unexpected command 1>&2\r\nexit /b 2\r\n",
    )
}

fn failing_executable() -> PathBuf {
    fake_executable(
        "printf '%s\\n' '... code error' '... severity 3' '... generic 17' '... data failure'\necho 'transport detail' >&2\nexit 7\n",
        "echo ... code error\r\necho ... severity 3\r\necho ... generic 17\r\necho ... data failure\r\necho transport detail 1>&2\r\nexit /b 7\r\n",
    )
}

fn fake_executable(unix_body: &str, windows_body: &str) -> PathBuf {
    #[cfg(windows)]
    let _ = unix_body;
    #[cfg(unix)]
    let _ = windows_body;
    let directory = tempfile::tempdir().expect("temp directory").keep();
    #[cfg(windows)]
    {
        let path = directory.join("fake.cmd");
        std::fs::write(&path, format!("@echo off\r\n{windows_body}")).expect("script");
        path
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let path = directory.join("fake.sh");
        std::fs::write(&path, format!("#!/bin/sh\n{unix_body}")).expect("script");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
            .expect("permissions");
        path
    }
}

fn changelist_executable() -> PathBuf {
    fake_executable(
        "if [ \"$1 $2\" = \"change -o\" ]; then printf 'Description:\\n\\told\\n'; exit 0; fi\ncat >/dev/null\necho 'Change 321 created.'\n",
        "if \"%1 %2\"==\"change -o\" (echo Description:& echo 	old& exit /b 0)\r\nmore > nul\r\necho Change 321 created.\r\n",
    )
}

fn input_stall_executable() -> PathBuf {
    fake_executable(
        "if [ \"$1 $2\" = \"change -o\" ]; then printf 'Description:\\n\\told\\n'; exit 0; fi\nsleep 10\n",
        "if \"%1 %2\"==\"change -o\" (echo Description:& echo 	old& exit /b 0)\r\nfor /L %%i in (1,1,100000000) do @rem\r\n",
    )
}

fn argument_logging_executable() -> (PathBuf, PathBuf) {
    let directory = tempfile::tempdir().expect("temp directory").keep();
    let log = directory.join("arguments.log");
    #[cfg(windows)]
    {
        let executable = directory.join("args.cmd");
        std::fs::write(
            &executable,
            format!("@echo off\r\necho %*>>\"{}\"\r\n", log.display()),
        )
        .expect("script");
        (executable, log)
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let executable = directory.join("args.sh");
        std::fs::write(
            &executable,
            format!("#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\n", log.display()),
        )
        .expect("script");
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755))
            .expect("permissions");
        (executable, log)
    }
}

#[tokio::test]
async fn reconcile_execute_requires_confirmation_before_spawning() {
    let provider = P4Provider::new("definitely-missing-p4").with_timeout(Duration::from_millis(50));
    let error = provider
        .reconcile(
            Path::new("."),
            ReconcileMode::Execute,
            Confirmation::NotConfirmed,
        )
        .await
        .expect_err("execution must be rejected");

    assert!(matches!(error, P4Error::ConfirmationRequired { .. }));
}

#[tokio::test]
async fn changelist_creation_requires_confirmation_before_spawning() {
    let provider = P4Provider::new("definitely-missing-p4");
    let error = provider
        .create_changelist(Path::new("."), "description", Confirmation::NotConfirmed)
        .await
        .expect_err("creation must be rejected");

    assert!(matches!(error, P4Error::ConfirmationRequired { .. }));
}

#[tokio::test]
async fn sync_requires_confirmation_before_spawning() {
    let provider = P4Provider::new("definitely-missing-p4");
    let error = provider
        .sync(Path::new("."), Confirmation::NotConfirmed)
        .await
        .expect_err("sync must be rejected");

    assert!(matches!(error, P4Error::ConfirmationRequired { .. }));
}
