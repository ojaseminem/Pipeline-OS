use std::path::PathBuf;

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use semver::Version;
use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum AppCategory {
    GameEngine,
    Dcc,
    Art,
    Code,
    VersionControl,
    Utility,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum AppState {
    Installed,
    NewDetected,
    Missing,
    Hidden,
    ManuallyOverridden,
    Portable,
    NeedsConfiguration,
    UnsupportedVersion,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct DetectionEvidence {
    pub source: String,
    pub detail: String,
    pub confidence: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct AppInstallation {
    #[schemars(with = "String")]
    pub version: Version,
    pub executable: PathBuf,
    pub state: AppState,
    pub evidence: Vec<DetectionEvidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct DetectedApplication {
    pub id: String,
    pub name: String,
    pub category: AppCategory,
    pub installations: Vec<AppInstallation>,
}

impl DetectedApplication {
    pub fn new(id: impl Into<String>, name: impl Into<String>, category: AppCategory) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            category,
            installations: Vec::new(),
        }
    }

    pub fn add_installation(&mut self, installation: AppInstallation) {
        if self
            .installations
            .iter()
            .any(|existing| existing.executable == installation.executable)
        {
            return;
        }
        self.installations.push(installation);
        self.installations
            .sort_by(|left, right| right.version.cmp(&left.version));
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct ProjectConfig {
    pub schema_version: u32,
    pub name: String,
    pub project_type: String,
    #[serde(default)]
    pub linked_apps: Vec<LinkedApp>,
    #[serde(default)]
    pub launch_profiles: Vec<LaunchProfile>,
    #[serde(default)]
    pub shortcuts: Vec<ProjectShortcut>,
    pub version_control: Option<VersionControlConfig>,
    #[serde(default)]
    pub enabled_health_checks: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct LinkedApp {
    pub app_id: String,
    pub preferred_version: Option<String>,
    pub project_file: Option<String>,
    pub folder: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct LaunchProfile {
    pub id: String,
    pub name: String,
    pub app_id: String,
    #[serde(default)]
    pub arguments: Vec<String>,
    pub working_directory: Option<String>,
    pub preferred_version: Option<String>,
    pub fallback_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct ProjectShortcut {
    pub name: String,
    pub kind: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct VersionControlConfig {
    pub provider: String,
    pub root: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CliEnvelope<T> {
    pub schema_version: u32,
    pub command: String,
    pub success: bool,
    pub data: Option<T>,
    pub warnings: Vec<ApiMessage>,
    pub errors: Vec<ApiMessage>,
}

impl<T> CliEnvelope<T> {
    pub fn success(command: impl Into<String>, data: T) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            command: command.into(),
            success: true,
            data: Some(data),
            warnings: Vec::new(),
            errors: Vec::new(),
        }
    }

    pub fn failure(command: impl Into<String>, error: ApiMessage) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            command: command.into(),
            success: false,
            data: None,
            warnings: Vec::new(),
            errors: vec![error],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ApiMessage {
    pub code: String,
    pub message: String,
    pub remediation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HealthIssue {
    pub code: String,
    pub severity: HealthSeverity,
    pub title: String,
    pub detail: String,
    pub remediation: Option<String>,
    pub checked_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum HealthSeverity {
    Info,
    Warning,
    Error,
}
