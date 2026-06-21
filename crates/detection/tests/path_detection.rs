use std::fs;

use vantadeck_detection::{DetectionSource, PathDetectionSource, ScanRequest};

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
