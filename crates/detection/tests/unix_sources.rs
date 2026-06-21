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
