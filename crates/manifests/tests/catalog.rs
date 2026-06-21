use std::{fs, path::Path};

use vantadeck_manifests::AppManifest;

#[test]
fn every_builtin_manifest_is_valid() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../manifests/apps");
    let entries = fs::read_dir(root).expect("built-in manifest directory");
    let mut count = 0;
    for entry in entries {
        let path = entry.expect("manifest entry").path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let content = fs::read_to_string(&path).expect("read manifest");
        AppManifest::from_json(&content)
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        count += 1;
    }
    assert!(count >= 9, "expected the representative alpha catalog");
}
