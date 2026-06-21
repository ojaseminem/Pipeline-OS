use std::{fs, path::PathBuf};

use vantadeck_detection::{
    DetectionEngine, FilesystemDetectionSource, ManualOverride, ScanRequest,
};
use vantadeck_domain::{AppCategory, AppState};

#[tokio::test]
async fn recursively_groups_versioned_installations() {
    let root = tempfile::tempdir().expect("install root");
    for version in ["4.2.3", "3.6.9"] {
        let directory = root.path().join(format!("Blender {version}"));
        fs::create_dir_all(&directory).expect("version directory");
        fs::write(directory.join("blender.exe"), b"fixture").expect("executable fixture");
    }

    let engine = DetectionEngine::new(vec![Box::new(FilesystemDetectionSource::new(
        "common-root",
        80,
        vec![root.path().to_path_buf()],
        3,
    ))]);
    let application = engine
        .scan_app(
            &ScanRequest::new("blender", "Blender", vec!["blender.exe"]),
            AppCategory::Dcc,
            None,
        )
        .await
        .expect("scan succeeds");

    assert_eq!(application.installations.len(), 2);
    assert_eq!(application.installations[0].version.to_string(), "4.2.3");
    assert_eq!(application.installations[1].version.to_string(), "3.6.9");
}

#[tokio::test]
async fn manual_override_wins_for_the_same_version() {
    let root = tempfile::tempdir().expect("install root");
    let detected = root.path().join("Blender 4.2.3");
    fs::create_dir_all(&detected).expect("version directory");
    fs::write(detected.join("blender.exe"), b"fixture").expect("executable fixture");
    let override_path = PathBuf::from("D:/Portable/Blender/blender.exe");

    let engine = DetectionEngine::new(vec![Box::new(FilesystemDetectionSource::new(
        "common-root",
        80,
        vec![root.path().to_path_buf()],
        3,
    ))]);
    let application = engine
        .scan_app(
            &ScanRequest::new("blender", "Blender", vec!["blender.exe"]),
            AppCategory::Dcc,
            Some(ManualOverride {
                version: "4.2.3".parse().expect("version"),
                executable: override_path.clone(),
            }),
        )
        .await
        .expect("scan succeeds");

    assert_eq!(application.installations.len(), 1);
    assert_eq!(application.installations[0].executable, override_path);
    assert_eq!(
        application.installations[0].state,
        AppState::ManuallyOverridden
    );
}
