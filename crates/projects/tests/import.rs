use std::fs;

use vantadeck_projects::{import_project, infer_project, load_project, repair_project};

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
fn reimporting_a_configured_project_adopts_the_existing_metadata() {
    let root = tempfile::tempdir().expect("project root");
    import_project(root.path(), Some("Original")).expect("first import");

    // Re-importing (e.g. after removing the project from the local registry,
    // or cloning a teammate's repo that already has the config) must not be
    // blocked — it should load and return the config that's already there.
    let adopted =
        import_project(root.path(), Some("Replacement")).expect("re-import adopts existing");

    assert_eq!(adopted.name, "Original");
    let loaded = load_project(root.path()).expect("original project");
    assert_eq!(loaded.name, "Original");
}

#[test]
fn importing_a_legacy_vantadeck_folder_migrates_it_to_pipelineos() {
    let root = tempfile::tempdir().expect("project root");
    fs::create_dir_all(root.path().join(".vantadeck")).expect("legacy dir");
    fs::write(
        root.path().join(".vantadeck/project.toml"),
        "schema_version = 1\nname = \"Legacy\"\nproject_type = \"general-creative\"\nlinked_apps = []\nlaunch_profiles = []\nshortcuts = []\nenabled_health_checks = []\ntags = []\n",
    )
    .expect("legacy project.toml");

    let adopted = import_project(root.path(), None).expect("import migrates legacy config");

    assert_eq!(adopted.name, "Legacy");
    assert!(root.path().join(".pipelineos/project.toml").is_file());
    assert!(!root.path().join(".vantadeck").exists());
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
fn repairs_missing_project_file_without_losing_prior_config() {
    let root = tempfile::tempdir().expect("project root");
    import_project(root.path(), Some("Northbreak")).expect("initial import");
    // Simulate the PROJECT_CONFIG_INVALID case: the metadata dir survives but
    // project.toml itself is gone (e.g. a bad sync or manual delete).
    fs::remove_file(root.path().join(".pipelineos/project.toml")).expect("remove project file");

    let repaired = repair_project(root.path(), None).expect("repair succeeds");

    assert!(root.path().join(".pipelineos/project.toml").is_file());
    assert_eq!(repaired.project_type, "general-creative");
}

#[test]
fn repairs_corrupt_project_file_by_renaming_it_aside() {
    let root = tempfile::tempdir().expect("project root");
    import_project(root.path(), Some("Northbreak")).expect("initial import");
    fs::write(
        root.path().join(".pipelineos/project.toml"),
        "not valid toml {{{",
    )
    .expect("corrupt file");

    repair_project(root.path(), None).expect("repair succeeds");

    // The corrupt file is preserved, not deleted, and the repaired one loads cleanly.
    assert!(
        root.path()
            .join(".pipelineos/project.toml.broken")
            .is_file()
    );
    vantadeck_projects::load_project(root.path()).expect("repaired project loads");
}

#[test]
fn imports_generic_folder_as_portable_project() {
    let root = tempfile::tempdir().expect("project root");
    let config = import_project(root.path(), Some("References")).expect("import project");

    assert_eq!(config.project_type, "general-creative");
    assert!(root.path().join(".pipelineos/project.toml").is_file());
    let file =
        fs::read_to_string(root.path().join(".pipelineos/project.toml")).expect("project TOML");
    assert!(!file.contains(&root.path().display().to_string()));
}
