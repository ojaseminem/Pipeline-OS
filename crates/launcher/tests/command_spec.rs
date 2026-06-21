use std::path::PathBuf;

use semver::Version;
use vantadeck_domain::{AppInstallation, AppState, DetectionEvidence, LaunchProfile};
use vantadeck_launcher::{LaunchSpec, resolve_launch_profile};

#[test]
fn launch_spec_preserves_arguments_without_shell_interpolation() {
    let spec = LaunchSpec::new(
        PathBuf::from("C:/Apps/Blender/blender.exe"),
        vec!["D:/Projects/My Game/scene.blend".into()],
        PathBuf::from("D:/Projects/My Game"),
    )
    .expect("structured command");

    assert_eq!(spec.arguments.len(), 1);
    assert_eq!(spec.arguments[0], "D:/Projects/My Game/scene.blend");
}

#[test]
fn rejects_empty_executable() {
    assert!(LaunchSpec::new(PathBuf::new(), Vec::new(), PathBuf::from(".")).is_err());
}

#[test]
fn resolves_preferred_version_and_portable_placeholders() {
    let root = tempfile::tempdir().expect("project");
    let work = root.path().join("Game");
    std::fs::create_dir(&work).expect("working directory");
    let preferred = root.path().join("Unity-2022.exe");
    let latest = root.path().join("Unity-6.exe");
    std::fs::write(&preferred, b"fixture").expect("preferred executable");
    std::fs::write(&latest, b"fixture").expect("latest executable");
    let profile = LaunchProfile {
        id: "editor".into(),
        name: "Editor".into(),
        app_id: "unity".into(),
        arguments: vec!["-projectPath".into(), "{projectRoot}/Game".into()],
        working_directory: Some("Game".into()),
        preferred_version: Some("^2022.3".into()),
        fallback_version: Some("6000.0.0".into()),
    };
    let installations = vec![
        installation("6000.0.0", latest),
        installation("2022.3.18", preferred.clone()),
    ];

    let resolved =
        resolve_launch_profile(root.path(), &profile, &installations).expect("resolved profile");

    assert_eq!(resolved.executable, preferred);
    assert_eq!(resolved.working_directory, work);
    assert_eq!(
        resolved.arguments[1],
        format!("{}/Game", root.path().display())
    );
}

#[test]
fn uses_explicit_fallback_when_preferred_version_is_missing() {
    let root = tempfile::tempdir().expect("project");
    let executable = root.path().join("Unity-2021.exe");
    std::fs::write(&executable, b"fixture").expect("executable");
    let profile = LaunchProfile {
        id: "editor".into(),
        name: "Editor".into(),
        app_id: "unity".into(),
        arguments: Vec::new(),
        working_directory: Some(".".into()),
        preferred_version: Some("^2022.3".into()),
        fallback_version: Some("2021.3.1".into()),
    };

    let resolved = resolve_launch_profile(
        root.path(),
        &profile,
        &[installation("2021.3.1", executable.clone())],
    )
    .expect("fallback");
    assert_eq!(resolved.executable, executable);
}

fn installation(version: &str, executable: PathBuf) -> AppInstallation {
    AppInstallation {
        version: Version::parse(version).expect("version"),
        executable,
        state: AppState::Installed,
        evidence: vec![DetectionEvidence {
            source: "test".into(),
            detail: "fixture".into(),
            confidence: 100,
        }],
    }
}
