use std::fs;

use vantadeck_application::ApplicationService;
use vantadeck_domain::{LaunchProfile, LinkedApp};
use vantadeck_projects::{load_project, save_project};
use vantadeck_storage::Storage;
use vantadeck_vcs::GitProvider;

#[tokio::test]
async fn scans_catalog_with_manual_override_and_persists_results() {
    let manifests = tempfile::tempdir().expect("manifest directory");
    fs::write(
        manifests.path().join("blender.json"),
        r#"{
          "schemaVersion":1,"id":"blender","name":"Blender","category":"dcc",
          "platforms":["windows"],"executables":["blender.exe"],
          "launchTemplates":[]
        }"#,
    )
    .expect("manifest");
    let installs = tempfile::tempdir().expect("install root");
    let detected = installs.path().join("Blender 4.2.3");
    fs::create_dir_all(&detected).expect("detected directory");
    fs::write(detected.join("blender.exe"), b"fixture").expect("detected executable");
    let portable = installs.path().join("Portable/blender.exe");
    fs::create_dir_all(portable.parent().expect("portable parent")).expect("portable directory");
    fs::write(&portable, b"fixture").expect("portable executable");

    let storage = Storage::connect("sqlite::memory:").await.expect("storage");
    let service = ApplicationService::new(storage, GitProvider::new("git"));
    service
        .set_manual_override("blender", "4.2.3".parse().expect("version"), &portable)
        .await
        .expect("manual override");
    let applications = service
        .scan_apps(manifests.path(), &[installs.path().to_path_buf()])
        .await
        .expect("scan applications");

    assert_eq!(applications.len(), 1);
    assert_eq!(applications[0].installations.len(), 1);
    assert_eq!(applications[0].installations[0].executable, portable);
    assert_eq!(
        service
            .detected_installations("blender")
            .await
            .expect("persisted detections")
            .len(),
        1
    );
}

#[tokio::test]
async fn imports_and_registers_project_with_activity() {
    let root = tempfile::tempdir().expect("project root");
    let storage = Storage::connect("sqlite::memory:").await.expect("storage");
    let service = ApplicationService::new(storage, GitProvider::new("git"));

    let project = service
        .import_project(root.path(), Some("Voidline"))
        .await
        .expect("import project");

    assert_eq!(project.name, "Voidline");
    assert_eq!(
        service.registered_projects().await.expect("projects").len(),
        1
    );
    assert_eq!(
        service.recent_activity(10).await.expect("activity").len(),
        1
    );
}

#[tokio::test]
async fn removed_project_can_be_reimported_without_error() {
    let root = tempfile::tempdir().expect("project root");
    let storage = Storage::connect("sqlite::memory:").await.expect("storage");
    let service = ApplicationService::new(storage, GitProvider::new("git"));

    service
        .import_project(root.path(), Some("Voidline"))
        .await
        .expect("first import");
    assert_eq!(service.registered_projects().await.unwrap().len(), 1);

    service
        .remove_project(root.path())
        .await
        .expect("remove project");
    assert_eq!(service.registered_projects().await.unwrap().len(), 0);
    // The on-disk config from the first import must still be there — removal
    // only unregisters, it never touches files.
    assert!(root.path().join(".pipelineos/project.toml").is_file());

    // Re-importing the same folder must succeed (not AlreadyExists) and adopt
    // the config that's still on disk rather than erroring or overwriting it.
    let readopted = service
        .import_project(root.path(), Some("Ignored Name"))
        .await
        .expect("re-import after removal");
    assert_eq!(readopted.name, "Voidline");
    assert_eq!(service.registered_projects().await.unwrap().len(), 1);
}

