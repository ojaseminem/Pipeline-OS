use std::path::PathBuf;

use semver::{Version, VersionReq};
use thiserror::Error;
use vantadeck_domain::{AppInstallation, AppState, LaunchProfile};
use vantadeck_security::{PathSecurityError, resolve_within_root};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchSpec {
    pub executable: PathBuf,
    pub arguments: Vec<String>,
    pub working_directory: PathBuf,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum LaunchError {
    #[error("launch executable cannot be empty")]
    EmptyExecutable,
    #[error("launch working directory cannot be empty")]
    EmptyWorkingDirectory,
    #[error("no compatible installed application version was found")]
    NoCompatibleVersion,
    #[error("launch executable is missing: {0}")]
    ExecutableMissing(PathBuf),
    #[error("launch working directory is missing: {0}")]
    WorkingDirectoryMissing(PathBuf),
    #[error("launch profile contains an unsafe working directory: {0}")]
    UnsafeWorkingDirectory(#[from] PathSecurityError),
}

pub fn resolve_launch_profile(
    project_root: &std::path::Path,
    profile: &LaunchProfile,
    installations: &[AppInstallation],
) -> Result<LaunchSpec, LaunchError> {
    let eligible = installations
        .iter()
        .filter(|installation| {
            matches!(
                installation.state,
                AppState::Installed
                    | AppState::NewDetected
                    | AppState::ManuallyOverridden
                    | AppState::Portable
            ) && installation.executable.is_file()
        })
        .collect::<Vec<_>>();
    let selected = profile
        .preferred_version
        .as_deref()
        .and_then(|constraint| select_version(&eligible, constraint))
        .or_else(|| {
            profile
                .fallback_version
                .as_deref()
                .and_then(|constraint| select_version(&eligible, constraint))
        })
        .or_else(|| eligible.iter().max_by_key(|item| &item.version).copied())
        .ok_or(LaunchError::NoCompatibleVersion)?;
    if !selected.executable.is_file() {
        return Err(LaunchError::ExecutableMissing(selected.executable.clone()));
    }
    let working_directory = resolve_within_root(
        project_root,
        std::path::Path::new(profile.working_directory.as_deref().unwrap_or(".")),
    )?;
    if !working_directory.is_dir() {
        return Err(LaunchError::WorkingDirectoryMissing(working_directory));
    }
    let root = project_root.to_string_lossy();
    let arguments = profile
        .arguments
        .iter()
        .map(|argument| argument.replace("{projectRoot}", &root))
        .collect();
    LaunchSpec::new(selected.executable.clone(), arguments, working_directory)
}

fn select_version<'a>(
    installations: &[&'a AppInstallation],
    constraint: &str,
) -> Option<&'a AppInstallation> {
    if let Ok(requirement) = VersionReq::parse(constraint) {
        return installations
            .iter()
            .filter(|item| requirement.matches(&item.version))
            .max_by_key(|item| &item.version)
            .copied();
    }
    let exact = Version::parse(constraint).ok()?;
    installations
        .iter()
        .find(|item| item.version == exact)
        .copied()
}

impl LaunchSpec {
    pub fn new(
        executable: PathBuf,
        arguments: Vec<String>,
        working_directory: PathBuf,
    ) -> Result<Self, LaunchError> {
        if executable.as_os_str().is_empty() {
            return Err(LaunchError::EmptyExecutable);
        }
        if working_directory.as_os_str().is_empty() {
            return Err(LaunchError::EmptyWorkingDirectory);
        }
        Ok(Self {
            executable,
            arguments,
            working_directory,
        })
    }

    pub fn command(&self) -> tokio::process::Command {
        let mut command = tokio::process::Command::new(&self.executable);
        command
            .args(&self.arguments)
            .current_dir(&self.working_directory);
        command
    }
}
