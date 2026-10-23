//! Studio folder layouts and the project asset library.
//!
//! Layouts are additive templates (folders are created, never moved or
//! deleted) that give art and production files a predictable home. The asset
//! index lists creative files (concept art, references, models, audio...)
//! while skipping engine caches and anything git-ignored, and per-asset review
//! status (WIP / review / approved) is shared in `.pipelineos/assets.toml`.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

use serde::{Deserialize, Serialize};

use crate::{StudioError, metadata_path, write_atomic};

pub const ASSETS_FILE: &str = "assets.toml";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderLayout {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub folders: &'static [&'static str],
}

/// Built-in layouts. Paths use `/` and are relative to the project root.
pub fn folder_layouts() -> Vec<FolderLayout> {
    vec![
        FolderLayout {
            id: "game-art",
            name: "Game team",
            description: "Art, audio, design docs, and builds beside the engine project. Good for indie and mid-size game teams.",
            folders: &[
                "Art/Concept",
                "Art/Reference",
                "Art/Source/Models",
                "Art/Source/Textures",
                "Art/Source/Animation",
                "Art/Exports",
                "Art/Marketing",
                "Audio/Source",
                "Audio/Exports",
                "Docs/Design",
                "Docs/Tech",
                "Builds",
            ],
        },
        FolderLayout {
            id: "studio-pipeline",
            name: "Studio pipeline",
            description: "Numbered production stages from brief to delivery, the convention many larger studios and outsourcers expect.",
            folders: &[
                "00_Admin",
                "01_Reference",
                "02_Concept/Characters",
                "02_Concept/Environments",
                "02_Concept/Props",
                "03_Production/Characters",
                "03_Production/Environments",
                "03_Production/Props",
                "03_Production/FX",
                "04_Review",
                "05_Delivery",
            ],
        },
        FolderLayout {
            id: "film-vfx",
            name: "Film / VFX / Animation",
            description: "Asset- and shot-based structure for animation, VFX, and cinematics.",
            folders: &[
                "editorial",
                "reference",
                "concept",
                "assets/characters",
                "assets/props",
                "assets/environments",
                "shots",
                "renders",
                "comp",
                "delivery",
            ],
        },
    ]
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutResult {
    pub created: Vec<String>,
    pub existing: Vec<String>,
}

/// Creates the layout's missing folders. Each new leaf gets a `.gitkeep` so
/// the empty structure can be committed and shared. Never moves or deletes.
pub fn apply_layout(root: &Path, layout_id: &str) -> Result<LayoutResult, StudioError> {
    let layout = folder_layouts()
        .into_iter()
        .find(|layout| layout.id == layout_id)
        .ok_or_else(|| StudioError::NotFound {
            kind: "layout",
            id: layout_id.to_owned(),
        })?;
    let mut result = LayoutResult {
        created: Vec::new(),
        existing: Vec::new(),
    };
    for folder in layout.folders {
        let path = root.join(folder);
        if path.is_dir() {
            result.existing.push((*folder).to_owned());
            continue;
        }
        std::fs::create_dir_all(&path).map_err(|e| StudioError::io(&path, e))?;
        let keep = path.join(".gitkeep");
        std::fs::write(&keep, "").map_err(|e| StudioError::io(&keep, e))?;
        result.created.push((*folder).to_owned());
    }
    Ok(result)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AssetKind {
    /// Flat or texture images (PNG, EXR, DDS...).
    Image,
    /// Layered or procedural sources (PSD, Krita, Substance, ZBrush...).
    Source,
    /// Interchange meshes (FBX, OBJ, glTF...).
    Model,
    /// DCC scene files and USD stages (Blender, Maya, Houdini, USD...).
    Scene,
    /// Baked caches and volumes (Alembic, OpenVDB).
    Cache,
    /// Rigging and animation sources (Spine, BVH).
    Animation,
    Video,
    Audio,
    Document,
}

pub fn asset_kind(path: &Path) -> Option<AssetKind> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    Some(match ext.as_str() {
        "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp" | "tga" | "tif" | "tiff" | "exr"
        | "hdr" | "dds" | "ktx" | "ktx2" | "avif" | "heic" => AssetKind::Image,
        "psd" | "psb" | "kra" | "clip" | "procreate" | "afphoto" | "afdesign" | "xcf" | "ai"
        | "svg" | "aseprite" | "ase" | "spp" | "sbs" | "sbsar" | "ztl" | "zpr" => AssetKind::Source,
        "fbx" | "obj" | "gltf" | "glb" | "dae" | "stl" | "ply" | "3ds" => AssetKind::Model,
        "usd" | "usda" | "usdc" | "usdz" | "blend" | "ma" | "mb" | "max" | "c4d" | "hip"
        | "hiplc" | "hipnc" | "tbscene" => AssetKind::Scene,
        "abc" | "vdb" => AssetKind::Cache,
        "spine" | "bvh" => AssetKind::Animation,
        "mp4" | "mov" | "webm" | "mkv" | "avi" => AssetKind::Video,
        "wav" | "ogg" | "mp3" | "flac" | "aif" | "aiff" => AssetKind::Audio,
        "pdf" | "md" | "docx" | "pptx" | "xlsx" | "txt" => AssetKind::Document,
        _ => return None,
    })
}

/// Engine caches and generated folders that never hold authored assets.
const SKIP_DIRS: &[&str] = &[
    ".git",
    ".pipelineos",
    ".godot",
    "Library",
    "Temp",
    "Logs",
    "obj",
    "Intermediate",
    "Saved",
    "DerivedDataCache",
    "Binaries",
    "node_modules",
    "__pycache__",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ReviewStatus {
    #[default]
    None,
    Wip,
    Review,
    Approved,
    Rejected,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetMeta {
    #[serde(default)]
    pub status: ReviewStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetEntry {
    /// Project-relative path with `/` separators.
    pub path: String,
    pub name: String,
    /// Project-relative folder.
    pub folder: String,
    pub kind: AssetKind,
    pub size: u64,
    /// Unix seconds.
    pub modified: u64,
    pub meta: AssetMeta,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetIndex {
    pub assets: Vec<AssetEntry>,
    /// True when `limit` cut the listing short.
    pub truncated: bool,
}

/// Upper bound on files examined, so a huge depot stays responsive.
const MAX_SCANNED: usize = 50_000;

/// Lists authored creative files, newest first, respecting `.gitignore`.
pub fn index_assets(root: &Path, limit: usize) -> Result<AssetIndex, StudioError> {
    let metas = load_asset_meta(root)?;
    let mut assets = Vec::new();
    let walker = ignore::WalkBuilder::new(root)
        .hidden(true)
        .git_ignore(true)
        .filter_entry(|entry| {
            !(entry.file_type().is_some_and(|t| t.is_dir())
                && entry
                    .file_name()
                    .to_str()
                    .is_some_and(|name| SKIP_DIRS.contains(&name)))
        })
        .build();
    let mut truncated = false;
    let mut examined = 0usize;
    for entry in walker.flatten() {
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        examined += 1;
        if examined > MAX_SCANNED {
            truncated = true;
            break;
        }
        let Some(kind) = asset_kind(entry.path()) else {
            continue;
        };
        let Ok(relative) = entry.path().strip_prefix(root) else {
            continue;
        };
        let path = to_slash(relative);
        let metadata = entry.metadata().ok();
        let folder = relative.parent().map(to_slash).unwrap_or_default();
        assets.push(AssetEntry {
            name: entry.file_name().to_string_lossy().into_owned(),
            folder,
            kind,
            size: metadata.as_ref().map(|m| m.len()).unwrap_or(0),
            modified: metadata
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0),
            meta: metas.get(&path).cloned().unwrap_or_default(),
            path,
        });
    }
    assets.sort_by(|a, b| {
        b.modified
            .cmp(&a.modified)
            .then_with(|| a.path.cmp(&b.path))
    });
    if assets.len() > limit {
        assets.truncate(limit);
        truncated = true;
    }
    Ok(AssetIndex { assets, truncated })
}

fn to_slash(path: &Path) -> String {
    path.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct AssetsFile {
    #[serde(default)]
    asset: BTreeMap<String, AssetMeta>,
}

fn assets_path(root: &Path) -> PathBuf {
    metadata_path(root, ASSETS_FILE)
}

pub fn load_asset_meta(root: &Path) -> Result<BTreeMap<String, AssetMeta>, StudioError> {
    let path = assets_path(root);
    match std::fs::read_to_string(&path) {
        Ok(raw) => toml::from_str::<AssetsFile>(&raw)
            .map(|file| file.asset)
            .map_err(|e| StudioError::Parse {
                path,
                message: e.to_string(),
            }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
        Err(error) => Err(StudioError::io(&path, error)),
    }
}

/// Sets (or, with the default value, clears) review metadata for one asset.
pub fn set_asset_meta(root: &Path, asset_path: &str, meta: AssetMeta) -> Result<(), StudioError> {
    if asset_path.is_empty()
        // has_root: on Windows "/x" and "\\x" aren't absolute but escape the project.
        || Path::new(asset_path).has_root()
        || Path::new(asset_path).is_absolute()
        || asset_path.split(['/', '\\']).any(|part| part == "..")
    {
        return Err(StudioError::Invalid(format!(
            "asset path {asset_path:?} must be relative to the project"
        )));
    }
    let mut metas = load_asset_meta(root)?;
    let key = asset_path.replace('\\', "/");
    if meta == AssetMeta::default() {
        metas.remove(&key);
    } else {
        metas.insert(key, meta);
    }
    let body = toml::to_string_pretty(&AssetsFile { asset: metas })
        .map_err(|e| StudioError::Invalid(e.to_string()))?;
    write_atomic(&assets_path(root), &body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layouts_are_additive_and_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("Art/Concept")).unwrap();
        let first = apply_layout(dir.path(), "game-art").unwrap();
        assert!(first.existing.contains(&"Art/Concept".to_owned()));
        assert!(dir.path().join("Docs/Design/.gitkeep").exists());
        let second = apply_layout(dir.path(), "game-art").unwrap();
        assert!(second.created.is_empty());
        assert!(apply_layout(dir.path(), "nope").is_err());
    }

    #[test]
    fn indexes_creative_files_and_skips_caches() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        for file in [
            "Art/Concept/hero.png",
            "Art/Source/hero.psd",
            "Models/ship.fbx",
            "Library/cache.png",
            "src/main.cs",
            "ignored/big.png",
        ] {
            let path = root.join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, b"x").unwrap();
        }
        std::fs::write(root.join(".gitignore"), "ignored/\n").unwrap();
        // The ignore crate only honours .gitignore inside a git repository.
        std::fs::create_dir_all(root.join(".git")).unwrap();
        let index = index_assets(root, 100).unwrap();
        let paths: Vec<_> = index.assets.iter().map(|a| a.path.as_str()).collect();
        assert!(paths.contains(&"Art/Concept/hero.png"));
        assert!(paths.contains(&"Art/Source/hero.psd"));
        assert!(paths.contains(&"Models/ship.fbx"));
        assert!(!paths.iter().any(|p| p.starts_with("Library/")));
        assert!(!paths.iter().any(|p| p.starts_with("ignored/")));
        assert!(!paths.contains(&"src/main.cs"));
        let hero = index.assets.iter().find(|a| a.name == "hero.png").unwrap();
        assert_eq!(hero.kind, AssetKind::Image);
        assert_eq!(hero.folder, "Art/Concept");
    }

    #[test]
    fn classifies_creative_formats() {
        let cases = [
            ("shot.usdz", AssetKind::Scene),
            ("set.usda", AssetKind::Scene),
            ("hero.blend", AssetKind::Scene),
            ("rig.mb", AssetKind::Scene),
            ("fx.hiplc", AssetKind::Scene),
            ("look.tbscene", AssetKind::Scene),
            ("sim.abc", AssetKind::Cache),
            ("smoke.vdb", AssetKind::Cache),
            ("hero.spine", AssetKind::Animation),
            ("crate.GLB", AssetKind::Model),
            ("part.stl", AssetKind::Model),
            ("prop.dae", AssetKind::Model),
            ("sprite.aseprite", AssetKind::Source),
            ("logo.afdesign", AssetKind::Source),
            ("bust.ztl", AssetKind::Source),
            ("rock.sbsar", AssetKind::Source),
            ("albedo.ktx2", AssetKind::Image),
            ("sky.exr", AssetKind::Image),
            ("theme.flac", AssetKind::Audio),
            ("cut.mkv", AssetKind::Video),
            ("notes.txt", AssetKind::Document),
        ];
        for (file, kind) in cases {
            assert_eq!(asset_kind(Path::new(file)), Some(kind), "{file}");
        }
        assert_eq!(asset_kind(Path::new("Player.cs")), None);
        assert_eq!(asset_kind(Path::new("README")), None);
    }

    #[test]
    fn review_metadata_round_trips_and_clears() {
        let dir = tempfile::tempdir().unwrap();
        let meta = AssetMeta {
            status: ReviewStatus::Review,
            note: Some("Silhouette reads well; push the colors".into()),
            owner: Some("ana".into()),
        };
        set_asset_meta(dir.path(), "Art/Concept/hero.png", meta.clone()).unwrap();
        assert_eq!(
            load_asset_meta(dir.path()).unwrap()["Art/Concept/hero.png"],
            meta
        );
        set_asset_meta(dir.path(), "Art/Concept/hero.png", AssetMeta::default()).unwrap();
        assert!(load_asset_meta(dir.path()).unwrap().is_empty());
        assert!(set_asset_meta(dir.path(), "../x.png", meta).is_err());
    }
}
