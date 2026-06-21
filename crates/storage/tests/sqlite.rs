use std::path::PathBuf;
use vantadeck_domain::{AppInstallation, AppState};
use vantadeck_storage::Storage;
use vantadeck_storage::{ManualOverrideRecord, RegisteredProject};

#[tokio::test]
async fn stores_machine_local_preference() {
    let storage = Storage::connect("sqlite::memory:").await.expect("database");
    storage
        .set_preference("theme", &serde_json::json!("dark"))
        .await
        .expect("write preference");

    let value = storage
        .get_preference("theme")
        .await
        .expect("read preference");
    assert_eq!(value, Some(serde_json::json!("dark")));
}

#[tokio::test]
async fn file_database_reopens_with_persisted_values() {
    let root = tempfile::tempdir().expect("database directory");
    let path = root.path().join("vantadeck.db");
    {
        let storage = Storage::connect_path(&path).await.expect("file database");
        storage
            .set_preference("theme", &serde_json::json!("light"))
            .await
            .expect("write preference");
    }
    let reopened = Storage::connect_path(&path).await.expect("reopen database");

    assert_eq!(
        reopened
            .get_preference("theme")
            .await
            .expect("read preference"),
        Some(serde_json::json!("light"))
    );
}

#[tokio::test]
async fn searches_projects_and_updates_pinned_state() {
    let storage = Storage::connect("sqlite::memory:").await.expect("storage");
    for (name, root) in [
        ("Voidline", "D:/Projects/Voidline"),
        ("Emberfall", "D:/Projects/Emberfall"),
        ("Void Tools", "D:/Tools/Void"),
    ] {
        storage
            .register_project(&RegisteredProject {
                root: root.into(),
                name: name.into(),
                pinned: false,
            })
            .await
            .expect("register project");
    }

    let matches = storage.search_projects("void", 10).await.expect("search");
    assert_eq!(matches.len(), 2);
    storage
        .set_project_pinned(std::path::Path::new("D:/Projects/Voidline"), true)
        .await
        .expect("pin");
    let projects = storage.registered_projects().await.expect("projects");
    assert!(projects[0].pinned, "pinned projects sort first");
    assert_eq!(projects[0].name, "Voidline");
}

#[tokio::test]
async fn keeps_a_validated_tool_index_available_offline() {
    let storage = Storage::connect("sqlite::memory:").await.expect("storage");
    storage
        .cache_tool_index(
            "https://tools.vantadeck.org/v1/index.json",
            r#"[{"id":"mesh-helper"}]"#,
            Some("etag-1"),
        )
        .await
        .expect("cache index");

    let cached = storage
        .cached_tool_index("https://tools.vantadeck.org/v1/index.json")
        .await
        .expect("read cache")
        .expect("cached index");
    assert_eq!(cached.content, r#"[{"id":"mesh-helper"}]"#);
    assert_eq!(cached.etag.as_deref(), Some("etag-1"));
    assert!(!cached.fetched_at.is_empty());
}

#[tokio::test]
async fn stores_manual_override_without_project_data() {
    let storage = Storage::connect("sqlite::memory:").await.expect("database");
    let record = ManualOverrideRecord {
        app_id: "blender".into(),
        version: "4.2.3".parse().expect("version"),
        executable: PathBuf::from("D:/Portable/Blender/blender.exe"),
    };

    storage
        .set_manual_override(&record)
        .await
        .expect("store override");
    let loaded = storage
        .get_manual_override("blender", &record.version)
        .await
        .expect("load override")
        .expect("override exists");

    assert_eq!(loaded, record);
    assert_eq!(
        storage
            .manual_overrides("blender")
            .await
            .expect("list overrides"),
        vec![record]
    );
}

#[tokio::test]
async fn replaces_stale_detected_installations_for_an_app() {
    let storage = Storage::connect("sqlite::memory:").await.expect("database");
    let installation = |version: &str, path: &str| AppInstallation {
        version: version.parse().expect("version"),
        executable: PathBuf::from(path),
        state: AppState::Installed,
        evidence: Vec::new(),
    };
    storage
        .replace_detected_installations(
            "blender",
            &[
                installation("4.2.3", "C:/Blender/4.2.3/blender.exe"),
                installation("3.6.9", "C:/Blender/3.6.9/blender.exe"),
            ],
        )
        .await
        .expect("first scan");
    storage
        .replace_detected_installations(
            "blender",
            &[installation("4.2.3", "C:/Blender/4.2.3/blender.exe")],
        )
        .await
        .expect("second scan");

    let loaded = storage
        .detected_installations("blender")
        .await
        .expect("load detections");
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].version.to_string(), "4.2.3");
}

#[tokio::test]
async fn registers_projects_and_bounds_recent_activity() {
    let storage = Storage::connect("sqlite::memory:").await.expect("database");
    storage
        .register_project(&RegisteredProject {
            root: PathBuf::from("D:/Projects/Voidline"),
            name: "Voidline".into(),
            pinned: true,
        })
        .await
        .expect("register project");
    storage
        .record_activity("project-import", "Imported Voidline")
        .await
        .expect("first activity");
    storage
        .record_activity("health-check", "Checked Voidline")
        .await
        .expect("second activity");

    assert_eq!(
        storage.registered_projects().await.expect("projects").len(),
        1
    );
    let activity = storage.recent_activity(1).await.expect("activity");
    assert_eq!(activity.len(), 1);
    assert_eq!(activity[0].kind, "health-check");
}
