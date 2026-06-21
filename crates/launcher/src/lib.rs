use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

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
    #[error("{0} is built for a different processor architecture and cannot run on this machine")]
    IncompatibleArchitecture(PathBuf),
}

// PE `IMAGE_FILE_HEADER.Machine` values for the architectures we reason about.
// Which of these are referenced depends on the host architecture, so each is
// allowed to be unused on targets where it does not apply.
#[allow(dead_code)]
const IMAGE_FILE_MACHINE_I386: u16 = 0x014c;
#[allow(dead_code)]
const IMAGE_FILE_MACHINE_AMD64: u16 = 0x8664;
#[allow(dead_code)]
const IMAGE_FILE_MACHINE_ARM64: u16 = 0xaa64;
#[allow(dead_code)]
const IMAGE_FILE_MACHINE_ARMNT: u16 = 0x01c4;

/// Reads the PE `Machine` field of an executable, returning `None` for files
/// that are not readable PE images (scripts, shell wrappers, app bundles, etc.).
pub fn executable_machine(path: &Path) -> Option<u16> {
    let mut file = std::fs::File::open(path).ok()?;
    let mut dos_header = [0u8; 64];
    file.read_exact(&mut dos_header).ok()?;
    if &dos_header[0..2] != b"MZ" {
        return None;
    }
    let pe_offset = u32::from_le_bytes([
        dos_header[60],
        dos_header[61],
        dos_header[62],
        dos_header[63],
    ]) as u64;
    file.seek(SeekFrom::Start(pe_offset)).ok()?;
    let mut signature_and_machine = [0u8; 6];
    file.read_exact(&mut signature_and_machine).ok()?;
    if &signature_and_machine[0..4] != b"PE\0\0" {
        return None;
    }
    Some(u16::from_le_bytes([
        signature_and_machine[4],
        signature_and_machine[5],
    ]))
}

/// PE machine types this host can execute, ordered widest-compatibility first.
fn host_runnable_machines() -> &'static [u16] {
    #[cfg(target_arch = "x86_64")]
    {
        &[IMAGE_FILE_MACHINE_AMD64, IMAGE_FILE_MACHINE_I386]
    }
    #[cfg(target_arch = "aarch64")]
    {
        &[
            IMAGE_FILE_MACHINE_ARM64,
            IMAGE_FILE_MACHINE_AMD64,
            IMAGE_FILE_MACHINE_I386,
            IMAGE_FILE_MACHINE_ARMNT,
        ]
    }
    #[cfg(target_arch = "x86")]
    {
        &[IMAGE_FILE_MACHINE_I386]
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64", target_arch = "x86")))]
    {
        &[]
    }
}

/// Returns an error if `executable` is a PE image built for an architecture this
/// host cannot run. Non-PE files and unknown architectures are allowed through so
/// that valid scripts and unusual targets are never blocked. This is checked
/// before spawning so the operating system never raises a blocking
/// "machine type mismatch" dialog the user cannot dismiss.
pub fn ensure_runnable(executable: &Path) -> Result<(), LaunchError> {
    let Some(machine) = executable_machine(executable) else {
        return Ok(());
    };
    let runnable = host_runnable_machines();
    if runnable.is_empty() || runnable.contains(&machine) {
        Ok(())
    } else {
        Err(LaunchError::IncompatibleArchitecture(
            executable.to_path_buf(),
        ))
    }
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
    ensure_runnable(&selected.executable)?;
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
