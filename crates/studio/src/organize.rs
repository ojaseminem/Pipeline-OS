//! Folder layouts and project organizing.
//!
//! A layout is a plain JSON file: the folders a project should have, plus
//! simple rules ("PSD files go to Art/Source/Textures") for sorting loose
//! files into them. Layouts come from three places, later ones overriding
//! earlier ones with the same id:
//!
//! - built in (shipped with Pipeline OS),
//! - the user's library (`<data dir>/layouts/*.json`, this machine only),
//! - the project (`.pipelineos/layouts/*.json`, shared through version control).
//!
//! Organizing is always previewed first. A plan lists every move; applying it
//! moves only the files the user kept, never overwrites, never touches files
//! an engine manages (Unity `Assets/`, Unreal `Content/`, Godot imports...),
//! and records an undo journal on this machine.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use globset::{GlobBuilder, GlobMatcher};
use serde::{Deserialize, Serialize};

use crate::{
    METADATA_DIR, StudioError,
    assets::{AssetKind, asset_kind},
    metadata_path, validate_id, write_atomic,
};

pub const LAYOUT_FORMAT: &str = "pipelineos-layout";
pub const LAYOUTS_DIR: &str = "layouts";
const UNDO_DIR: &str = "organize-undo";

const BUILT_IN: &[&str] = &[
    include_str!("layouts/game-art.json"),
    include_str!("layouts/studio-pipeline.json"),
    include_str!("layouts/film-vfx.json"),
    include_str!("layouts/client-project.json"),
];

fn format_marker() -> String {
    LAYOUT_FORMAT.to_owned()
}

fn one() -> u32 {
    1
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Layout {
    #[serde(default = "format_marker")]
    pub format: String,
    #[serde(default = "one")]
    pub version: u32,
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Folders to create, relative to the project root, with `/` separators.
    #[serde(default)]
    pub folders: Vec<String>,
    /// Sorting rules; the first rule a file matches decides where it goes.
    #[serde(default)]
    pub rules: Vec<LayoutRule>,
    /// Path globs that are never moved (README, licenses...).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keep: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutRule {
    /// Destination folder, relative to the project root.
    pub to: String,
    /// File-name globs (`*.psd`, `*concept*`); a glob with `/` matches the
    /// whole project path. Case-insensitive. Empty means any name.
    #[serde(default, rename = "match", skip_serializing_if = "Vec::is_empty")]
    pub patterns: Vec<String>,
    /// Asset kinds (image, source, model, scene, cache, animation, video,
    /// audio, document). Empty means any kind.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub kinds: Vec<AssetKind>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LayoutSource {
    BuiltIn,
    User,
    Project,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutEntry {
    #[serde(flatten)]
    pub layout: Layout,
    pub source: LayoutSource,
    /// The file it was read from (not set for built-in layouts).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// A built-in or user layout with the same id that this one hides.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overrides: Option<LayoutSource>,
}

/// Where a layout is saved: this machine's library or the project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LayoutScope {
    User,
    Project,
}

// ------------------------------------------------------------ the library

pub fn built_in_layouts() -> Vec<Layout> {
    BUILT_IN
        .iter()
        .map(|raw| serde_json::from_str(raw).expect("built-in layouts are valid JSON"))
        .collect()
}

fn scope_dir(
    root: Option<&Path>,
    user_dir: Option<&Path>,
    scope: LayoutScope,
) -> Result<PathBuf, StudioError> {
    match scope {
        LayoutScope::Project => root
            .map(|root| metadata_path(root, LAYOUTS_DIR))
            .ok_or_else(|| StudioError::Invalid("open a project to use its layouts".into())),
        LayoutScope::User => user_dir.map(|dir| dir.join(LAYOUTS_DIR)).ok_or_else(|| {
            StudioError::Invalid("this machine's layout library isn't available".into())
        }),
    }
}

/// The folder holding a scope's layout files (created on first save).
pub fn layouts_dir(
    root: Option<&Path>,
    user_dir: Option<&Path>,
    scope: LayoutScope,
) -> Result<PathBuf, StudioError> {
    scope_dir(root, user_dir, scope)
}

fn read_dir_layouts(dir: &Path) -> Result<Vec<(Layout, PathBuf)>, StudioError> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(StudioError::io(dir, error)),
    };
    let mut layouts = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let raw = std::fs::read_to_string(&path).map_err(|e| StudioError::io(&path, e))?;
        let layout = parse_layout(&raw).map_err(|error| StudioError::Parse {
            path: path.clone(),
            message: error.to_string(),
        })?;
        layouts.push((layout, path));
    }
    layouts.sort_by_key(|(layout, _)| layout.name.to_lowercase());
    Ok(layouts)
}

