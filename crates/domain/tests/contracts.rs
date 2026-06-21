use std::path::PathBuf;

use vantadeck_domain::{
    ApiMessage, AppCategory, AppInstallation, AppState, CliEnvelope, DetectedApplication,
    ProjectConfig,
};

#[test]
fn project_config_uses_versioned_portable_paths() {
    let config: ProjectConfig = toml::from_str(
        r#"
schema_version = 1
name = "Voidline"
project_type = "game-development"

[[linked_apps]]
app_id = "unity"
preferred_version = "2022.3"
project_file = "Game/Voidline.uproject"
"#,
    )
    .expect("valid project config");

    assert_eq!(config.schema_version, 1);
    assert_eq!(
        config.linked_apps[0].project_file.as_deref(),
        Some("Game/Voidline.uproject")
    );
}

#[test]
fn cli_failure_envelope_exposes_stable_error_code() {
    let envelope = CliEnvelope::<serde_json::Value>::failure(
        "project.vcs.sync",
        ApiMessage {
            code: "CONFIRMATION_REQUIRED".into(),
            message: "Pass --yes to sync".into(),
            remediation: Some("Review the operation and retry with --yes.".into()),
        },
    );
    let json = serde_json::to_value(envelope).expect("serializable envelope");

    assert_eq!(json["success"], false);
    assert_eq!(json["errors"][0]["code"], "CONFIRMATION_REQUIRED");
    assert!(json["data"].is_null());
}

#[test]
fn cli_envelope_has_stable_contract_fields() {
    let envelope = CliEnvelope::success("apps.list", vec!["blender"]);
    let json = serde_json::to_value(envelope).expect("serializable envelope");

    assert_eq!(json["schemaVersion"], 1);
    assert_eq!(json["command"], "apps.list");
    assert_eq!(json["success"], true);
    assert!(json["warnings"].is_array());
    assert!(json["errors"].is_array());
}

#[test]
fn application_versions_are_grouped_under_one_application() {
    let mut application = DetectedApplication::new("blender", "Blender", AppCategory::Dcc);
    application.add_installation(AppInstallation {
        version: "4.2.0".parse().expect("valid version"),
        executable: PathBuf::from("C:/Program Files/Blender/4.2/blender.exe"),
        state: AppState::Installed,
        evidence: Vec::new(),
    });
    application.add_installation(AppInstallation {
        version: "3.6.0".parse().expect("valid version"),
        executable: PathBuf::from("C:/Program Files/Blender/3.6/blender.exe"),
        state: AppState::Installed,
        evidence: Vec::new(),
    });

    assert_eq!(application.installations.len(), 2);
    assert_eq!(application.installations[0].version.to_string(), "4.2.0");
}
