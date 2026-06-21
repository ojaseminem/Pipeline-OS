use std::fs;

use vantadeck_projects::{ProjectError, import_project, infer_project};

#[test]
fn infers_unity_version_from_project_settings() {
    let root = tempfile::tempdir().expect("project root");
    let settings = root.path().join("ProjectSettings");
    fs::create_dir_all(&settings).expect("settings directory");
    fs::write(
        settings.join("ProjectVersion.txt"),
        "m_EditorVersion: 2022.3.18f1\n",
    )
    .expect("version file");

    let config = infer_project(root.path(), Some("Voidline")).expect("inferred project");

    assert_eq!(config.project_type, "game-development");
    assert_eq!(config.linked_apps.len(), 1);
    assert_eq!(config.linked_apps[0].app_id, "unity");
    assert_eq!(
        config.linked_apps[0].preferred_version.as_deref(),
        Some("2022.3.18f1")
    );
    assert_eq!(config.linked_apps[0].folder.as_deref(), Some("."));
}

#[test]
fn refuses_to_overwrite_existing_project_metadata() {
    let root = tempfile::tempdir().expect("project root");
    import_project(root.path(), Some("Original")).expect("first import");

    let error = import_project(root.path(), Some("Replacement"))
        .expect_err("existing project metadata must be preserved");

    assert!(matches!(error, ProjectError::AlreadyExists));
    let loaded = vantadeck_projects::load_project(root.path()).expect("original project");
    assert_eq!(loaded.name, "Original");
}

#[test]
fn infers_unreal_project_and_blender_source_folder() {
    let root = tempfile::tempdir().expect("project root");
    fs::write(root.path().join("Riftbound.uproject"), "{}").expect("uproject");
    let art = root.path().join("Art/Source");
    fs::create_dir_all(&art).expect("art source");
    fs::write(art.join("Character.blend"), b"fixture").expect("blend fixture");

    let config = infer_project(root.path(), Some("Riftbound")).expect("inferred project");

    let unreal = config
        .linked_apps
        .iter()
        .find(|app| app.app_id == "unreal-engine")
        .expect("unreal link");
    assert_eq!(unreal.project_file.as_deref(), Some("Riftbound.uproject"));
    let blender = config
        .linked_apps
        .iter()
        .find(|app| app.app_id == "blender")
        .expect("blender link");
    assert_eq!(blender.folder.as_deref(), Some("Art/Source"));
}

#[test]
fn imports_generic_folder_as_portable_project() {
    let root = tempfile::tempdir().expect("project root");
    let config = import_project(root.path(), Some("References")).expect("import project");

    assert_eq!(config.project_type, "general-creative");
    assert!(root.path().join(".vantadeck/project.toml").is_file());
    let file =
        fs::read_to_string(root.path().join(".vantadeck/project.toml")).expect("project TOML");
    assert!(!file.contains(&root.path().display().to_string()));
}
