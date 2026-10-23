//! In-app updates with visible progress: the download runs in the background
//! (cancellable), its progress is emitted as `update://state`, and installing
//! is a separate, explicit step. On macOS, where an unnotarized app often may
//! not replace itself (App Translocation, a read-only disk image, or App
//! Management protection), the UI offers the installer instead.

use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State, async_runtime::JoinHandle};
use tauri_plugin_updater::{Update, UpdaterExt};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Phase {
    #[default]
    Idle,
    Downloading,
    Ready,
    Installing,
    Restarting,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UpdateState {
    pub phase: Phase,
    pub version: Option<String>,
    pub downloaded: u64,
    pub total: Option<u64>,
    pub error: Option<String>,
    /// A direct installer download (macOS DMG / Windows setup) to fall back
    /// to when the in-place install isn't possible.
    pub installer_url: Option<String>,
    /// Why an in-place install will likely fail on this machine, if known.
    pub blocker: Option<String>,
}

#[derive(Default)]
pub struct UpdateManager {
    state: Mutex<UpdateState>,
    task: Mutex<Option<JoinHandle<()>>>,
    pending: Mutex<Option<(Update, Vec<u8>)>>,
}

fn set(app: &AppHandle, change: impl FnOnce(&mut UpdateState)) {
    let manager = app.state::<UpdateManager>();
    let snapshot = {
        let mut state = manager.state.lock().unwrap();
        change(&mut state);
        state.clone()
    };
    let _ = app.emit("update://state", snapshot);
}

/// The installer that sits next to the updater archive in the release:
/// `X_aarch64.app.tar.gz` -> `X_aarch64.dmg`, `X-setup.exe` stays as is.
pub fn installer_url_for(download_url: &str) -> String {
    if let Some(stem) = download_url.strip_suffix(".app.tar.gz") {
        format!("{stem}.dmg")
    } else {
        download_url.to_owned()
    }
}

/// The running `.app` bundle on macOS.
fn current_bundle(exe: &Path) -> Option<PathBuf> {
    exe.ancestors()
        .find(|ancestor| ancestor.extension().is_some_and(|ext| ext == "app"))
        .map(Path::to_path_buf)
}

/// Explains why replacing the running app in place won't work, before trying.
pub fn macos_install_blocker(exe: &Path) -> Option<String> {
    let path = exe.to_string_lossy();
    if path.contains("/AppTranslocation/") {
        return Some(
            "macOS is running Pipeline OS from a temporary, read-only location (App Translocation). Quit it, drag the app into Applications, and open it from there; updates then install in place.".into(),
        );
    }
    if path.starts_with("/Volumes/") {
        return Some(
            "Pipeline OS is running from the disk image. Drag it into Applications and open it from there to enable updates.".into(),
        );
    }
    let bundle = current_bundle(exe)?;
    let parent = bundle.parent()?;
    let probe = parent.join(".pipelineos-update-probe");
    match std::fs::write(&probe, b"") {
        Ok(()) => {
            let _ = std::fs::remove_file(probe);
            None
        }
        Err(_) => Some(format!(
            "Pipeline OS can't write to {}. Updating will ask for an administrator password, or you can install the new version from its installer.",
            parent.display()
        )),
    }
}

fn friendly_error(error: &str) -> String {
    let lower = error.to_lowercase();
    if lower.contains("signature") {
        "The download didn't match the Pipeline OS signing key, so it wasn't installed.".into()
    } else if lower.contains("read-only")
        || lower.contains("permission")
        || lower.contains("not permitted")
        || lower.contains("move the new app")
    {
        format!(
            "macOS didn't allow Pipeline OS to replace itself ({error}). Install the new version from its installer instead."
        )
    } else if lower.contains("network")
        || lower.contains("connect")
        || lower.contains("timed out")
        || lower.contains("dns")
    {
        format!("The download was interrupted ({error}). Check your connection and try again.")
    } else {
        error.to_owned()
    }
}

#[tauri::command]
pub fn update_state(manager: State<'_, UpdateManager>) -> UpdateState {
    manager.state.lock().unwrap().clone()
}

/// Starts downloading the available update in the background.
#[tauri::command]
pub async fn update_download(app: AppHandle) -> Result<UpdateState, String> {
    let manager = app.state::<UpdateManager>();
    if matches!(
        manager.state.lock().unwrap().phase,
        Phase::Downloading | Phase::Installing
    ) {
        return Ok(manager.state.lock().unwrap().clone());
    }
    let update = app
        .updater()
        .map_err(|error| error.to_string())?
        .check()
        .await
        .map_err(|error| friendly_error(&error.to_string()))?
        .ok_or("You're already on the latest version.")?;
    let blocker = if cfg!(target_os = "macos") {
        std::env::current_exe()
            .ok()
            .and_then(|exe| macos_install_blocker(&exe))
    } else {
        None
    };
    let installer = installer_url_for(update.download_url.as_str());
    let version = update.version.clone();
    set(&app, |state| {
        *state = UpdateState {
            phase: Phase::Downloading,
            version: Some(version),
            installer_url: Some(installer),
            blocker,
            ..UpdateState::default()
        };
    });
    let handle = app.clone();
    let task = tauri::async_runtime::spawn(async move {
        let mut downloaded = 0u64;
        let mut last_emit = 0u64;
        let progress = handle.clone();
        let result = update
            .download(
                move |chunk, total| {
                    downloaded += chunk as u64;
                    // Emit roughly every 256 KiB so the UI stays smooth but quiet.
                    if downloaded - last_emit >= 256 * 1024 || Some(downloaded) == total {
                        last_emit = downloaded;
                        set(&progress, |state| {
                            state.downloaded = downloaded;
                            state.total = total;
                        });
                    }
                },
                || {},
            )
            .await;
        match result {
            Ok(bytes) => {
                *handle.state::<UpdateManager>().pending.lock().unwrap() = Some((update, bytes));
                set(&handle, |state| state.phase = Phase::Ready);
            }
            Err(error) => set(&handle, |state| {
                state.phase = Phase::Failed;
                state.error = Some(friendly_error(&error.to_string()));
            }),
        }
    });
    *manager.task.lock().unwrap() = Some(task);
    Ok(manager.state.lock().unwrap().clone())
}

#[tauri::command]
pub fn update_cancel(app: AppHandle, manager: State<'_, UpdateManager>) {
    if let Some(task) = manager.task.lock().unwrap().take() {
        task.abort();
    }
    manager.pending.lock().unwrap().take();
    set(&app, |state| {
        if matches!(state.phase, Phase::Downloading | Phase::Ready) {
            state.phase = Phase::Cancelled;
            state.downloaded = 0;
        }
    });
}

/// Installs the downloaded update and restarts.
#[tauri::command]
pub async fn update_install(app: AppHandle) -> Result<(), String> {
    let pending = app.state::<UpdateManager>().pending.lock().unwrap().take();
    let Some((update, bytes)) = pending else {
        return Err("Download the update first.".into());
    };
    set(&app, |state| state.phase = Phase::Installing);
    // The macOS installer may show an administrator prompt on the main
    // thread; keep this off the async runtime.
    let result = tauri::async_runtime::spawn_blocking(move || update.install(bytes))
        .await
        .map_err(|error| error.to_string())?;
    match result {
        Ok(()) => {
            set(&app, |state| state.phase = Phase::Restarting);
            app.restart();
        }
        Err(error) => {
            let message = friendly_error(&error.to_string());
            set(&app, |state| {
                state.phase = Phase::Failed;
                state.error = Some(message.clone());
            });
            Err(message)
        }
    }
}

/// Fallback when the in-place install isn't possible: the installer's URL,
/// which the UI opens in the browser to download.
#[tauri::command]
pub fn update_installer_url(manager: State<'_, UpdateManager>) -> Option<String> {
    manager.state.lock().unwrap().installer_url.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installer_url_is_the_dmg_next_to_the_archive() {
        assert_eq!(
            installer_url_for("https://x/dl/PipelineOS.Dev_0.4.13-11_aarch64.app.tar.gz"),
            "https://x/dl/PipelineOS.Dev_0.4.13-11_aarch64.dmg"
        );
        assert_eq!(
            installer_url_for("https://x/dl/A_x64-setup.exe"),
            "https://x/dl/A_x64-setup.exe"
        );
    }

    #[test]
    fn translocated_and_disk_image_launches_are_blocked() {
        assert!(
            macos_install_blocker(Path::new(
                "/private/var/folders/x/AppTranslocation/ABC/d/PipelineOS.app/Contents/MacOS/pos"
            ))
            .is_some()
        );
        assert!(
            macos_install_blocker(Path::new(
                "/Volumes/PipelineOS/PipelineOS.app/Contents/MacOS/pos"
            ))
            .is_some()
        );
    }

    #[test]
    fn errors_are_explained() {
        assert!(friendly_error("Read-only file system (os error 30)").contains("installer"));
        assert!(friendly_error("signature verification failed").contains("signing key"));
    }
}
