//! Apps started from Finder, the Dock, or a desktop launcher get a minimal
//! PATH (`/usr/bin:/bin:/usr/sbin:/sbin` on macOS), so tools installed with
//! Homebrew, MacPorts, pipx, or a version manager (git-lfs, gh, claude,
//! codex...) look "not installed". Recover the user's login-shell PATH once at
//! startup, then add the usual install locations as a fallback.

use std::{
    path::PathBuf,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

/// Locations that commonly hold developer tools on macOS and Linux.
fn common_dirs(home: Option<&PathBuf>) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = [
        "/opt/homebrew/bin",
        "/opt/homebrew/sbin",
        "/usr/local/bin",
        "/usr/local/sbin",
        "/opt/local/bin",
        "/usr/local/git/bin",
        "/Library/Apple/usr/bin",
        "/snap/bin",
    ]
    .into_iter()
    .map(PathBuf::from)
    .collect();
    if let Some(home) = home {
        for relative in [
            ".local/bin",
            "bin",
            ".cargo/bin",
            ".volta/bin",
            ".bun/bin",
            ".npm-global/bin",
        ] {
            dirs.push(home.join(relative));
        }
    }
    dirs
}

/// The login shell's PATH, or None if it can't be read within two seconds.
fn login_shell_path() -> Option<String> {
    let shell = std::env::var("SHELL")
        .ok()
        .filter(|shell| !shell.is_empty())?;
    let mut child = Command::new(shell)
        .args(["-ilc", "printf '__PATH__%s__PATH__' \"$PATH\""])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                return None;
            }
        }
    }
    let mut output = String::new();
    std::io::Read::read_to_string(&mut child.stdout.take()?, &mut output).ok()?;
    // Shell profiles may print banners; take only what's between the markers.
    let start = output.find("__PATH__")? + "__PATH__".len();
    let end = output[start..].find("__PATH__")? + start;
    Some(output[start..end].to_owned()).filter(|path| !path.is_empty())
}

/// Merges `extra` directories into `current`, keeping order and skipping
/// duplicates and missing directories.
pub fn merged_path(
    current: &str,
    extra: impl IntoIterator<Item = PathBuf>,
    exists: impl Fn(&PathBuf) -> bool,
) -> String {
    let mut seen: Vec<PathBuf> = std::env::split_paths(current).collect();
    for dir in extra {
        if !seen.contains(&dir) && exists(&dir) {
            seen.push(dir);
        }
    }
    std::env::join_paths(seen)
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|_| current.to_owned())
}

/// Call once, early, before any threads read the environment.
pub fn fix_path() {
    if cfg!(windows) {
        return;
    }
    let current = std::env::var("PATH").unwrap_or_default();
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let shell_dirs: Vec<PathBuf> = login_shell_path()
        .map(|path| std::env::split_paths(&path).collect())
        .unwrap_or_default();
    let merged = merged_path(
        &current,
        shell_dirs.into_iter().chain(common_dirs(home.as_ref())),
        |dir| dir.is_dir(),
    );
    // SAFETY: called from `run()` before the async runtime or any other
    // thread is started, so nothing reads the environment concurrently.
    unsafe { std::env::set_var("PATH", merged) };
}

// PATH uses `:` separators only on unix, where this module does its work.
#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn merges_without_duplicates_and_skips_missing() {
        let merged = merged_path(
            "/usr/bin:/bin",
            [
                PathBuf::from("/opt/homebrew/bin"),
                PathBuf::from("/usr/bin"),
                PathBuf::from("/nope"),
            ],
            |dir| dir != &PathBuf::from("/nope"),
        );
        assert_eq!(merged, "/usr/bin:/bin:/opt/homebrew/bin");
    }
}