/// Every layout available to a project: project ones first, then this
/// machine's, then built-in. A project or user layout with a built-in's id
/// replaces it.
pub fn list_layouts(
    root: Option<&Path>,
    user_dir: Option<&Path>,
) -> Result<Vec<LayoutEntry>, StudioError> {
    let mut by_id: BTreeMap<String, LayoutEntry> = BTreeMap::new();
    let mut order: Vec<String> = Vec::new();
    let mut add = |layout: Layout, source: LayoutSource, file: Option<PathBuf>| {
        let id = layout.id.clone();
        let overrides = by_id.get(&id).map(|previous| previous.source);
        if overrides.is_none() {
            order.push(id.clone());
        }
        by_id.insert(
            id,
            LayoutEntry {
                layout,
                source,
                file: file.map(|path| path.to_string_lossy().into_owned()),
                overrides,
            },
        );
    };
    for layout in built_in_layouts() {
        add(layout, LayoutSource::BuiltIn, None);
    }
    if let Some(dir) = user_dir {
        for (layout, path) in read_dir_layouts(&dir.join(LAYOUTS_DIR))? {
            add(layout, LayoutSource::User, Some(path));
        }
    }
    if let Some(root) = root {
        for (layout, path) in read_dir_layouts(&metadata_path(root, LAYOUTS_DIR))? {
            add(layout, LayoutSource::Project, Some(path));
        }
    }
    let rank = |source: LayoutSource| match source {
        LayoutSource::Project => 0,
        LayoutSource::User => 1,
        LayoutSource::BuiltIn => 2,
    };
    let mut entries: Vec<LayoutEntry> = order
        .into_iter()
        .filter_map(|id| by_id.remove(&id))
        .collect();
    entries.sort_by_key(|entry| rank(entry.source));
    Ok(entries)
}

pub fn find_layout(
    root: Option<&Path>,
    user_dir: Option<&Path>,
    id: &str,
) -> Result<Layout, StudioError> {
    list_layouts(root, user_dir)?
        .into_iter()
        .find(|entry| entry.layout.id == id)
        .map(|entry| entry.layout)
        .ok_or_else(|| StudioError::NotFound {
            kind: "layout",
            id: id.to_owned(),
        })
}

/// Pretty JSON exactly as it is saved, for copying and sharing.
pub fn layout_to_json(layout: &Layout) -> Result<String, StudioError> {
    serde_json::to_string_pretty(layout)
        .map(|json| format!("{json}\n"))
        .map_err(|e| StudioError::Invalid(e.to_string()))
}

/// Saves a layout as `<scope>/layouts/<id>.json`, replacing one with that id.
pub fn save_layout(
    root: Option<&Path>,
    user_dir: Option<&Path>,
    scope: LayoutScope,
    layout: &Layout,
) -> Result<PathBuf, StudioError> {
    let mut layout = layout.clone();
    layout.format = LAYOUT_FORMAT.to_owned();
    layout.version = 1;
    validate_layout(&layout)?;
    let path = scope_dir(root, user_dir, scope)?.join(format!("{}.json", layout.id));
    write_atomic(&path, &layout_to_json(&layout)?)?;
    Ok(path)
}

pub fn delete_layout(
    root: Option<&Path>,
    user_dir: Option<&Path>,
    scope: LayoutScope,
    id: &str,
) -> Result<(), StudioError> {
    validate_id(id)?;
    let path = scope_dir(root, user_dir, scope)?.join(format!("{id}.json"));
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Err(StudioError::NotFound {
            kind: "layout",
            id: id.to_owned(),
        }),
        Err(error) => Err(StudioError::io(&path, error)),
    }
}

// ------------------------------------------------------- parse and check

/// Reads a layout from JSON, or from a plain-text folder list:
///
/// ```text
/// # Game team                       <- first comment line names the layout
/// Art/Concept
/// Art/Source/Textures  <- *.psd *.kra source
/// Audio                <- audio
/// ```
///
/// Each line is a folder; after `<-`, file-name globs and asset kinds that
/// belong in it.
pub fn parse_layout(text: &str) -> Result<Layout, StudioError> {
    let trimmed = text.trim_start_matches('\u{feff}').trim();
    if trimmed.is_empty() {
        return Err(StudioError::Invalid("the layout is empty".into()));
    }
    let layout = if trimmed.starts_with('{') {
        let layout: Layout = serde_json::from_str(trimmed)
            .map_err(|e| StudioError::Invalid(format!("not a valid layout: {e}")))?;
        if layout.format != LAYOUT_FORMAT {
            return Err(StudioError::Invalid(format!(
                "expected \"format\": \"{LAYOUT_FORMAT}\", found {:?}",
                layout.format
            )));
        }
        if layout.version != 1 {
            return Err(StudioError::Invalid(format!(
                "layout version {} is newer than this app understands; update Pipeline OS",
                layout.version
            )));
        }
        layout
    } else {
        parse_text_layout(trimmed)?
    };
    validate_layout(&layout)?;
    Ok(layout)
}

