//! Studio workflow primitives shared by the desktop app, the CLI, and the MCP
//! server: pipeline tasks (builds, renders, exports) with a streaming runner,
//! a git-friendly file-based issue tracker, AI-ready project briefs, and
//! studio folder layouts with an asset index.
//!
//! Everything here is project-local and plain-text under `.pipelineos/`, so it
//! travels with the repository and diffs cleanly in code review.

pub mod active_time;
pub mod ai_clients;
pub mod assets;
pub mod context;
pub mod discovery;
pub mod engine_activity;
pub mod github;
pub mod issues;
pub mod task_presets;
pub mod tasks;
pub mod toml_merge;
pub mod trackers;

use std::path::{Path, PathBuf};

use thiserror::Error;

/// Folder holding all Pipeline OS project metadata.
pub const METADATA_DIR: &str = ".pipelineos";

#[derive(Debug, Error)]
pub enum StudioError {
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{path} is not valid: {message}")]
    Parse { path: PathBuf, message: String },
    #[error("invalid identifier {0:?}: use 1-48 lowercase letters, digits, or dashes")]
    InvalidId(String),
    #[error("{0}")]
    Invalid(String),
    #[error("{kind} {id:?} was not found")]
    NotFound { kind: &'static str, id: String },
}

impl StudioError {
    pub(crate) fn io(path: &Path, source: std::io::Error) -> Self {
        Self::Io {
            path: path.to_path_buf(),
            source,
        }
    }
}

pub(crate) fn metadata_path(root: &Path, relative: &str) -> PathBuf {
    root.join(METADATA_DIR).join(relative)
}

/// Ids name files on disk, so they are restricted to a safe, portable charset.
pub fn validate_id(id: &str) -> Result<(), StudioError> {
    let ok = !id.is_empty()
        && id.len() <= 48
        && !id.starts_with('-')
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if ok {
        Ok(())
    } else {
        Err(StudioError::InvalidId(id.to_owned()))
    }
}

/// Lowercase, dash-separated slug of `value`, suitable for an id.
pub fn slugify(value: &str) -> String {
    let mut slug = String::new();
    for c in value.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
        } else if !slug.ends_with('-') && !slug.is_empty() {
            slug.push('-');
        }
    }
    let slug = slug.trim_end_matches('-');
    slug.chars()
        .take(40)
        .collect::<String>()
        .trim_end_matches('-')
        .to_owned()
}

/// Writes `contents` atomically (temp file + rename) so a crash never leaves a
/// half-written metadata file behind.
pub(crate) fn write_atomic(path: &Path, contents: &str) -> Result<(), StudioError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| StudioError::io(parent, e))?;
    }
    let tmp = path.with_extension(format!("tmp-{}", uuid::Uuid::new_v4().simple()));
    std::fs::write(&tmp, contents).map_err(|e| StudioError::io(&tmp, e))?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        StudioError::io(path, e)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_restricted_to_a_safe_charset() {
        assert!(validate_id("build-win64").is_ok());
        assert!(validate_id("a1").is_ok());
        for bad in ["", "../x", "A", "a b", "-lead", "a/b", &"x".repeat(49)] {
            assert!(validate_id(bad).is_err(), "{bad:?} should be rejected");
        }
    }

    #[test]
    fn slugify_produces_valid_ids() {
        assert_eq!(slugify("Build Win64 (Shipping)!"), "build-win64-shipping");
        assert_eq!(slugify("  --  "), "");
        assert!(validate_id(&slugify("Render Shot 010 / Beauty")).is_ok());
    }
}