#[tokio::test]
async fn import_gitignores_local_config_and_touch_rechecks_it() {
    let root = tempfile::tempdir().expect("project root");
    fs::write(root.path().join(".gitignore"), "target/\n").expect("seed gitignore");
    let storage = Storage::connect("sqlite::memory:").await.expect("storage");
    let service = ApplicationService::new(storage, GitProvider::new("git"));

    service
        .import_project(root.path(), Some("Voidline"))
        .await
        .expect("import project");

    let gitignore = fs::read_to_string(root.path().join(".gitignore")).expect("gitignore");
    assert!(gitignore.contains("/.pipelineos/local.json"));

    // Simulate git being set up (or .gitignore edited) after the project was
    // already registered: the entry should reappear on next open even if
    // something stripped it out.
    fs::write(root.path().join(".gitignore"), "target/\n").expect("strip entry");
    service
        .touch_project_opened(root.path())
        .await
        .expect("touch opened");
    let gitignore = fs::read_to_string(root.path().join(".gitignore")).expect("gitignore");
    assert!(gitignore.contains("/.pipelineos/local.json"));
}

#[tokio::test]
async fn validates_and_serves_the_tool_index_offline() {
    let storage = Storage::connect("sqlite::memory:").await.expect("storage");
    let service = ApplicationService::new(storage, GitProvider::new("git"));
    let source = "https://tools.vantadeck.org/v1/index.json";
    let index = r#"[{
      "schemaVersion":1,
      "id":"mesh-helper",
      "name":"Mesh Helper",
      "description":"Mesh workflow helper.",
      "sourceUrl":"https://github.com/example/mesh-helper",
      "license":"MIT",
      "supportedHosts":["blender"],
      "platforms":["windows"],
      "provenance":"Upstream release.",
      "reviewState":"reviewed",
      "lastVerifiedAt":"2026-06-21",
      "safetyNotes":"Review before installing.",
      "artifacts":[]
    }]"#;

    service
        .cache_tool_index(source, index, Some("etag-1"))
        .await
        .expect("validated cache");
    let cached = service.cached_tools(source).await.expect("offline tools");
    assert_eq!(cached.len(), 1);
    assert_eq!(cached[0].id, "mesh-helper");
}

#[tokio::test]
async fn searches_and_pins_registered_projects() {
    let first = tempfile::tempdir().expect("first project");
    let second = tempfile::tempdir().expect("second project");
    let storage = Storage::connect("sqlite::memory:").await.expect("storage");
    let service = ApplicationService::new(storage, GitProvider::new("git"));
    service
        .import_project(first.path(), Some("Voidline"))
        .await
        .expect("first import");
    service
        .import_project(second.path(), Some("Emberfall"))
        .await
        .expect("second import");

    assert_eq!(service.search_projects("void", 10).await.unwrap().len(), 1);
    service
        .set_project_pinned(first.path(), true)
        .await
        .expect("pin project");
    assert!(service.registered_projects().await.unwrap()[0].pinned);
}

#[tokio::test]
async fn resolves_a_project_launch_profile_from_local_installations() {
    let root = tempfile::tempdir().expect("project root");
    let executable = root.path().join("blender.exe");
    std::fs::write(&executable, b"fixture").expect("executable");
    let storage = Storage::connect("sqlite::memory:").await.expect("storage");
    let service = ApplicationService::new(storage, GitProvider::new("git"));
    service
        .import_project(root.path(), Some("Art Project"))
        .await
        .expect("import");
    let mut config = load_project(root.path()).expect("project");
    config.launch_profiles.push(LaunchProfile {
        id: "blender".into(),
        name: "Open Blender".into(),
        app_id: "blender".into(),
        arguments: vec!["{projectRoot}".into()],
        working_directory: Some(".".into()),
        preferred_version: Some("4.2.3".into()),
        fallback_version: None,
    });
    save_project(root.path(), &config).expect("profile");
    service
        .set_manual_override("blender", "4.2.3".parse().unwrap(), &executable)
        .await
        .expect("override");

    let launch = service
        .resolve_project_launch(root.path(), "blender")
        .await
        .expect("resolved launch");
    assert_eq!(launch.executable, executable);
    assert_eq!(launch.arguments, vec![root.path().display().to_string()]);
}