fn parse_text_layout(text: &str) -> Result<Layout, StudioError> {
    let mut name = None;
    let mut folders = Vec::new();
    let mut rules = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(comment) = line.strip_prefix('#') {
            if name.is_none() && !comment.trim().is_empty() {
                name = Some(comment.trim().to_owned());
            }
            continue;
        }
        let (folder, rule) = match line.split_once("<-") {
            Some((folder, rule)) => (folder.trim(), Some(rule.trim())),
            None => (line, None),
        };
        let folder = folder.trim_matches(['/', '\\']).replace('\\', "/");
        if !folders.contains(&folder) {
            folders.push(folder.clone());
        }
        if let Some(rule) = rule {
            let mut patterns = Vec::new();
            let mut kinds = Vec::new();
            for token in rule.split_whitespace() {
                match serde_json::from_value::<AssetKind>(serde_json::Value::String(
                    token.to_ascii_lowercase(),
                )) {
                    Ok(kind) if !token.contains(['*', '?', '.', '[']) => kinds.push(kind),
                    _ => patterns.push(token.to_owned()),
                }
            }
            rules.push(LayoutRule {
                to: folder,
                patterns,
                kinds,
            });
        }
    }
    let name = name.unwrap_or_else(|| "Imported layout".to_owned());
    Ok(Layout {
        format: LAYOUT_FORMAT.to_owned(),
        version: 1,
        id: slug(&name),
        name,
        description: String::new(),
        folders,
        rules,
        keep: Vec::new(),
    })
}

/// A file-safe id from a display name ("My Layout!" -> "my-layout").
pub fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let out = out.trim_end_matches('-');
    let out: String = out.chars().take(48).collect();
    let out = out.trim_end_matches('-').to_owned();
    if out.is_empty() { "layout".into() } else { out }
}

fn check_relative(kind: &str, value: &str) -> Result<(), StudioError> {
    let bad = value.trim().is_empty()
        || Path::new(value).is_absolute()
        || Path::new(value).has_root()
        || value.contains(':')
        || value
            .split(['/', '\\'])
            .any(|part| part == ".." || part == "." || part.is_empty())
        || value.split(['/', '\\']).next() == Some(METADATA_DIR)
        || value.split(['/', '\\']).next() == Some(".git");
    if bad {
        return Err(StudioError::Invalid(format!(
            "{kind} {value:?} must be a folder inside the project, like \"Art/Concept\""
        )));
    }
    Ok(())
}

pub fn validate_layout(layout: &Layout) -> Result<(), StudioError> {
    validate_id(&layout.id)?;
    if layout.name.trim().is_empty() {
        return Err(StudioError::Invalid("the layout needs a name".into()));
    }
    if layout.folders.is_empty() && layout.rules.is_empty() {
        return Err(StudioError::Invalid(
            "the layout needs at least one folder".into(),
        ));
    }
    for folder in &layout.folders {
        check_relative("folder", folder)?;
    }
    for (index, rule) in layout.rules.iter().enumerate() {
        check_relative("rule destination", &rule.to)?;
        if rule.patterns.is_empty() && rule.kinds.is_empty() {
            return Err(StudioError::Invalid(format!(
                "rule {} (to {:?}) needs \"match\" globs or \"kinds\"",
                index + 1,
                rule.to
            )));
        }
        for pattern in &rule.patterns {
            glob(pattern)?;
        }
    }
    for pattern in &layout.keep {
        glob(pattern)?;
    }
    Ok(())
}

fn glob(pattern: &str) -> Result<GlobMatcher, StudioError> {
    GlobBuilder::new(pattern)
        .case_insensitive(true)
        .literal_separator(false)
        .build()
        .map(|glob| glob.compile_matcher())
        .map_err(|e| StudioError::Invalid(format!("pattern {pattern:?} is not valid: {e}")))
}

// -------------------------------------------------------- create folders

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutResult {
    pub created: Vec<String>,
    pub existing: Vec<String>,
}

/// Creates the layout's missing folders. Each new leaf gets a `.gitkeep` so
/// the empty structure can be committed and shared. Never moves or deletes.
pub fn create_folders(root: &Path, layout: &Layout) -> Result<LayoutResult, StudioError> {
    validate_layout(layout)?;
    let mut result = LayoutResult {
        created: Vec::new(),
        existing: Vec::new(),
    };
    for folder in &layout.folders {
        let path = root.join(folder);
        if path.is_dir() {
            result.existing.push(folder.clone());
            continue;
        }
        std::fs::create_dir_all(&path).map_err(|e| StudioError::io(&path, e))?;
        let keep = path.join(".gitkeep");
        std::fs::write(&keep, "").map_err(|e| StudioError::io(&keep, e))?;
        result.created.push(folder.clone());
    }
    Ok(result)
}

