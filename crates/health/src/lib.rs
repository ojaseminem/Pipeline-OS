use std::path::Path;

use async_trait::async_trait;
use chrono::Utc;
use vantadeck_domain::{HealthIssue, HealthSeverity};

#[async_trait]
pub trait HealthCheck: Send + Sync {
    async fn run(&self, project_root: &Path) -> Vec<HealthIssue>;
}

pub struct ProjectPathCheck;

#[async_trait]
impl HealthCheck for ProjectPathCheck {
    async fn run(&self, project_root: &Path) -> Vec<HealthIssue> {
        if project_root.is_dir() {
            return Vec::new();
        }
        vec![HealthIssue {
            code: "PROJECT_PATH_MISSING".into(),
            severity: HealthSeverity::Error,
            title: "Project path missing".into(),
            detail: format!("{} is unavailable", project_root.display()),
            remediation: Some("Reconnect the drive or update the project path.".into()),
            checked_at: Utc::now(),
        }]
    }
}
