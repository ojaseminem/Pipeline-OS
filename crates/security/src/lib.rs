use std::{
    fs::File,
    io::{self, Read},
    path::{Component, Path, PathBuf},
};

use sha2::{Digest, Sha256};
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PathSecurityError {
    #[error("project configuration paths must be relative")]
    AbsoluteProjectPath,
    #[error("path escapes the declared project root")]
    OutsideRoot,
}

#[derive(Debug, Error)]
pub enum ArtifactVerificationError {
    #[error("artifact could not be read: {0}")]
    Io(#[from] io::Error),
    #[error("expected SHA-256 digest is invalid")]
    InvalidExpectedChecksum,
    #[error("artifact checksum mismatch: expected {expected}, found {actual}")]
    ChecksumMismatch { expected: String, actual: String },
}

impl PathSecurityError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::AbsoluteProjectPath => "ABSOLUTE_PROJECT_PATH",
            Self::OutsideRoot => "PATH_OUTSIDE_ROOT",
        }
    }
}

pub fn resolve_within_root(root: &Path, relative: &Path) -> Result<PathBuf, PathSecurityError> {
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| matches!(component, Component::Prefix(_) | Component::RootDir))
    {
        return Err(PathSecurityError::AbsoluteProjectPath);
    }

    let mut resolved = root.to_path_buf();
    let root_depth = resolved.components().count();
    for component in relative.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(segment) => resolved.push(segment),
            Component::ParentDir if resolved.components().count() > root_depth => {
                resolved.pop();
            }
            Component::ParentDir => return Err(PathSecurityError::OutsideRoot),
            Component::Prefix(_) | Component::RootDir => {
                return Err(PathSecurityError::AbsoluteProjectPath);
            }
        }
    }
    Ok(resolved)
}

pub fn verify_sha256(artifact: &Path, expected: &str) -> Result<(), ArtifactVerificationError> {
    if expected.len() != 64 || !expected.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(ArtifactVerificationError::InvalidExpectedChecksum);
    }
    let mut file = File::open(artifact)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let actual = format!("{:x}", hasher.finalize());
    if actual.eq_ignore_ascii_case(expected) {
        Ok(())
    } else {
        Err(ArtifactVerificationError::ChecksumMismatch {
            expected: expected.to_ascii_lowercase(),
            actual,
        })
    }
}
