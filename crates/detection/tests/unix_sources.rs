use std::{fs, path::Path};

use vantadeck_detection::{
    AppImageDetectionSource, DesktopEntryDetectionSource, DetectionSource,
    ExecutablePathDetectionSource, FlatpakDetectionSource, MacAppBundleDetectionSource,
    ScanRequest, SnapDetectionSource, parse_desktop_entry, parse_flatpak_metadata,
    parse_info_plist,
};

#[test]
fn parses_macos_bundle_metadata() {
    let bundle = parse_info_plist(include_str!("fixtures/Info.plist")).expect("valid plist");

    assert_eq!(bundle.name.as_deref(), Some("Blender"));
    assert_eq!(
        bundle.identifier.as_deref(),
        Some("org.blenderfoundation.blender")
    );
    assert_eq!(bundle.executable, "Blender");
    assert_eq!(bundle.version.to_string(), "4.2.3");
}

#[test]
fn parses_desktop_entry_exec_without_shell_interpretation() {
    let entry =
        parse_desktop_entry(include_str!("fixtures/blender.desktop")).expect("valid desktop entry");

    assert_eq!(entry.name.as_deref(), Some("Blender"));
    assert_eq!(entry.exec[0], "/opt/Blender 4.2/blender");
    assert_eq!(entry.exec[1], "--new-window");
    assert!(!entry.exec.iter().any(|argument| argument == "%F"));
}

#[test]
fn parses_flatpak_application_metadata() {
    let metadata = parse_flatpak_metadata(include_str!("fixtures/flatpak-metadata.ini"))
        .expect("valid Flatpak metadata");

    assert_eq!(metadata.app_id, "org.blender.Blender");
    assert_eq!(metadata.command, "blender");
    assert_eq!(metadata.branch.as_deref(), Some("stable"));
}

#[test]
fn parses_snap_desktop_metadata_and_env_command() {
    let entry =
        parse_desktop_entry(include_str!("fixtures/snap.desktop")).expect("valid Snap desktop");

    assert_eq!(entry.snap_instance.as_deref(), Some("blender"));
    assert_eq!(entry.exec[0], "env");
    assert!(
        entry
            .exec
            .iter()
            .any(|argument| argument == "/snap/bin/blender")
    );
}

#[tokio::test]
async fn mac_bundle_source_finds_bundle_executable() {
    let root = tempfile::tempdir().expect("fixture root");
    let bundle = root.path().join("Blender.app");
    let contents = bundle.join("Contents");
    let executable = contents.join("MacOS/Blender");
    fs::create_dir_all(executable.parent().expect("parent")).expect("bundle directories");
    fs::write(
        contents.join("Info.plist"),
        include_str!("fixtures/Info.plist"),
    )
    .expect("plist");
    fs::write(&executable, b"fixture").expect("bundle executable");

    let source = MacAppBundleDetectionSource::new(vec![root.path().to_path_buf()], 3);
    let results = scan(source, "blender", "Blender", "Blender").await;

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].executable, executable);
    assert_eq!(results[0].version.to_string(), "4.2.3");
}

#[tokio::test]
async fn mac_bundle_source_falls_back_to_common_bundle_layout() {
    let root = tempfile::tempdir().expect("fixture root");
    let bundle = root.path().join("Blender 4.1.2.app");
    let executable = bundle.join("Contents/MacOS/Blender");
    fs::create_dir_all(executable.parent().expect("parent")).expect("bundle directories");
    fs::write(&executable, b"fixture").expect("bundle executable");

    let source = MacAppBundleDetectionSource::new(vec![root.path().to_path_buf()], 3);
    let results = scan(source, "blender", "Blender", "Blender").await;

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].version.to_string(), "4.1.2");
}

