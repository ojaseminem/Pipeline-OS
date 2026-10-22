use std::fs;

use vantadeck_detection::{
    DetectionSource, KnownPathDetectionSource, PathDetectionSource, ScanRequest,
};

#[tokio::test]
async fn finds_executable_and_records_evidence() {
    let root = tempfile::tempdir().expect("temp install root");
    let executable = root.path().join("blender.exe");
    write_fixture(&executable);
    let source = PathDetectionSource::new("test-path", 90, vec![root.path().to_path_buf()]);

    let results = source
        .scan(&ScanRequest::new("blender", "Blender", vec!["blender.exe"]))
        .await
        .expect("scan succeeds");

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].executable, executable);
    assert_eq!(results[0].evidence[0].confidence, 90);
}

#[tokio::test]
async fn known_path_source_expands_version_globs() {
    // Mimic a versioned hub layout: <root>/Hub/Editor/<version>/Editor/Unity.exe
    let root = tempfile::tempdir().expect("temp hub root");
    let editor_dir = root.path().join("Hub/Editor/2022.3.18f1/Editor");
    fs::create_dir_all(&editor_dir).expect("editor dir");
    let executable = editor_dir.join("Unity.exe");
    write_fixture(&executable);

    let pattern = format!(
        "{}/Hub/Editor/*/Editor/Unity.exe",
        root.path().display().to_string().replace('\\', "/")
    );
    let source = KnownPathDetectionSource::new("known-path", 90);
    let results = source
        .scan(
            &ScanRequest::new("unity", "Unity", vec!["Unity.exe"]).with_known_paths(vec![pattern]),
        )
        .await
        .expect("scan succeeds");

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].executable, executable);
    assert_eq!(results[0].version.to_string(), "2022.3.18");
}

/// Writes a fixture file that the OS considers executable.
fn write_fixture(path: impl AsRef<std::path::Path>) {
    let path = path.as_ref();
    std::fs::write(path, b"fixture").expect("fixture");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }
}

#[cfg(unix)]
#[tokio::test]
async fn non_executable_files_with_an_app_name_are_ignored() {
    use vantadeck_detection::{DetectionSource, FilesystemDetectionSource, ScanRequest};
    let root = tempfile::tempdir().expect("root");
    // A data file that merely shares the app's name (e.g. a "gimp" palette
    // folder file inside another editor) must not become an installation.
    let resources = root.path().join("Other/share");
    std::fs::create_dir_all(&resources).expect("dirs");
    std::fs::write(resources.join("gimp"), b"palette").expect("data file");
    let source =
        FilesystemDetectionSource::new("filesystem", 80, vec![root.path().to_path_buf()], 6);
    let found = source
        .scan(&ScanRequest::new("gimp", "GIMP", vec!["gimp"]))
        .await
        .expect("scan");
    assert!(found.is_empty(), "{found:?}");
}
