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

const LOW_DISK_SPACE_BYTES: u64 = 5 * 1024 * 1024 * 1024; // 5 GiB
const CRITICAL_DISK_SPACE_BYTES: u64 = 500 * 1024 * 1024; // 500 MiB

/// Warns when the project's drive is low on free space — engine builds,
/// asset caches, and Git checkouts all fail unpredictably (and sometimes
/// corrupt state) when a write runs out of room mid-operation. A single
/// syscall, so this is cheap enough to run on every health check, including
/// the lightweight/automatic ones.
pub struct DiskSpaceCheck;

#[async_trait]
impl HealthCheck for DiskSpaceCheck {
    async fn run(&self, project_root: &Path) -> Vec<HealthIssue> {
        let Some(free_bytes) = available_space(project_root) else {
            return Vec::new();
        };
        let (code, severity, title) = if free_bytes < CRITICAL_DISK_SPACE_BYTES {
            (
                "DISK_SPACE_CRITICAL",
                HealthSeverity::Error,
                "Drive is nearly out of space",
            )
        } else if free_bytes < LOW_DISK_SPACE_BYTES {
            (
                "DISK_SPACE_LOW",
                HealthSeverity::Warning,
                "Drive is low on space",
            )
        } else {
            return Vec::new();
        };
        vec![HealthIssue {
            code: code.into(),
            severity,
            title: title.into(),
            detail: format!("{} free on this project's drive.", format_bytes(free_bytes)),
            remediation: Some(
                "Free up disk space before building, syncing, or importing large assets.".into(),
            ),
            checked_at: Utc::now(),
        }]
    }
}

fn format_bytes(bytes: u64) -> String {
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
    const MIB: f64 = 1024.0 * 1024.0;
    let bytes = bytes as f64;
    if bytes >= GIB {
        format!("{:.1} GB", bytes / GIB)
    } else {
        format!("{:.0} MB", bytes / MIB)
    }
}

/// Free bytes on the volume containing `path`, or `None` if it can't be
/// determined (including on platforms this isn't implemented for yet — this
/// check degrades to a silent no-op rather than a false "unknown" warning).
#[cfg(windows)]
fn available_space(path: &Path) -> Option<u64> {
    use std::ffi::{OsStr, c_void};
    use std::os::windows::ffi::OsStrExt;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetDiskFreeSpaceExW(
            directory_name: *const u16,
            free_bytes_available_to_caller: *mut u64,
            total_number_of_bytes: *mut c_void,
            total_number_of_free_bytes: *mut c_void,
        ) -> i32;
    }

    let wide: Vec<u16> = OsStr::new(path)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut free_available: u64 = 0;
    // SAFETY: `wide` is a NUL-terminated UTF-16 buffer alive for the call;
    // the two byte-count out-params we don't need are null, which the API
    // documents as valid (only the requested value is written).
    let ok = unsafe {
        GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut free_available,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    (ok != 0).then_some(free_available)
}

#[cfg(not(windows))]
fn available_space(_path: &Path) -> Option<u64> {
    // Pipeline OS is a Windows-first app today (see README "Known
    // limitations"); macOS/Linux disk-space detection can follow once those
    // targets are past experimental status.
    None
}
