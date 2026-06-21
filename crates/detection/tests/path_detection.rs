use std::fs;

use vantadeck_detection::{
    DetectionSource, KnownPathDetectionSource, PathDetectionSource, ScanRequest,
};

#[tokio::test]
async fn finds_executable_and_records_evidence() {
    let root = tempfile::tempdir().expect("temp install root");
    let executable = root.path().join("blender.exe");
    fs::write(&executable, b"fixture").expect("fixture executable");
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
    fs::write(&executable, b"fixture").expect("fixture editor");

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