// ------------------------------------------------------------ the plan

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileMove {
    /// Project-relative paths with `/` separators.
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedMove {
    pub from: String,
    pub to: String,
    /// Why: the rule that matched, in words ("*.psd", "audio files").
    pub reason: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrganizePlan {
    pub layout_id: String,
    pub moves: Vec<PlannedMove>,
    /// Files a rule matched whose destination already has a file of that name.
    pub conflicts: Vec<PlannedMove>,
    /// Matching files left alone because an engine manages them.
    pub protected: usize,
    /// Which engine folders were protected, for the explanation.
    pub protected_areas: Vec<String>,
    pub examined: usize,
    pub truncated: bool,
}

/// Folders that never hold loose creative files worth sorting.
const SKIP_DIRS: &[&str] = &[
    ".git",
    METADATA_DIR,
    ".github",
    ".godot",
    ".vs",
    ".vscode",
    ".idea",
    "Library",
    "Temp",
    "Logs",
    "obj",
    "bin",
    "Intermediate",
    "Saved",
    "DerivedDataCache",
    "Binaries",
    "Build",
    "Builds",
    "node_modules",
    "vendor",
    "third_party",
    "ThirdParty",
    "__pycache__",
    "site",
    "public",
    "src",
];

const MAX_EXAMINED: usize = 50_000;

/// Engine-managed areas: moving files there from outside the editor breaks
/// references, so organizing never touches them.
fn protected_areas(root: &Path) -> Vec<String> {
    let mut areas = Vec::new();
    let is_unity = root
        .join("ProjectSettings")
        .join("ProjectVersion.txt")
        .is_file()
        || (root.join("Assets").is_dir() && root.join("ProjectSettings").is_dir());
    if is_unity {
        areas.extend(["Assets", "Packages", "ProjectSettings", "UserSettings"].map(String::from));
    }
    let is_unreal = std::fs::read_dir(root)
        .map(|entries| {
            entries.flatten().any(|entry| {
                entry.path().extension().and_then(|ext| ext.to_str()) == Some("uproject")
            })
        })
        .unwrap_or(false);
    if is_unreal {
        areas.extend(["Content", "Source", "Config", "Plugins", "Platforms"].map(String::from));
    }
    areas
}

fn is_godot(root: &Path) -> bool {
    root.join("project.godot").is_file()
}

/// Godot keeps a `<file>.import` beside every imported asset and refers to
/// scenes, resources and scripts by path, so those stay where they are.
fn godot_managed(path: &Path) -> bool {
    let ext = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if matches!(
        ext.as_str(),
        "tscn" | "scn" | "tres" | "res" | "gd" | "gdshader" | "import" | "godot" | "cs"
    ) {
        return true;
    }
    let mut import = path.as_os_str().to_owned();
    import.push(".import");
    Path::new(&import).exists()
}

fn to_slash(path: &Path) -> String {
    path.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn first_component(path: &str) -> String {
    path.split('/').next().unwrap_or(path).to_lowercase()
}

struct CompiledRule {
    to: String,
    /// (glob, matches the whole path, names an extension like `*.zip`).
    patterns: Vec<(GlobMatcher, bool, bool)>,
    kinds: Vec<AssetKind>,
    reason: String,
}

fn kind_label(kind: AssetKind) -> &'static str {
    match kind {
        AssetKind::Image => "images",
        AssetKind::Source => "source art",
        AssetKind::Model => "3D models",
        AssetKind::Scene => "scene files",
        AssetKind::Cache => "caches",
        AssetKind::Animation => "animation",
        AssetKind::Video => "video",
        AssetKind::Audio => "audio",
        AssetKind::Document => "documents",
    }
}

fn compile_rules(layout: &Layout) -> Result<Vec<CompiledRule>, StudioError> {
    layout
        .rules
        .iter()
        .map(|rule| {
            let patterns = rule
                .patterns
                .iter()
                .map(|pattern| {
                    let extension = pattern
                        .rfind('.')
                        .is_some_and(|dot| pattern.rfind('*').is_none_or(|star| dot > star));
                    Ok((glob(pattern)?, pattern.contains('/'), extension))
                })
                .collect::<Result<Vec<_>, StudioError>>()?;
            let mut reason = rule.patterns.join(", ");
            if !rule.kinds.is_empty() {
                let kinds = rule
                    .kinds
                    .iter()
                    .map(|kind| kind_label(*kind))
                    .collect::<Vec<_>>()
                    .join(", ");
                reason = if reason.is_empty() {
                    kinds
                } else {
                    format!("{reason} ({kinds})")
                };
            }
            Ok(CompiledRule {
                to: rule.to.trim_matches('/').to_owned(),
                patterns,
                kinds: rule.kinds.clone(),
                reason,
            })
        })
        .collect()
}

/// Works out which loose files the layout would move, without touching
/// anything. Files already inside one of the layout's top-level folders,
/// engine-managed files, git-ignored files and project files stay put.
pub fn plan_organize(root: &Path, layout: &Layout) -> Result<OrganizePlan, StudioError> {
    validate_layout(layout)?;
    let rules = compile_rules(layout)?;
    let keep = layout
        .keep
        .iter()
        .map(|pattern| glob(pattern))
        .collect::<Result<Vec<_>, _>>()?;
    let areas = protected_areas(root);
    let protected_tops: BTreeSet<String> = areas.iter().map(|area| area.to_lowercase()).collect();
    let godot = is_godot(root);
    let organized_tops: BTreeSet<String> = layout
        .folders
        .iter()
        .chain(layout.rules.iter().map(|rule| &rule.to))
        .map(|folder| first_component(folder))
        .collect();

    let mut plan = OrganizePlan {
        layout_id: layout.id.clone(),
        moves: Vec::new(),
        conflicts: Vec::new(),
        protected: 0,
        protected_areas: areas.clone(),
        examined: 0,
        truncated: false,
    };
    if godot {
        plan.protected_areas
            .push("Godot scenes, scripts and imported files".into());
    }
    let mut claimed: BTreeSet<String> = BTreeSet::new();
    let walker = ignore::WalkBuilder::new(root)
        .hidden(true)
        .git_ignore(true)
        .filter_entry(|entry| {
            !(entry.depth() > 0
                && entry.file_type().is_some_and(|t| t.is_dir())
                && entry
                    .file_name()
                    .to_str()
                    .is_some_and(|name| SKIP_DIRS.contains(&name)))
        })
        .build();
    for entry in walker.flatten() {
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        plan.examined += 1;
        if plan.examined > MAX_EXAMINED {
            plan.truncated = true;
            break;
        }
        let Ok(relative) = entry.path().strip_prefix(root) else {
            continue;
        };
        let path = to_slash(relative);
        let top = first_component(&path);
        let at_root = !path.contains('/');
        if !at_root && organized_tops.contains(&top) {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if keep
            .iter()
            .any(|glob| glob.is_match(&name) || glob.is_match(&path))
        {
            continue;
        }
        let kind = asset_kind(entry.path());
        let Some(rule) = rules.iter().find(|rule| {
            let kind_ok =
                rule.kinds.is_empty() || kind.is_some_and(|kind| rule.kinds.contains(&kind));
            // Recognised creative files match by name; anything else (code,
            // project files) only when a glob names its extension, like *.zip.
            let name_ok = |(glob, whole_path, extension): &(GlobMatcher, bool, bool)| {
                (kind.is_some() || *extension)
                    && glob.is_match(if *whole_path { &path } else { &name })
            };
            kind_ok
                && if rule.patterns.is_empty() {
                    kind.is_some()
                } else {
                    rule.patterns.iter().any(name_ok)
                }
        }) else {
            continue;
        };
        if (!at_root && protected_tops.contains(&top)) || (godot && godot_managed(entry.path())) {
            plan.protected += 1;
            continue;
        }
        let to = format!("{}/{}", rule.to, name);
        let planned = PlannedMove {
            from: path,
            reason: rule.reason.clone(),
            size: entry.metadata().map(|m| m.len()).unwrap_or(0),
            to: to.clone(),
        };
        if root.join(&to).exists() || !claimed.insert(to.to_lowercase()) {
            plan.conflicts.push(planned);
        } else {
            plan.moves.push(planned);
        }
    }
    plan.moves
        .sort_by(|a, b| a.to.cmp(&b.to).then_with(|| a.from.cmp(&b.from)));
    plan.conflicts.sort_by(|a, b| a.from.cmp(&b.from));
    Ok(plan)
}

// ------------------------------------------------------- apply and undo

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrganizeJournal {
    pub layout_id: String,
    /// RFC 3339.
    pub at: String,
    pub moves: Vec<FileMove>,
    /// Folders this organize created, so undo can remove them if empty.
    #[serde(default)]
    pub created_folders: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrganizeResult {
    pub moved: Vec<FileMove>,
    /// `from` paths that couldn't be moved, with the reason.
    pub skipped: Vec<(String, String)>,
    pub created_folders: Vec<String>,
}

fn safe_relative(value: &str) -> Result<PathBuf, StudioError> {
    check_relative("path", value)?;
    Ok(PathBuf::from(value))
}

fn journal_path(user_dir: &Path, root: &Path) -> PathBuf {
    use sha2::{Digest, Sha256};
    let canonical = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let digest = Sha256::digest(canonical.to_string_lossy().as_bytes());
    let name: String = digest
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect();
    user_dir.join(UNDO_DIR).join(format!("{name}.json"))
}

/// Moves the chosen files (a plan's moves, possibly trimmed by the user) and
/// creates the layout's folders. Never overwrites; a file that changed since
/// the preview is skipped. The journal for undo is kept in `user_dir`.
pub fn apply_organize(
    root: &Path,
    user_dir: &Path,
    layout: &Layout,
    moves: &[FileMove],
) -> Result<OrganizeResult, StudioError> {
    let created = create_folders(root, layout)?;
    let mut result = OrganizeResult {
        moved: Vec::new(),
        skipped: Vec::new(),
        created_folders: created.created,
    };
    let areas: BTreeSet<String> = protected_areas(root)
        .iter()
        .map(|area| area.to_lowercase())
        .collect();
    let godot = is_godot(root);
    for file_move in moves {
        let (from, to) = match (safe_relative(&file_move.from), safe_relative(&file_move.to)) {
            (Ok(from), Ok(to)) => (root.join(from), root.join(to)),
            _ => {
                result
                    .skipped
                    .push((file_move.from.clone(), "path is outside the project".into()));
                continue;
            }
        };
        let in_engine_area = file_move.from.contains('/')
            && areas.contains(&first_component(&file_move.from))
            || areas.contains(&first_component(&file_move.to))
            || (godot && godot_managed(&from));
        if in_engine_area {
            result.skipped.push((
                file_move.from.clone(),
                "the engine manages this file".into(),
            ));
            continue;
        }
        if !from.is_file() {
            result
                .skipped
                .push((file_move.from.clone(), "the file is no longer there".into()));
            continue;
        }
        if to.exists() {
            result.skipped.push((
                file_move.from.clone(),
                format!("{} already exists", file_move.to),
            ));
            continue;
        }
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent).map_err(|e| StudioError::io(parent, e))?;
        }
        match std::fs::rename(&from, &to) {
            Ok(()) => {
                result.moved.push(file_move.clone());
                remove_empty_parents(root, &from);
            }
            Err(error) => result
                .skipped
                .push((file_move.from.clone(), error.to_string())),
        }
    }
    if !result.moved.is_empty() || !result.created_folders.is_empty() {
        let journal = OrganizeJournal {
            layout_id: layout.id.clone(),
            at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            moves: result.moved.clone(),
            created_folders: result.created_folders.clone(),
        };
        let body = serde_json::to_string_pretty(&journal)
            .map_err(|e| StudioError::Invalid(e.to_string()))?;
        write_atomic(&journal_path(user_dir, root), &body)?;
    }
    Ok(result)
}

/// Removes folders a move left empty, up to (not including) the root.
fn remove_empty_parents(root: &Path, file: &Path) {
    let mut dir = file.parent();
    while let Some(current) = dir {
        if current == root || !current.starts_with(root) || std::fs::remove_dir(current).is_err() {
            break;
        }
        dir = current.parent();
    }
}

/// The last organize on this machine for this project, if it can be undone.
pub fn last_organize(root: &Path, user_dir: &Path) -> Result<Option<OrganizeJournal>, StudioError> {
    let path = journal_path(user_dir, root);
    match std::fs::read_to_string(&path) {
        Ok(raw) => serde_json::from_str(&raw)
            .map(Some)
            .map_err(|e| StudioError::Parse {
                path,
                message: e.to_string(),
            }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(StudioError::io(&path, error)),
    }
}

/// Moves files from the last organize back where they were and removes the
/// folders it created if they are empty again.
pub fn undo_organize(root: &Path, user_dir: &Path) -> Result<OrganizeResult, StudioError> {
    let journal = last_organize(root, user_dir)?.ok_or_else(|| StudioError::NotFound {
        kind: "organize to undo",
        id: root.to_string_lossy().into_owned(),
    })?;
    let mut result = OrganizeResult {
        moved: Vec::new(),
        skipped: Vec::new(),
        created_folders: Vec::new(),
    };
    for file_move in journal.moves.iter().rev() {
        let (Ok(from), Ok(to)) = (safe_relative(&file_move.from), safe_relative(&file_move.to))
        else {
            continue;
        };
        let (current, original) = (root.join(to), root.join(from));
        if !current.is_file() {
            result.skipped.push((
                file_move.to.clone(),
                "the file was moved or deleted since".into(),
            ));
            continue;
        }
        if original.exists() {
            result.skipped.push((
                file_move.to.clone(),
                format!("{} exists again", file_move.from),
            ));
            continue;
        }
        if let Some(parent) = original.parent() {
            std::fs::create_dir_all(parent).map_err(|e| StudioError::io(parent, e))?;
        }
        match std::fs::rename(&current, &original) {
            Ok(()) => {
                result.moved.push(FileMove {
                    from: file_move.to.clone(),
                    to: file_move.from.clone(),
                });
                remove_empty_parents(root, &current);
            }
            Err(error) => result
                .skipped
                .push((file_move.to.clone(), error.to_string())),
        }
    }
    // Folders made by the organize, deepest first, if nothing else is in them.
    let mut folders = journal.created_folders.clone();
    folders.sort_by_key(|folder| std::cmp::Reverse(folder.matches('/').count()));
    for folder in folders {
        let Ok(relative) = safe_relative(&folder) else {
            continue;
        };
        let dir = root.join(relative);
        let keep = dir.join(".gitkeep");
        let only_keep = std::fs::read_dir(&dir)
            .map(|entries| entries.flatten().all(|entry| entry.path() == keep))
            .unwrap_or(false);
        if only_keep {
            let _ = std::fs::remove_file(&keep);
            if std::fs::remove_dir(&dir).is_ok() {
                remove_empty_parents(root, &dir);
                result.created_folders.push(folder);
            }
        }
    }
    let path = journal_path(user_dir, root);
    std::fs::remove_file(&path).map_err(|e| StudioError::io(&path, e))?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(root: &Path, path: &str) {
        let full = root.join(path);
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(full, path).unwrap();
    }

    #[test]
    fn creating_folders_is_additive_and_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("Art/Concept")).unwrap();
        let layout = find_layout(None, None, "game-art").unwrap();
        let first = create_folders(dir.path(), &layout).unwrap();
        assert!(first.existing.contains(&"Art/Concept".to_owned()));
        assert!(dir.path().join("Docs/Design/.gitkeep").exists());
        assert!(
            create_folders(dir.path(), &layout)
                .unwrap()
                .created
                .is_empty()
        );
        assert!(find_layout(None, None, "nope").is_err());
    }

    #[test]
    fn built_in_layouts_are_valid() {
        let layouts = built_in_layouts();
        assert!(layouts.len() >= 4);
        for layout in layouts {
            validate_layout(&layout).unwrap();
            let json = layout_to_json(&layout).unwrap();
            assert_eq!(parse_layout(&json).unwrap(), layout);
        }
    }

    #[test]
    fn text_layouts_parse_folders_and_rules() {
        let layout = parse_layout(
            "# My Studio!\nArt/Concept\nArt/Source <- *.psd source\n\nAudio <- audio\n",
        )
        .unwrap();
        assert_eq!(layout.id, "my-studio");
        assert_eq!(layout.name, "My Studio!");
        assert_eq!(layout.folders, ["Art/Concept", "Art/Source", "Audio"]);
        assert_eq!(layout.rules[0].patterns, ["*.psd"]);
        assert_eq!(layout.rules[0].kinds, [AssetKind::Source]);
        assert_eq!(layout.rules[1].kinds, [AssetKind::Audio]);
    }

    #[test]
    fn invalid_layouts_are_explained() {
        assert!(
            parse_layout("{\"format\":\"other\",\"id\":\"a\",\"name\":\"A\",\"folders\":[\"x\"]}")
                .is_err()
        );
        assert!(parse_layout("../outside").is_err());
        assert!(parse_layout("# A\n.pipelineos/x").is_err());
        let error = parse_layout(r#"{"id":"a","name":"A","rules":[{"to":"x"}]}"#).unwrap_err();
        assert!(error.to_string().contains("needs"));
    }

    #[test]
    fn project_and_user_layouts_override_built_ins() {
        let project = tempfile::tempdir().unwrap();
        let user = tempfile::tempdir().unwrap();
        let mut copy = built_in_layouts().remove(0);
        copy.name = "Our game team".into();
        save_layout(
            Some(project.path()),
            Some(user.path()),
            LayoutScope::Project,
            &copy,
        )
        .unwrap();
        let mut mine = copy.clone();
        mine.id = "mine".into();
        save_layout(None, Some(user.path()), LayoutScope::User, &mine).unwrap();

        let list = list_layouts(Some(project.path()), Some(user.path())).unwrap();
        assert_eq!(list[0].source, LayoutSource::Project);
        assert_eq!(list[0].layout.name, "Our game team");
        assert_eq!(list[0].overrides, Some(LayoutSource::BuiltIn));
        assert_eq!(list[1].layout.id, "mine");
        assert_eq!(
            list.iter()
                .filter(|entry| entry.layout.id == copy.id)
                .count(),
            1
        );
        let saved =
            std::fs::read_to_string(project.path().join(".pipelineos/layouts/game-art.json"))
                .unwrap();
        assert!(saved.contains("\"format\": \"pipelineos-layout\""));

        delete_layout(
            Some(project.path()),
            Some(user.path()),
            LayoutScope::Project,
            "game-art",
        )
        .unwrap();
        let list = list_layouts(Some(project.path()), Some(user.path())).unwrap();
        assert_eq!(
            list.iter()
                .find(|entry| entry.layout.id == "game-art")
                .unwrap()
                .source,
            LayoutSource::BuiltIn
        );
    }

    #[test]
    fn plan_moves_loose_files_and_protects_engine_folders() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        // A Unity project with loose files at the root and in a messy folder.
        touch(root, "ProjectSettings/ProjectVersion.txt");
        touch(root, "Assets/Textures/hero.psd");
        touch(root, "hero_concept.png");
        touch(root, "rock.psd");
        touch(root, "stuff/ambience.wav");
        touch(root, "stuff/old/rock.psd");
        touch(root, "Art/Exports/already.png");
        touch(root, "README.md");
        touch(root, "LevelDesign.cs");
        touch(root, "Design.pdf");
        let layout = find_layout(Some(root), None, "game-art").unwrap();
        let plan = plan_organize(root, &layout).unwrap();
        let moves: Vec<(&str, &str)> = plan
            .moves
            .iter()
            .map(|m| (m.from.as_str(), m.to.as_str()))
            .collect();
        assert!(moves.contains(&("hero_concept.png", "Art/Concept/hero_concept.png")));
        assert!(moves.contains(&("stuff/ambience.wav", "Audio/Source/ambience.wav")));
        assert!(
            moves
                .iter()
                .any(|(_, to)| *to == "Art/Source/Textures/rock.psd")
        );
        // Two rock.psd files can't both land in one folder.
        assert_eq!(plan.conflicts.len(), 1);
        assert!(!moves.iter().any(|(from, _)| from.starts_with("Assets/")));
        assert!(!moves.iter().any(|(from, _)| from.starts_with("Art/")));
        assert!(
            !moves
                .iter()
                .any(|(from, _)| *from == "README.md" || *from == "LevelDesign.cs")
        );
        assert!(moves.contains(&("Design.pdf", "Docs/Design/Design.pdf")));
        assert_eq!(plan.protected, 1);
        assert!(plan.protected_areas.contains(&"Assets".to_string()));
    }

    #[test]
    fn godot_imported_files_stay() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        touch(root, "project.godot");
        touch(root, "icon.png");
        touch(root, "icon.png.import");
        touch(root, "concept_sketch.png");
        let layout = find_layout(None, None, "game-art").unwrap();
        let plan = plan_organize(root, &layout).unwrap();
        assert_eq!(plan.moves.len(), 1);
        assert_eq!(plan.moves[0].from, "concept_sketch.png");
        assert_eq!(plan.protected, 1);
    }

    #[test]
    fn apply_and_undo_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let user = tempfile::tempdir().unwrap();
        let root = dir.path();
        touch(root, "dump/hero_concept.png");
        touch(root, "dump/theme.wav");
        touch(root, "keep.txt");
        let layout = find_layout(None, None, "game-art").unwrap();
        let plan = plan_organize(root, &layout).unwrap();
        // The user unticks the audio file.
        let chosen: Vec<FileMove> = plan
            .moves
            .iter()
            .filter(|m| m.from.ends_with(".png"))
            .map(|m| FileMove {
                from: m.from.clone(),
                to: m.to.clone(),
            })
            .collect();
        let result = apply_organize(root, user.path(), &layout, &chosen).unwrap();
        assert_eq!(result.moved.len(), 1);
        assert!(root.join("Art/Concept/hero_concept.png").is_file());
        assert!(root.join("dump/theme.wav").is_file());
        assert!(root.join("Audio/Source/.gitkeep").is_file());
        assert!(last_organize(root, user.path()).unwrap().is_some());

        // Never escapes the project or overwrites.
        let bad = apply_organize(
            root,
            user.path(),
            &layout,
            &[FileMove {
                from: "keep.txt".into(),
                to: "../x.txt".into(),
            }],
        )
        .unwrap();
        assert_eq!(bad.skipped.len(), 1);

        // That attempt moved nothing, so the first organize is still the one to undo.
        let undo = undo_organize(root, user.path()).unwrap();
        assert_eq!(undo.moved.len(), 1);
        assert!(root.join("dump/hero_concept.png").is_file());
        assert!(!root.join("Art").exists());
        assert!(last_organize(root, user.path()).unwrap().is_none());
    }

    #[test]
    fn undo_restores_files_and_removes_new_folders() {
        let dir = tempfile::tempdir().unwrap();
        let user = tempfile::tempdir().unwrap();
        let root = dir.path();
        touch(root, "dump/hero_concept.png");
        let layout = find_layout(None, None, "client-project").unwrap();
        let plan = plan_organize(root, &layout).unwrap();
        let moves: Vec<FileMove> = plan
            .moves
            .iter()
            .map(|m| FileMove {
                from: m.from.clone(),
                to: m.to.clone(),
            })
            .collect();
        apply_organize(root, user.path(), &layout, &moves).unwrap();
        assert!(!root.join("dump").exists());
        let undo = undo_organize(root, user.path()).unwrap();
        assert_eq!(undo.moved.len(), 1);
        assert!(root.join("dump/hero_concept.png").is_file());
        assert!(!root.join("01_Brief").exists());
        assert!(!root.join("04_Exports").exists());
    }
}
