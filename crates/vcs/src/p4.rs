use std::{
    collections::BTreeMap,
    env, io,
    path::{Path, PathBuf},
    process::{Output, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::{io::AsyncWriteExt, process::Command, sync::Notify, time::sleep};

use crate::VcsOperationResult;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct P4Capabilities {
    pub status: bool,
    pub sync: bool,
    pub opened_files: bool,
    pub reconcile: bool,
    pub changelists: bool,
    pub submit: bool,
    pub lock_status: bool,
    pub lock: bool,
    pub unlock: bool,
}

impl Default for P4Capabilities {
    fn default() -> Self {
        Self {
            status: true,
            sync: true,
            opened_files: true,
            reconcile: true,
            changelists: true,
            submit: true,
            lock_status: true,
            lock: true,
            unlock: true,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct P4ConnectionConfig {
    pub port: Option<String>,
    pub user: Option<String>,
    pub client: Option<String>,
    pub charset: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct P4Info {
    pub user: Option<String>,
    pub client: Option<String>,
    pub client_root: Option<String>,
    pub server_address: Option<String>,
    pub server_version: Option<String>,
}

impl P4Info {
    pub fn is_configured(&self) -> bool {
        self.user.is_some()
            && self
                .client
                .as_deref()
                .is_some_and(|client| client != "*unknown*")
            && self.client_root.is_some()
            && self.server_address.is_some()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum P4DiagnosisState {
    Ready,
    ExecutableNotFound,
    AuthenticationExpired,
    ConfigurationInvalid,
    SslTrustRequired,
    ServerUnavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct P4Diagnosis {
    pub state: P4DiagnosisState,
    pub executable: PathBuf,
    pub version: Option<String>,
    pub info: Option<P4Info>,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct P4FileStatus {
    pub depot_path: String,
    pub client_path: Option<String>,
    pub action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct P4OpenedFile {
    pub depot_path: String,
    pub client_path: Option<String>,
    pub action: String,
    pub changelist: Option<String>,
    pub user: Option<String>,
    pub client: Option<String>,
    pub locked_by_me: bool,
    pub locked_by: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct P4TaggedError {
    pub code: Option<String>,
    pub severity: Option<i32>,
    pub generic: Option<i32>,
    pub message: Option<String>,
    pub fields: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confirmation {
    NotConfirmed,
    Confirmed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReconcileMode {
    Preview,
    Execute,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockOperation {
    Lock,
    Unlock,
}

#[derive(Debug, Error)]
pub enum P4Error {
    #[error("Perforce command failed to start: {0}")]
    Io(#[from] io::Error),
    #[error("Perforce command timed out after {0:?}")]
    Timeout(Duration),
    #[error("Perforce command was cancelled by the caller")]
    Cancelled,
    #[error("Perforce command `{command}` failed with status {status:?}: {stderr}")]
    CommandFailed {
        command: String,
        status: Option<i32>,
        stdout: String,
        stderr: String,
        tagged: Vec<P4TaggedError>,
    },
    #[error("malformed Perforce tagged output: {0}")]
    MalformedTaggedOutput(String),
    #[error("confirmation is required for {operation}")]
    ConfirmationRequired { operation: &'static str },
    #[error("changelist description cannot be empty")]
    EmptyDescription,
    #[error("at least one file path is required")]
    EmptyFileList,
    #[error("Perforce returned no `Change <id> created.` response")]
    MissingChangelist,
}

#[derive(Debug, Default)]
struct CancellationInner {
    cancelled: AtomicBool,
    notify: Notify,
}

#[derive(Debug, Clone, Default)]
pub struct P4CancellationToken(Arc<CancellationInner>);

impl P4CancellationToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.cancelled.store(true, Ordering::Release);
        self.0.notify.notify_waiters();
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.cancelled.load(Ordering::Acquire)
    }

    async fn cancelled(&self) {
        loop {
            let notified = self.0.notify.notified();
            if self.is_cancelled() {
                return;
            }
            notified.await;
        }
    }
}

#[derive(Debug, Clone)]
pub struct P4Provider {
    binary: PathBuf,
    config: P4ConnectionConfig,
    timeout: Duration,
    cancellation: Option<P4CancellationToken>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct P4CommandSpec {
    pub executable: PathBuf,
    pub working_directory: PathBuf,
    pub arguments: Vec<String>,
}

impl P4Provider {
    pub fn new(binary: impl Into<PathBuf>) -> Self {
        Self {
            binary: binary.into(),
            config: P4ConnectionConfig::default(),
            timeout: Duration::from_secs(30),
            cancellation: None,
        }
    }

    pub fn discover() -> Option<Self> {
        discover_p4_executable(&default_p4_locations(), env::var_os("PATH").as_deref())
            .map(Self::new)
    }

    pub fn with_config(mut self, config: P4ConnectionConfig) -> Self {
        self.config = config;
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Applies caller-controlled cancellation to every command issued by this provider.
    pub fn with_cancellation(mut self, cancellation: P4CancellationToken) -> Self {
        self.cancellation = Some(cancellation);
        self
    }

    pub fn capabilities(&self) -> P4Capabilities {
        P4Capabilities::default()
    }

    pub fn command_spec(&self, root: &Path, arguments: &[&str]) -> P4CommandSpec {
        let mut all_arguments = Vec::new();
        for (flag, value) in [
            ("-p", self.config.port.as_deref()),
            ("-u", self.config.user.as_deref()),
            ("-c", self.config.client.as_deref()),
            ("-C", self.config.charset.as_deref()),
        ] {
            if let Some(value) = value {
                all_arguments.push(flag.to_owned());
                all_arguments.push(value.to_owned());
            }
        }
        all_arguments.extend(arguments.iter().map(|argument| (*argument).to_owned()));
        P4CommandSpec {
            executable: self.binary.clone(),
            working_directory: root.to_owned(),
            arguments: all_arguments,
        }
    }

    pub fn lock_command_spec(
        &self,
        root: &Path,
        operation: LockOperation,
        paths: &[&str],
    ) -> P4CommandSpec {
        let command = match operation {
            LockOperation::Lock => "lock",
            LockOperation::Unlock => "unlock",
        };
        let mut arguments = vec![command, "--"];
        arguments.extend_from_slice(paths);
        self.command_spec(root, &arguments)
    }

    pub async fn diagnose(&self, root: &Path) -> Result<P4Diagnosis, P4Error> {
        let version_output = match self
            .run_raw(root, &["-V"], self.cancellation.as_ref())
            .await
        {
            Ok(output) => output,
            Err(P4Error::Io(error)) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(self.diagnosis(P4DiagnosisState::ExecutableNotFound, None, None, error));
            }
            Err(error) => {
                if matches!(error, P4Error::Cancelled | P4Error::Timeout(_)) {
                    return Err(error);
                }
                let state = classify_p4_failure(&error.to_string());
                return Ok(self.diagnosis(state, None, None, error));
            }
        };
        let version = Some(
            String::from_utf8_lossy(&version_output.stdout)
                .trim()
                .to_owned(),
        );
        let info_output = match self
            .run(root, &["-ztag", "info"], self.cancellation.as_ref())
            .await
        {
            Ok(output) => output,
            Err(error) => {
                if matches!(error, P4Error::Cancelled | P4Error::Timeout(_)) {
                    return Err(error);
                }
                let state = classify_p4_failure(&error_text(&error));
                return Ok(self.diagnosis(state, version, None, error));
            }
        };
        let info = match parse_p4_info(&String::from_utf8_lossy(&info_output.stdout)) {
            Ok(info) if info.is_configured() => info,
            Ok(info) => {
                return Ok(P4Diagnosis {
                    state: P4DiagnosisState::ConfigurationInvalid,
                    executable: self.binary.clone(),
                    version,
                    info: Some(info),
                    detail: Some("P4USER, P4CLIENT/client root, or P4PORT is missing".into()),
                });
            }
            Err(error) => {
                if matches!(error, P4Error::Cancelled | P4Error::Timeout(_)) {
                    return Err(error);
                }
                let state = classify_p4_failure(&error_text(&error));
                return Ok(self.diagnosis(state, version, None, error));
            }
        };
        match self
            .run(root, &["-ztag", "login", "-s"], self.cancellation.as_ref())
            .await
        {
            Ok(_) => Ok(P4Diagnosis {
                state: P4DiagnosisState::Ready,
                executable: self.binary.clone(),
                version,
                info: Some(info),
                detail: None,
            }),
            Err(error) => {
                if matches!(error, P4Error::Cancelled | P4Error::Timeout(_)) {
                    return Err(error);
                }
                let state = classify_p4_failure(&error_text(&error));
                Ok(self.diagnosis(state, version, Some(info), error))
            }
        }
    }

    pub async fn status(&self, root: &Path) -> Result<Vec<P4FileStatus>, P4Error> {
        self.status_with_token(root, self.cancellation.as_ref())
            .await
    }

    pub async fn status_with_cancellation(
        &self,
        root: &Path,
        cancellation: &P4CancellationToken,
    ) -> Result<Vec<P4FileStatus>, P4Error> {
        self.status_with_token(root, Some(cancellation)).await
    }

    async fn status_with_token(
        &self,
        root: &Path,
        cancellation: Option<&P4CancellationToken>,
    ) -> Result<Vec<P4FileStatus>, P4Error> {
        let output = self.run(root, &["-ztag", "status"], cancellation).await?;
        parse_p4_status(&String::from_utf8_lossy(&output.stdout))
    }

    pub async fn sync(
        &self,
        root: &Path,
        confirmation: Confirmation,
    ) -> Result<VcsOperationResult, P4Error> {
        self.require_confirmation(confirmation, "sync")?;
        self.operation(root, &["sync"], self.cancellation.as_ref())
            .await
    }

    pub async fn opened_files(&self, root: &Path) -> Result<Vec<P4OpenedFile>, P4Error> {
        let output = self
            .run(root, &["-ztag", "opened"], self.cancellation.as_ref())
            .await?;
        parse_p4_opened(&String::from_utf8_lossy(&output.stdout))
    }

    pub async fn lock_status(&self, root: &Path) -> Result<Vec<P4OpenedFile>, P4Error> {
        let output = self
            .run(
                root,
                &["-ztag", "fstat", "-Ol", "..."],
                self.cancellation.as_ref(),
            )
            .await?;
        parse_p4_opened(&String::from_utf8_lossy(&output.stdout))
    }

    pub async fn lock_files(
        &self,
        root: &Path,
        operation: LockOperation,
        paths: &[&str],
        confirmation: Confirmation,
    ) -> Result<VcsOperationResult, P4Error> {
        self.require_confirmation(
            confirmation,
            match operation {
                LockOperation::Lock => "lock files",
                LockOperation::Unlock => "unlock files",
            },
        )?;
        if paths.is_empty() {
            return Err(P4Error::EmptyFileList);
        }
        let spec = self.lock_command_spec(root, operation, paths);
        self.operation_owned(spec, self.cancellation.as_ref()).await
    }

    pub async fn reconcile(
        &self,
        root: &Path,
        mode: ReconcileMode,
        confirmation: Confirmation,
    ) -> Result<Vec<P4FileStatus>, P4Error> {
        if mode == ReconcileMode::Execute {
            self.require_confirmation(confirmation, "reconcile")?;
        }
        let arguments: &[&str] = if mode == ReconcileMode::Preview {
            &["-ztag", "reconcile", "-n"]
        } else {
            &["-ztag", "reconcile"]
        };
        let output = self
            .run(root, arguments, self.cancellation.as_ref())
            .await?;
        parse_p4_reconcile(&String::from_utf8_lossy(&output.stdout))
    }

    pub async fn create_changelist(
        &self,
        root: &Path,
        description: &str,
        confirmation: Confirmation,
    ) -> Result<i64, P4Error> {
        self.require_confirmation(confirmation, "create changelist")?;
        if description.trim().is_empty() {
            return Err(P4Error::EmptyDescription);
        }
        let template = self
            .run(root, &["change", "-o"], self.cancellation.as_ref())
            .await?;
        let spec = replace_description(&String::from_utf8_lossy(&template.stdout), description);
        let output = self
            .run_with_input(
                root,
                &["change", "-i"],
                spec.as_bytes(),
                self.cancellation.as_ref(),
            )
            .await?;
        parse_p4_changelist_created(&String::from_utf8_lossy(&output.stdout))
            .ok_or(P4Error::MissingChangelist)
    }

    pub async fn submit_changelist(
        &self,
        root: &Path,
        changelist: i64,
        confirmation: Confirmation,
    ) -> Result<VcsOperationResult, P4Error> {
        self.require_confirmation(confirmation, "submit changelist")?;
        self.operation(
            root,
            &["submit", "-c", &changelist.to_string()],
            self.cancellation.as_ref(),
        )
        .await
    }

    fn require_confirmation(
        &self,
        confirmation: Confirmation,
        operation: &'static str,
    ) -> Result<(), P4Error> {
        if confirmation == Confirmation::Confirmed {
            Ok(())
        } else {
            Err(P4Error::ConfirmationRequired { operation })
        }
    }

    fn diagnosis(
        &self,
        state: P4DiagnosisState,
        version: Option<String>,
        info: Option<P4Info>,
        error: impl std::fmt::Display,
    ) -> P4Diagnosis {
        P4Diagnosis {
            state,
            executable: self.binary.clone(),
            version,
            info,
            detail: Some(error.to_string()),
        }
    }

    async fn operation(
        &self,
        root: &Path,
        arguments: &[&str],
        cancellation: Option<&P4CancellationToken>,
    ) -> Result<VcsOperationResult, P4Error> {
        let output = self.run(root, arguments, cancellation).await?;
        Ok(operation_result(output))
    }

    async fn operation_owned(
        &self,
        spec: P4CommandSpec,
        cancellation: Option<&P4CancellationToken>,
    ) -> Result<VcsOperationResult, P4Error> {
        let output = self.run_spec(spec, cancellation).await?;
        Ok(operation_result(output))
    }

    async fn run(
        &self,
        root: &Path,
        arguments: &[&str],
        cancellation: Option<&P4CancellationToken>,
    ) -> Result<Output, P4Error> {
        let spec = self.command_spec(root, arguments);
        let output = self.run_spec_raw(&spec, cancellation).await?;
        check_output(&spec, output)
    }

    async fn run_raw(
        &self,
        root: &Path,
        arguments: &[&str],
        cancellation: Option<&P4CancellationToken>,
    ) -> Result<Output, P4Error> {
        let spec = self.command_spec(root, arguments);
        self.run_spec_raw(&spec, cancellation).await
    }

    async fn run_spec(
        &self,
        spec: P4CommandSpec,
        cancellation: Option<&P4CancellationToken>,
    ) -> Result<Output, P4Error> {
        let output = self.run_spec_raw(&spec, cancellation).await?;
        check_output(&spec, output)
    }

    async fn run_spec_raw(
        &self,
        spec: &P4CommandSpec,
        cancellation: Option<&P4CancellationToken>,
    ) -> Result<Output, P4Error> {
        if cancellation.is_some_and(P4CancellationToken::is_cancelled) {
            return Err(P4Error::Cancelled);
        }
        let mut command = command_from_spec(spec);
        let operation = command.output();
        tokio::pin!(operation);
        let deadline = sleep(self.timeout);
        tokio::pin!(deadline);
        if let Some(cancellation) = cancellation {
            tokio::select! {
                biased;
                _ = cancellation.cancelled() => Err(P4Error::Cancelled),
                _ = &mut deadline => Err(P4Error::Timeout(self.timeout)),
                result = &mut operation => result.map_err(P4Error::Io),
            }
        } else {
            tokio::select! {
                _ = &mut deadline => Err(P4Error::Timeout(self.timeout)),
                result = &mut operation => result.map_err(P4Error::Io),
            }
        }
    }

    async fn run_with_input(
        &self,
        root: &Path,
        arguments: &[&str],
        input: &[u8],
        cancellation: Option<&P4CancellationToken>,
    ) -> Result<Output, P4Error> {
        let spec = self.command_spec(root, arguments);
        let mut command = command_from_spec(&spec);
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn()?;
        let mut stdin = child.stdin.take().expect("piped stdin");
        let operation = async move {
            stdin.write_all(input).await?;
            drop(stdin);
            child.wait_with_output().await
        };
        tokio::pin!(operation);
        let deadline = sleep(self.timeout);
        tokio::pin!(deadline);
        let output = if let Some(cancellation) = cancellation {
            tokio::select! {
                biased;
                _ = cancellation.cancelled() => return Err(P4Error::Cancelled),
                _ = &mut deadline => return Err(P4Error::Timeout(self.timeout)),
                result = &mut operation => result?,
            }
        } else {
            tokio::select! {
                _ = &mut deadline => return Err(P4Error::Timeout(self.timeout)),
                result = &mut operation => result?,
            }
        };
        check_output(&spec, output)
    }
}

fn operation_result(output: Output) -> VcsOperationResult {
    VcsOperationResult {
        stdout: String::from_utf8_lossy(&output.stdout).trim().to_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
    }
}

fn command_from_spec(spec: &P4CommandSpec) -> Command {
    let mut command = Command::new(&spec.executable);
    command
        .current_dir(&spec.working_directory)
        .args(&spec.arguments)
        .kill_on_drop(true);
    // Run p4 without flashing a console window on Windows.
    #[cfg(windows)]
    command.creation_flags(0x0800_0000);
    command
}

fn check_output(spec: &P4CommandSpec, output: Output) -> Result<Output, P4Error> {
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let tagged = extract_tagged_errors(&stdout);
    if output.status.success() && tagged.is_empty() {
        return Ok(output);
    }
    Err(P4Error::CommandFailed {
        command: format!("{} {}", spec.executable.display(), spec.arguments.join(" ")),
        status: output.status.code(),
        stdout,
        stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        tagged,
    })
}

type TaggedRecord = BTreeMap<String, Vec<String>>;

fn parse_tagged_records(input: &str) -> Result<Vec<TaggedRecord>, P4Error> {
    let mut records = Vec::new();
    let mut current = TaggedRecord::new();
    for line in input.lines() {
        if line.trim().is_empty() {
            if !current.is_empty() {
                records.push(std::mem::take(&mut current));
            }
            continue;
        }
        let tagged = line
            .strip_prefix("... ")
            .ok_or_else(|| P4Error::MalformedTaggedOutput(line.to_owned()))?;
        let (key, value) = tagged.split_once(' ').unwrap_or((tagged, ""));
        if key.is_empty() {
            return Err(P4Error::MalformedTaggedOutput(line.to_owned()));
        }
        if matches!(key, "depotFile" | "clientFile") && current.contains_key(key) {
            records.push(std::mem::take(&mut current));
        }
        current
            .entry(key.to_owned())
            .or_default()
            .push(value.to_owned());
    }
    if !current.is_empty() {
        records.push(current);
    }
    let errors = records
        .iter()
        .filter(|record| is_error_record(record))
        .map(tagged_error)
        .collect::<Vec<_>>();
    if !errors.is_empty() {
        return Err(P4Error::CommandFailed {
            command: "p4 tagged output".into(),
            status: None,
            stdout: input.to_owned(),
            stderr: String::new(),
            tagged: errors,
        });
    }
    Ok(records)
}

fn extract_tagged_errors(input: &str) -> Vec<P4TaggedError> {
    parse_tagged_records(input)
        .err()
        .and_then(|error| match error {
            P4Error::CommandFailed { tagged, .. } => Some(tagged),
            _ => None,
        })
        .unwrap_or_default()
}

fn is_error_record(record: &TaggedRecord) -> bool {
    first_ref(record, "code").is_some_and(|code| code == "error")
        || first_ref(record, "severity")
            .and_then(|severity| severity.parse::<i32>().ok())
            .is_some_and(|severity| severity >= 3)
}

fn tagged_error(record: &TaggedRecord) -> P4TaggedError {
    P4TaggedError {
        code: first(record, "code"),
        severity: first_ref(record, "severity").and_then(|value| value.parse().ok()),
        generic: first_ref(record, "generic").and_then(|value| value.parse().ok()),
        message: first(record, "data"),
        fields: record.clone(),
    }
}

fn first(record: &TaggedRecord, key: &str) -> Option<String> {
    first_ref(record, key).map(str::to_owned)
}

fn first_ref<'a>(record: &'a TaggedRecord, key: &str) -> Option<&'a str> {
    record
        .get(key)
        .and_then(|values| values.first())
        .map(String::as_str)
}

pub fn parse_p4_info(input: &str) -> Result<P4Info, P4Error> {
    let records = parse_tagged_records(input)?;
    if records.len() != 1 {
        return Err(P4Error::MalformedTaggedOutput(
            "expected exactly one info record".into(),
        ));
    }
    let record = &records[0];
    Ok(P4Info {
        user: first(record, "userName"),
        client: first(record, "clientName"),
        client_root: first(record, "clientRoot"),
        server_address: first(record, "serverAddress"),
        server_version: first(record, "serverVersion"),
    })
}

pub fn parse_p4_status(input: &str) -> Result<Vec<P4FileStatus>, P4Error> {
    parse_file_statuses(input)
}

pub fn parse_p4_reconcile(input: &str) -> Result<Vec<P4FileStatus>, P4Error> {
    parse_file_statuses(input)
}

fn parse_file_statuses(input: &str) -> Result<Vec<P4FileStatus>, P4Error> {
    parse_tagged_records(input)?
        .into_iter()
        .map(|record| {
            let depot_path = required(&record, "depotFile")?;
            let action = required(&record, "action")?;
            Ok(P4FileStatus {
                depot_path,
                client_path: first(&record, "clientFile"),
                action,
            })
        })
        .collect()
}

pub fn parse_p4_opened(input: &str) -> Result<Vec<P4OpenedFile>, P4Error> {
    parse_tagged_records(input)?
        .into_iter()
        .map(|record| {
            let depot_path = required(&record, "depotFile")?;
            let action = required(&record, "action")?;
            Ok(P4OpenedFile {
                depot_path,
                client_path: first(&record, "clientFile"),
                action,
                changelist: first(&record, "change"),
                user: first(&record, "user"),
                client: first(&record, "client"),
                locked_by_me: record.contains_key("ourLock"),
                locked_by: paired_lock_owners(&record),
            })
        })
        .collect()
}

fn required(record: &TaggedRecord, key: &str) -> Result<String, P4Error> {
    first(record, key).ok_or_else(|| {
        P4Error::MalformedTaggedOutput(format!("record missing required `{key}` field"))
    })
}

fn paired_lock_owners(record: &TaggedRecord) -> Vec<String> {
    let mut owners = record
        .keys()
        .filter_map(|key| key.strip_prefix("otherLock"))
        .filter_map(|suffix| suffix.parse::<usize>().ok())
        .filter_map(|index| {
            let lock_value = first_ref(record, &format!("otherLock{index}"));
            let owner = first(record, &format!("otherOpen{index}")).or_else(|| {
                lock_value
                    .filter(|value| !value.is_empty() && *value != "1")
                    .map(str::to_owned)
            });
            owner.map(|owner| (index, owner))
        })
        .collect::<Vec<_>>();
    owners.sort_by_key(|(index, _)| *index);
    owners.into_iter().map(|(_, owner)| owner).collect()
}

pub fn parse_p4_changelist_created(output: &str) -> Option<i64> {
    let mut parts = output.split_whitespace();
    while let Some(part) = parts.next() {
        if part == "Change" {
            let Some(id_token) = parts.next() else {
                continue;
            };
            let Some(status_token) = parts.next() else {
                continue;
            };
            if let Ok(id) = id_token.parse()
                && status_token == "created."
            {
                return Some(id);
            }
        }
    }
    None
}

fn replace_description(template: &str, description: &str) -> String {
    let mut output = String::new();
    let mut lines = template.lines().peekable();
    while let Some(line) = lines.next() {
        if line == "Description:" {
            output.push_str("Description:\n");
            while lines.peek().is_some_and(|next| next.starts_with('\t')) {
                lines.next();
            }
            for description_line in description.lines() {
                output.push('\t');
                output.push_str(description_line);
                output.push('\n');
            }
        } else {
            output.push_str(line);
            output.push('\n');
        }
    }
    output
}

pub fn classify_p4_failure(message: &str) -> P4DiagnosisState {
    let lower = message.to_ascii_lowercase();
    if lower.contains("password")
        || lower.contains("not logged in")
        || lower.contains("login expired")
        || lower.contains("session has expired")
        || lower.contains("ticket expired")
    {
        P4DiagnosisState::AuthenticationExpired
    } else if lower.contains("authenticity")
        || lower.contains("p4trust")
        || lower.contains("ssl trust")
        || lower.contains("fingerprint")
    {
        P4DiagnosisState::SslTrustRequired
    } else if lower.contains("client '")
        || lower.contains("client unknown")
        || lower.contains("not under client's root")
        || lower.contains("p4client")
        || lower.contains("p4port") && lower.contains("unset")
    {
        P4DiagnosisState::ConfigurationInvalid
    } else {
        P4DiagnosisState::ServerUnavailable
    }
}

fn error_text(error: &P4Error) -> String {
    match error {
        P4Error::CommandFailed {
            stdout,
            stderr,
            tagged,
            ..
        } => {
            let messages = tagged
                .iter()
                .filter_map(|error| error.message.as_deref())
                .collect::<Vec<_>>()
                .join(" ");
            format!("{stdout} {stderr} {messages}")
        }
        _ => error.to_string(),
    }
}

pub fn discover_p4_executable(
    known_locations: &[PathBuf],
    path: Option<&std::ffi::OsStr>,
) -> Option<PathBuf> {
    known_locations
        .iter()
        .find(|candidate| is_executable(candidate))
        .cloned()
        .or_else(|| {
            path.into_iter()
                .flat_map(env::split_paths)
                .flat_map(|directory| [directory.join("p4"), directory.join("p4.exe")])
                .find(|candidate| is_executable(candidate))
        })
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

fn default_p4_locations() -> Vec<PathBuf> {
    let mut locations = vec![
        PathBuf::from("/usr/local/bin/p4"),
        PathBuf::from("/usr/bin/p4"),
    ];
    if let Some(program_files) = env::var_os("ProgramFiles") {
        locations.push(PathBuf::from(program_files).join("Perforce").join("p4.exe"));
    }
    if let Some(home) = env::var_os("HOME") {
        locations.push(PathBuf::from(home).join("bin").join("p4"));
    }
    locations
}