#[tokio::test]
async fn desktop_and_snap_sources_resolve_direct_exec_targets() {
    let root = tempfile::tempdir().expect("fixture root");
    let executable = root.path().join("bin/blender");
    fs::create_dir_all(executable.parent().expect("parent")).expect("bin");
    fs::write(&executable, b"fixture").expect("executable");
    let desktop_root = root.path().join("applications");
    fs::create_dir_all(&desktop_root).expect("desktop root");
    fs::write(
        desktop_root.join("blender.desktop"),
        format!(
            "[Desktop Entry]\nName=Blender\nExec=\"{}\" %F\n",
            slash(&executable)
        ),
    )
    .expect("desktop entry");
    let snap_root = root.path().join("snap-applications");
    fs::create_dir_all(&snap_root).expect("snap desktop root");
    fs::write(
        snap_root.join("blender_blender.desktop"),
        format!(
            "[Desktop Entry]\nName=Blender\nExec=env SNAP_NAME=blender \"{}\" %F\nX-SnapInstanceName=blender\n",
            slash(&executable)
        ),
    )
    .expect("snap desktop entry");

    let desktop = DesktopEntryDetectionSource::new(
        vec![desktop_root],
        vec![executable.parent().expect("bin").to_path_buf()],
    );
    let snap = SnapDetectionSource::new(vec![snap_root], vec![]);

    assert_eq!(
        scan(desktop, "blender", "Blender", "blender").await.len(),
        1
    );
    assert_eq!(scan(snap, "blender", "Blender", "blender").await.len(), 1);
}

#[tokio::test]
async fn flatpak_source_uses_deployed_application_command() {
    let root = tempfile::tempdir().expect("fixture root");
    let deployment = root
        .path()
        .join("app/org.blender.Blender/x86_64/stable/deploy-id");
    let executable = deployment.join("files/bin/blender");
    fs::create_dir_all(executable.parent().expect("parent")).expect("deployment");
    fs::write(
        deployment.join("metadata"),
        include_str!("fixtures/flatpak-metadata.ini"),
    )
    .expect("metadata");
    fs::write(&executable, b"fixture").expect("command");

    let source = FlatpakDetectionSource::new(vec![root.path().join("app")], vec![]);
    let results = scan(source, "org.blender.Blender", "Blender", "blender").await;

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].executable, executable);
}

#[tokio::test]
async fn flatpak_source_reads_exported_desktop_evidence() {
    let root = tempfile::tempdir().expect("fixture root");
    let launcher = root.path().join("bin/flatpak");
    fs::create_dir_all(launcher.parent().expect("parent")).expect("bin");
    fs::write(&launcher, b"fixture").expect("flatpak launcher");
    let exports = root.path().join("exports/share/applications");
    fs::create_dir_all(&exports).expect("exports");
    fs::write(
        exports.join("org.blender.Blender.desktop"),
        format!(
            "[Desktop Entry]\nName=Blender\nExec=\"{}\" run org.blender.Blender %U\nX-Flatpak=org.blender.Blender\n",
            slash(&launcher)
        ),
    )
    .expect("Flatpak export");

    let source = FlatpakDetectionSource::new(vec![], vec![exports]);
    let results = scan(source, "org.blender.Blender", "Blender", "blender").await;

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].executable, launcher);
    assert_eq!(results[0].evidence[0].source, "flatpak-export");
}

#[tokio::test]
async fn path_and_appimage_sources_find_portable_executables() {
    let root = tempfile::tempdir().expect("fixture root");
    let bin = root.path().join("bin");
    let applications = root.path().join("Applications");
    fs::create_dir_all(&bin).expect("bin");
    fs::create_dir_all(&applications).expect("applications");
    let executable = bin.join("blender");
    let appimage = applications.join("Blender-4.2.3-x86_64.AppImage");
    fs::write(&executable, b"fixture").expect("path executable");
    fs::write(&appimage, b"fixture").expect("AppImage");

    let path_source = ExecutablePathDetectionSource::new(vec![bin]);
    let appimage_source = AppImageDetectionSource::new(vec![applications], 2);

    assert_eq!(
        scan(path_source, "blender", "Blender", "blender")
            .await
            .len(),
        1
    );
    let results = scan(appimage_source, "blender", "Blender", "blender").await;
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].executable, appimage);
    assert_eq!(results[0].version.to_string(), "4.2.3");
}

#[cfg(target_os = "macos")]
#[test]
fn macos_system_sources_can_be_composed() {
    let _bundles = MacAppBundleDetectionSource::system();
    let _path = ExecutablePathDetectionSource::system();
}

#[cfg(target_os = "linux")]
#[test]
fn linux_system_sources_can_be_composed() {
    let _desktop = DesktopEntryDetectionSource::system();
    let _path = ExecutablePathDetectionSource::system();
    let _flatpak = FlatpakDetectionSource::system();
    let _snap = SnapDetectionSource::system();
    let _appimage = AppImageDetectionSource::system();
}

async fn scan(
    source: impl DetectionSource,
    app_id: &str,
    display_name: &str,
    executable: &str,
) -> Vec<vantadeck_domain::AppInstallation> {
    source
        .scan(&ScanRequest::new(app_id, display_name, vec![executable]))
        .await
        .expect("source scan")
}

