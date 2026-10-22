//! Saving user-requested exports (themes, briefs) to a path the user picked
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

#[tauri::command]
pub async fn save_text_file(path: String, contents: String) -> Result<(), String> {
    let path = Path::new(&path);
    if !allowed(path) {
        return Err(format!(
            "Only {} files can be saved here.",
            ALLOWED_EXTENSIONS.join(", ")
        ));
    }
    std::fs::write(path, contents).map_err(|error| error.to_string())
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
