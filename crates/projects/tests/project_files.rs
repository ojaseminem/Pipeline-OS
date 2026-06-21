use vantadeck_domain::ProjectConfig;
use vantadeck_projects::{
    ProjectError, load_project, load_project_versioned, save_project, save_project_if_unchanged,
};

#[test]
fn saves_and_loads_canonical_project_toml() {
    let root = tempfile::tempdir().expect("temp project");
    let config = ProjectConfig {
        schema_version: 1,
        name: "Voidline".into(),
        project_type: "game-development".into(),
        linked_apps: Vec::new(),
        launch_profiles: Vec::new(),
        shortcuts: Vec::new(),
        version_control: None,
        enabled_health_checks: vec!["project-path".into()],
    };

    save_project(root.path(), &config).expect("saved atomically");
    let loaded = load_project(root.path()).expect("loaded project");

    assert_eq!(loaded, config);
    assert!(root.path().join(".vantadeck/project.toml").is_file());
}

#[test]
fn refuses_to_overwrite_an_external_project_edit() {
    let root = tempfile::tempdir().expect("temp project");
    let mut config = project("Voidline");
    save_project(root.path(), &config).expect("initial save");
    let loaded = load_project_versioned(root.path()).expect("versioned load");

    let external = project("Externally Renamed");
    save_project(root.path(), &external).expect("external save");
    config.name = "Local Rename".into();

    let error = save_project_if_unchanged(root.path(), &config, &loaded.revision)
        .expect_err("stale edit must be refused");
    assert!(matches!(error, ProjectError::ExternallyModified));
    assert_eq!(
        load_project(root.path())
            .expect("external edit remains")
            .name,
        "Externally Renamed"
    );
    let conflict = std::fs::read_to_string(
        root.path()
            .join(".vantadeck/project.toml.vantadeck-conflict"),
    )
    .expect("local proposal is preserved");
    assert!(conflict.contains("name = \"Local Rename\""));
}

#[test]
fn recovers_backup_left_by_an_interrupted_save() {
    let root = tempfile::tempdir().expect("temp project");
    save_project(root.path(), &project("Before Interruption")).expect("initial save");
    let directory = root.path().join(".vantadeck");
    std::fs::rename(
        directory.join("project.toml"),
        directory.join("project.toml.bak"),
    )
    .expect("simulate interrupted swap");

    let loaded = load_project(root.path()).expect("backup recovered");

    assert_eq!(loaded.name, "Before Interruption");
    assert!(directory.join("project.toml").is_file());
    assert!(!directory.join("project.toml.bak").exists());
}

#[test]
fn saves_when_the_expected_revision_is_current() {
    let root = tempfile::tempdir().expect("temp project");
    let config = project("Voidline");
    save_project(root.path(), &config).expect("initial save");
    let loaded = load_project_versioned(root.path()).expect("versioned load");
    let updated = project("Updated");

    let revision = save_project_if_unchanged(root.path(), &updated, &loaded.revision)
        .expect("compare and save");

    assert_ne!(revision, loaded.revision);
    assert_eq!(load_project(root.path()).expect("updated").name, "Updated");
}

fn project(name: &str) -> ProjectConfig {
    ProjectConfig {
        schema_version: 1,
        name: name.into(),
        project_type: "game-development".into(),
        linked_apps: Vec::new(),
        launch_profiles: Vec::new(),
        shortcuts: Vec::new(),
        version_control: None,
        enabled_health_checks: vec!["project-path".into()],
    }
}