fn slash(path: &Path) -> String {
    path.display().to_string().replace('\\', "/")
}

/// Writes a fake `.app` bundle with a *binary* Info.plist, like most real
/// macOS apps ship.
fn fake_bundle(
    parent: &Path,
    bundle: &str,
    identifier: &str,
    executable: &str,
    version: &str,
) -> std::path::PathBuf {
    let contents = parent.join(bundle).join("Contents");
    let binary = contents.join("MacOS").join(executable);
    fs::create_dir_all(binary.parent().expect("parent")).expect("bundle dirs");
    fs::write(&binary, b"fixture").expect("binary");
    let mut dict = plist::Dictionary::new();
    dict.insert("CFBundleIdentifier".into(), identifier.into());
    dict.insert("CFBundleExecutable".into(), executable.into());
    dict.insert(
        "CFBundleName".into(),
        bundle.trim_end_matches(".app").into(),
    );
    dict.insert("CFBundleShortVersionString".into(), version.into());
    plist::Value::Dictionary(dict)
        .to_file_binary(contents.join("Info.plist"))
        .expect("binary plist");
    binary
}

#[tokio::test]
async fn mac_bundles_match_by_identifier_deeply_and_skip_unity_hub() {
    let root = tempfile::tempdir().expect("root");
    let editor = fake_bundle(
        &root.path().join("Unity/Hub/Editor/2022.3.18f1"),
        "Unity.app",
        "com.unity3d.UnityEditor5.x",
        "Unity",
        "Unity version 2022.3.18f1",
    );
    fake_bundle(
        root.path(),
        "Unity Hub.app",
        "com.unity3d.unityhub",
        "Unity Hub",
        "3.9.1",
    );
    let source = MacAppBundleDetectionSource::new(vec![root.path().to_path_buf()], 6);
    let request = ScanRequest::new("unity", "Unity", vec!["Unity.exe", "Unity"])
        .with_mac_bundle_ids(vec!["com.unity3d.UnityEditor"]);
    let results = source.scan(&request).await.expect("scan");
    assert_eq!(results.len(), 1, "only the editor, not Unity Hub");
    assert_eq!(results[0].executable, editor);
    assert_eq!(results[0].version.major, 2022);
}

#[tokio::test]
async fn mac_bundles_use_the_declared_executable_not_the_windows_name() {
    let root = tempfile::tempdir().expect("root");
    let binary = fake_bundle(
        &root.path().join("Adobe Photoshop 2025"),
        "Adobe Photoshop 2025.app",
        "com.adobe.Photoshop",
        "Adobe Photoshop 2025",
        "26.0.0",
    );
    let source = MacAppBundleDetectionSource::new(vec![root.path().to_path_buf()], 6);
    let request = ScanRequest::new("photoshop", "Adobe Photoshop", vec!["Photoshop.exe"])
        .with_mac_bundle_ids(vec!["com.adobe.Photoshop"]);
    let results = source.scan(&request).await.expect("scan");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].executable, binary);
    assert_eq!(results[0].version.to_string(), "26.0.0");
}

#[tokio::test]
async fn mac_bundles_match_product_names_without_the_vendor() {
    let root = tempfile::tempdir().expect("root");
    fake_bundle(
        &root.path().join("Houdini/Houdini20.5.410"),
        "Houdini FX 20.5.410.app",
        "com.sidefx.houdinifx",
        "houdinifx",
        "20.5.410",
    );
    fake_bundle(
        root.path(),
        "Calculator.app",
        "com.apple.calculator",
        "Calculator",
        "11.0",
    );
    let source = MacAppBundleDetectionSource::new(vec![root.path().to_path_buf()], 6);
    let results = scan(source, "houdini", "SideFX Houdini", "houdinifx.exe").await;
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].version.to_string(), "20.5.410");
}

#[tokio::test]
async fn known_paths_resolve_app_bundles_to_their_executable() {
    let root = tempfile::tempdir().expect("root");
    let binary = fake_bundle(
        root.path(),
        "Spine.app",
        "com.esotericsoftware.spine",
        "Spine",
        "4.2.0",
    );
    let pattern = format!("{}/Spine.app", slash(root.path()));
    let source = vantadeck_detection::KnownPathDetectionSource::new("known-path", 90);
    let request =
        ScanRequest::new("spine", "Spine", vec!["Spine.exe"]).with_known_paths(vec![pattern]);
    let results = source.scan(&request).await.expect("scan");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].executable, binary);
}