#[tokio::test]
async fn health_reports_missing_linked_apps_and_broken_profiles() {
    let root = tempfile::tempdir().expect("project root");
    let storage = Storage::connect("sqlite::memory:").await.expect("storage");
    let service = ApplicationService::new(storage, GitProvider::new("git"));
    service
        .import_project(root.path(), Some("Missing Tools"))
        .await
        .expect("import");
    let mut config = load_project(root.path()).expect("project");
    config.linked_apps.push(LinkedApp {
        app_id: "unity".into(),
        preferred_version: Some("2022.3.18".into()),
        project_file: None,
        folder: Some(".".into()),
    });
    config.launch_profiles.push(LaunchProfile {
        id: "editor".into(),
        name: "Editor".into(),
        app_id: "unity".into(),
        arguments: Vec::new(),
        working_directory: Some("Missing".into()),
        preferred_version: Some("2022.3.18".into()),
        fallback_version: None,
    });
    save_project(root.path(), &config).expect("save project");

    let issues = service.project_health(root.path()).await;
    assert!(issues.iter().any(|issue| issue.code == "APP_NOT_INSTALLED"));
    assert!(
        issues
            .iter()
            .any(|issue| issue.code == "LAUNCH_PROFILE_BROKEN")
    );
}

#[tokio::test]
async fn scan_projects_finds_and_registers_unregistered_and_legacy_projects() {
    let drive = tempfile::tempdir().expect("fake drive root");
    let known = drive.path().join("Known");
    let unknown = drive.path().join("Unknown");
    let legacy = drive.path().join("Legacy");
    fs::create_dir_all(&known).unwrap();
    fs::create_dir_all(&unknown).unwrap();
    fs::create_dir_all(&legacy).unwrap();

    let storage = Storage::connect("sqlite::memory:").await.expect("storage");
    let service = ApplicationService::new(storage, GitProvider::new("git"));
    service
        .import_project(&known, Some("Known"))
        .await
        .expect("pre-register one project");

    // A project never registered on this machine (e.g. re-plugged drive).
    fs::create_dir_all(unknown.join(".pipelineos")).unwrap();
    fs::write(
        unknown.join(".pipelineos/project.toml"),
        "schema_version = 1\nname = \"Unknown\"\nproject_type = \"general-creative\"\nlinked_apps = []\nlaunch_profiles = []\nshortcuts = []\nenabled_health_checks = []\ntags = []\n",
    )
    .unwrap();

    // A project with only the legacy folder, which the scan should migrate.
    fs::create_dir_all(legacy.join(".vantadeck")).unwrap();
    fs::write(
        legacy.join(".vantadeck/project.toml"),
        "schema_version = 1\nname = \"Legacy\"\nproject_type = \"general-creative\"\nlinked_apps = []\nlaunch_profiles = []\nshortcuts = []\nenabled_health_checks = []\ntags = []\n",
    )
    .unwrap();

    let found = service
        .scan_projects_with_progress(&[drive.path().to_path_buf()], |_| {}, || false)
        .await
        .expect("scan for projects");

    let names: Vec<_> = found.iter().map(|project| project.name.clone()).collect();
    assert!(names.contains(&"Unknown".to_string()));
    assert!(names.contains(&"Legacy".to_string()));
    assert!(!names.contains(&"Known".to_string())); // already registered, not re-reported
    assert!(legacy.join(".pipelineos/project.toml").is_file()); // migrated in place
    assert_eq!(service.registered_projects().await.unwrap().len(), 3);
}

#[tokio::test]
async fn rejects_vcs_mutation_without_shared_service_confirmation() {
    let root = tempfile::tempdir().expect("repository root");
    let storage = Storage::connect("sqlite::memory:").await.expect("storage");
    let service = ApplicationService::new(storage, GitProvider::new("git"));

    let error = service
        .vcs_sync(root.path(), false)
        .await
        .expect_err("application boundary must require confirmation");
    assert!(matches!(
        error,
        vantadeck_application::ApplicationError::ConfirmationRequired(_)
    ));
}
