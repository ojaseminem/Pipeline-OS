use vantadeck_manifests::{AppManifest, ManifestError, ReviewState, ToolManifest};

#[test]
fn accepts_structured_launch_templates() {
    let manifest = AppManifest::from_json(
        r#"{
          "schemaVersion": 1,
          "id": "blender",
          "name": "Blender",
          "category": "dcc",
          "platforms": ["windows", "macos", "linux"],
          "executables": ["blender.exe", "blender"],
          "launchTemplates": [{"id":"empty","name":"Open Empty","arguments":[]}]
        }"#,
    )
    .expect("safe manifest");

    assert_eq!(manifest.id, "blender");
    assert_eq!(manifest.launch_templates[0].id, "empty");
}

#[test]
fn accepts_reviewed_tool_metadata_with_verified_artifact() {
    let manifest = ToolManifest::from_json(
        r#"{
          "schemaVersion": 1,
          "id": "example-tool",
          "name": "Example Tool",
          "description": "A production helper.",
          "sourceUrl": "https://github.com/example/tool",
          "license": "Apache-2.0",
          "supportedHosts": ["blender"],
          "platforms": ["windows", "linux"],
          "provenance": "Published by the upstream project release workflow.",
          "reviewState": "verified",
          "lastVerifiedAt": "2026-06-21",
          "safetyNotes": "Vantadeck does not execute this artifact.",
          "artifacts": [{
            "platform": "windows",
            "url": "https://github.com/example/tool/releases/download/v1/tool.zip",
            "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
          }]
        }"#,
    )
    .expect("valid tool manifest");

    assert_eq!(manifest.review_state, ReviewState::Verified);
    assert_eq!(manifest.artifacts.len(), 1);
}

#[test]
fn rejects_unverifiable_tool_artifacts() {
    let error = ToolManifest::from_json(
        r#"{
          "schemaVersion": 1,
          "id": "unsafe-tool",
          "name": "Unsafe Tool",
          "description": "Invalid metadata.",
          "sourceUrl": "https://example.com/source",
          "license": "MIT",
          "supportedHosts": [],
          "platforms": ["windows"],
          "provenance": "Community submission.",
          "reviewState": "submitted",
          "lastVerifiedAt": "2026-06-21",
          "safetyNotes": "Unreviewed.",
          "artifacts": [{
            "platform": "windows",
            "url": "http://example.com/tool.exe",
            "sha256": "not-a-checksum"
          }]
        }"#,
    )
    .expect_err("artifact must be HTTPS with a SHA-256 digest");

    assert!(matches!(error, ManifestError::UnsafeUrl { .. }));
}

#[test]
fn rejects_malformed_https_authorities() {
    for source_url in [
        "https://user@",
        "https://:bad",
        "https://[not-ipv6]",
        "https://[::1",
        "https://::1]",
        "https://[::1]suffix",
        "https://host:99999",
        "https://example.com\\path",
        "https://exa mple.com/path",
    ] {
        let source_url_json = serde_json::to_string(source_url).expect("test URL is JSON");
        let input = format!(
            r#"{{
              "schemaVersion":1,"id":"bad-tool","name":"Bad","description":"Bad metadata",
              "sourceUrl":{source_url_json},"license":"MIT","supportedHosts":["blender"],
              "platforms":["windows"],"provenance":"Unknown","reviewState":"submitted",
              "lastVerifiedAt":"2026-06-21","safetyNotes":"Unreviewed","artifacts":[]
            }}"#
        );
        assert!(
            matches!(
                ToolManifest::from_json(&input),
                Err(ManifestError::UnsafeUrl { .. })
            ),
            "accepted malformed authority: {source_url}"
        );
    }
}

#[test]
fn rejects_duplicate_tool_metadata() {
    for (supported_hosts, platforms) in [
        (r#"["blender","blender"]"#, r#"["windows"]"#),
        (r#"["blender"]"#, r#"["windows","windows"]"#),
    ] {
        let input = format!(
            r#"{{
              "schemaVersion":1,"id":"bad-tool","name":"Bad","description":"Bad metadata",
              "sourceUrl":"https://example.com/source","license":"MIT",
              "supportedHosts":{supported_hosts},"platforms":{platforms},
              "provenance":"Unknown","reviewState":"submitted",
              "lastVerifiedAt":"2026-06-21","safetyNotes":"Unreviewed","artifacts":[]
            }}"#
        );
        assert!(matches!(
            ToolManifest::from_json(&input),
            Err(ManifestError::EmptyField(_))
        ));
    }
}

#[test]
fn accepts_https_userinfo_ipv6_and_ports() {
    for source_url in [
        "https://user@example.com/source",
        "https://[2001:db8::1]/source",
        "https://[::1]:65535/source",
        "https://example.com:443/source",
    ] {
        let input = format!(
            r#"{{
              "schemaVersion":1,"id":"valid-tool","name":"Valid","description":"Valid metadata",
              "sourceUrl":"{source_url}","license":"MIT","supportedHosts":["blender"],
              "platforms":["windows"],"provenance":"Known","reviewState":"verified",
              "lastVerifiedAt":"2026-06-21","safetyNotes":"Reviewed","artifacts":[]
            }}"#
        );
        ToolManifest::from_json(&input).expect("HTTPS authority should be valid");
    }
}

#[test]
fn rejects_shell_metacharacters_in_arguments() {
    let error = AppManifest::from_json(
        r#"{
          "schemaVersion": 1,
          "id": "unsafe",
          "name": "Unsafe",
          "category": "utility",
          "platforms": ["windows"],
          "executables": ["unsafe.exe"],
          "launchTemplates": [{"id":"bad","name":"Bad","arguments":["{file} && curl evil"]}]
        }"#,
    )
    .expect_err("shell syntax must be rejected");

    assert!(matches!(error, ManifestError::UnsafeArgument { .. }));
}
