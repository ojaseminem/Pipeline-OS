//! Saving user-requested exports (themes, briefs) to a path the user picks
//! in the native save dialog. Limited to known text formats.

use std::path::Path;

const ALLOWED_EXTENSIONS: &[&str] = &["postheme", "md", "json", "txt"];

fn allowed(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            ALLOWED_EXTENSIONS
                .iter()
                .any(|allowed| extension.eq_ignore_ascii_case(allowed))
        })
}

/// Asks where to save with the native dialog and writes there. The path never
/// comes from the webview, so a script can't pick where files are written.
#[tauri::command(rename_all = "camelCase")]
pub async fn save_text_file<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    default_name: String,
    filter_name: String,
    extensions: Vec<String>,
    contents: String,
) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let picked = tauri::async_runtime::spawn_blocking(move || {
        let extensions: Vec<&str> = extensions.iter().map(String::as_str).collect();
        app.dialog()
            .file()
            .set_file_name(default_name)
            .add_filter(filter_name, &extensions)
            .blocking_save_file()
    })
    .await
    .map_err(|error| error.to_string())?;
    let Some(picked) = picked else {
        return Ok(None);
    };
    let path = picked.into_path().map_err(|error| error.to_string())?;
    if !allowed(&path) {
        return Err(format!(
            "Only {} files can be saved here.",
            ALLOWED_EXTENSIONS.join(", ")
        ));
    }
    std::fs::write(&path, contents).map_err(|error| error.to_string())?;
    Ok(Some(path.display().to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_known_text_formats_are_allowed() {
        assert!(allowed(Path::new("/tmp/My Theme.postheme")));
        assert!(allowed(Path::new("C:/briefs/context.MD")));
        assert!(!allowed(Path::new("/tmp/run.sh")));
        assert!(!allowed(Path::new("/tmp/noext")));
    }
}
