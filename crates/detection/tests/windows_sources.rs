use std::{fs, path::Path};

use vantadeck_detection::{
    DetectionSource, EpicLauncherDetectionSource, RegistryDetectionSource, ScanRequest,
    ShortcutDetectionSource, SteamDetectionSource, UnityHubDetectionSource, parse_epic_manifest,
    parse_registry_query, parse_steam_app_manifest, parse_steam_library_folders,
    parse_unity_hub_editors,
};

#[test]
fn parses_registry_query_records() {
    let input = include_str!("fixtures/registry-query.txt");
    let records = parse_registry_query(input);

    assert_eq!(records.len(), 2);
    assert_eq!(records[0].display_name.as_deref(), Some("Blender"));
    assert_eq!(records[0].display_version.as_deref(), Some("4.2.3"));
    assert_eq!(
        records[0].display_icon.as_deref(),
        Some(r#""C:\Program Files\Blender Foundation\Blender 4.2\blender.exe",0"#)
    );
}

#[test]
fn parses_unity_hub_editor_records_and_normalizes_unity_versions() {
    let records = parse_unity_hub_editors(include_str!("fixtures/editors-v2.json"))
        .expect("valid Unity fixture");

    assert_eq!(records.len(), 2);
    assert_eq!(records[0].version.to_string(), "2022.3.45-f1");
    assert!(records[0].location.ends_with("2022.3.45f1"));
}

#[test]
fn parses_unity_hub_data_array_layout() {
    let input = r#"{"schema_version":"v1","data":[{"version":"6000.1.2f1","location":"D:/Unity/6000.1.2f1"}]}"#;
    let records = parse_unity_hub_editors(input).expect("valid Unity data layout");

    assert_eq!(records.len(), 1);
    assert_eq!(records[0].version.to_string(), "6000.1.2-f1");
}

#[test]
fn parses_epic_launcher_manifest() {
    let record =
        parse_epic_manifest(include_str!("fixtures/epic-unreal.item")).expect("valid Epic fixture");

    assert_eq!(record.display_name.as_deref(), Some("Unreal Engine"));
    assert_eq!(record.version.to_string(), "5.4.4");
    assert!(record.launch_executable.ends_with("UnrealEditor.exe"));
}

#[test]
fn parses_steam_library_and_app_manifests() {
    let libraries = parse_steam_library_folders(include_str!("fixtures/libraryfolders.vdf"));
    let app = parse_steam_app_manifest(include_str!("fixtures/appmanifest_123.acf"))
        .expect("valid Steam fixture");

    assert_eq!(libraries.len(), 2);
    assert_eq!(libraries[1], Path::new(r"D:\SteamLibrary"));
    assert_eq!(app.name, "Blender");
    assert_eq!(app.install_dir, "Blender");
}

#[cfg(windows)]
#[test]
fn standard_windows_sources_can_be_composed_without_explicit_paths() {
    let _registry = RegistryDetectionSource::system();
    let _unity = UnityHubDetectionSource::system();
    let _epic = EpicLauncherDetectionSource::system();
    let _steam = SteamDetectionSource::system();
    let _shortcuts = ShortcutDetectionSource::system();
}

#[tokio::test]
async fn all_sources_emit_installations_from_sanitized_fixtures() {
    let root = tempfile::tempdir().expect("fixture root");
    let unity_root = root.path().join("Unity/2022.3.45f1");
    let epic_root = root.path().join("Epic/UE_5.4");
    let steam_root = root.path().join("SteamLibrary");
    let registry_root = root.path().join("Blender 4.2.3");
    for executable in [
        unity_root.join("Editor/Unity.exe"),
        epic_root.join("Engine/Binaries/Win64/UnrealEditor.exe"),
        steam_root.join("steamapps/common/Blender/blender.exe"),
        registry_root.join("blender.exe"),
    ] {
        fs::create_dir_all(executable.parent().expect("parent")).expect("directory");
        fs::write(executable, b"fixture").expect("executable");
    }

    let unity_json = format!(
        r#"{{"2022.3.45f1":{{"version":"2022.3.45f1","location":"{}"}}}}"#,
        slash(&unity_root)
    );
    let unity_file = root.path().join("editors-v2.json");
    fs::write(&unity_file, unity_json).expect("Unity data");

    let epic_json = format!(
        r#"{{"DisplayName":"Unreal Engine","AppVersion":"5.4.4","InstallLocation":"{}","LaunchExecutable":"Engine/Binaries/Win64/UnrealEditor.exe"}}"#,
        slash(&epic_root)
    );
    let epic_dir = root.path().join("Manifests");
    fs::create_dir_all(&epic_dir).expect("Epic manifests");
    fs::write(epic_dir.join("ue.item"), epic_json).expect("Epic data");

    let steamapps = steam_root.join("steamapps");
    fs::create_dir_all(&steamapps).expect("steamapps");
    fs::write(
        steamapps.join("libraryfolders.vdf"),
        format!(
            r#""libraryfolders" {{ "0" {{ "path" "{}" }} }}"#,
            slash(&steam_root)
        ),
    )
    .expect("Steam libraries");
    fs::write(
        steamapps.join("appmanifest_123.acf"),
        include_str!("fixtures/appmanifest_123.acf"),
    )
    .expect("Steam app");

    let registry_text = format!(
        "HKEY_LOCAL_MACHINE\\Software\\Tool\n    DisplayName    REG_SZ    Blender\n    DisplayVersion    REG_SZ    4.2.3\n    InstallLocation    REG_SZ    {}\n",
        registry_root.display()
    );
    let registry = RegistryDetectionSource::from_records(parse_registry_query(&registry_text));

    assert_eq!(
        unity_results(
            UnityHubDetectionSource::new(&unity_file),
            "Unity",
            "Unity.exe"
        )
        .await,
        1
    );
    assert_eq!(
        unity_results(
            EpicLauncherDetectionSource::new(&epic_dir),
            "Unreal Engine",
            "UnrealEditor.exe"
        )
        .await,
        1
    );
    assert_eq!(
        unity_results(
            SteamDetectionSource::new(steamapps.join("libraryfolders.vdf")),
            "Blender",
            "blender.exe"
        )
        .await,
        1
    );
    assert_eq!(unity_results(registry, "Blender", "blender.exe").await, 1);
}

#[tokio::test]
async fn registry_display_name_may_include_an_edition_or_version() {
    let root = tempfile::tempdir().expect("fixture root");
    let executable = root.path().join("blender.exe");
    fs::write(&executable, b"fixture").expect("executable");
    let text = format!(
        "HKEY_LOCAL_MACHINE\\Software\\Blender\n    DisplayName    REG_SZ    Blender 4.2 (64-bit)\n    InstallLocation    REG_SZ    {}\n",
        root.path().display()
    );
    let source = RegistryDetectionSource::from_records(parse_registry_query(&text));

    assert_eq!(unity_results(source, "Blender", "blender.exe").await, 1);
}

#[tokio::test]
async fn parses_shortcut_target_without_invoking_a_shell() {
    let root = tempfile::tempdir().expect("fixture root");
    let target = root.path().join("Tool/tool.exe");
    fs::create_dir_all(target.parent().expect("parent")).expect("target directory");
    fs::write(&target, b"fixture").expect("target executable");
    let shortcut = root.path().join("Tool.lnk");
    fs::write(&shortcut, local_path_shortcut(&target)).expect("shortcut fixture");

    let source = ShortcutDetectionSource::new(vec![root.path().to_path_buf()], 2);
    let results = source
        .scan(&ScanRequest::new("tool", "Tool", vec!["tool.exe"]))
        .await
        .expect("shortcut scan");

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].executable, target);
}

async fn unity_results(
    source: impl DetectionSource,
    display_name: &str,
    executable: &str,
) -> usize {
    source
        .scan(&ScanRequest::new("fixture", display_name, vec![executable]))
        .await
        .expect("source scan")
        .len()
}

fn slash(path: &Path) -> String {
    path.display().to_string().replace('\\', "/")
}

fn local_path_shortcut(target: &Path) -> Vec<u8> {
    let target = target.display().to_string();
    let target = target.as_bytes();
    let header_size = 0x4c_u32;
    let flags = 0x2_u32;
    let link_info_size = 0x1c_u32 + target.len() as u32 + 1;
    let mut bytes = vec![0; header_size as usize];
    bytes[0..4].copy_from_slice(&header_size.to_le_bytes());
    bytes[0x14..0x18].copy_from_slice(&flags.to_le_bytes());
    bytes.extend_from_slice(&link_info_size.to_le_bytes());
    bytes.extend_from_slice(&0x1c_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u32.to_le_bytes());
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    bytes.extend_from_slice(&0x1c_u32.to_le_bytes());
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    bytes.extend_from_slice(target);
    bytes.push(0);
    bytes
}
