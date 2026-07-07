use std::{collections::BTreeMap, fs, io, path::Path};

use thiserror::Error;
use vantadeck_domain::{LinkedApp, ProjectConfig};
use vantadeck_security::resolve_within_root;
use walkdir::WalkDir;

/// Current per-project metadata folder name. Visible in end users' own project
/// directories (and their git history), so it carries the Pipeline OS brand —
/// unlike internal code identifiers (crate names, bundle id), which stay
/// "Vantadeck" by design.
const CONFIG_DIR: &str = ".pipelineos";
/// Legacy folder name from before the Pipeline OS rename. Projects that still
/// have this (and no `.pipelineos` yet) are migrated automatically on first
/// touch; see [`migrate_legacy_config`].
const LEGACY_CONFIG_DIR: &str = ".vantadeck";
const PROJECT_FILE: &str = ".pipelineos/project.toml";
/// Local-only, machine-specific project data (e.g. a personal to-do list).
/// Never synced via git — callers must gitignore this path.
pub const LOCAL_FILE: &str = ".pipelineos/local.json";

#[derive(Debug, Error)]
pub enum ProjectError {
    #[error("project file operation failed: {0}")]
    Io(#[from] io::Error),
    #[error("project TOML is invalid: {0}")]
    TomlDecode(#[from] toml::de::Error),
    #[error("project TOML could not be encoded: {0}")]
    TomlEncode(#[from] toml::ser::Error),
    #[error("project schema version {0} is unsupported")]
    UnsupportedSchema(u32),
    #[error("project contains an unsafe relative path")]
    UnsafePath,
    #[error("project root is not an existing directory")]
    InvalidRoot,
    #[error("project metadata already exists")]
    AlreadyExists,
    #[error("project metadata changed outside Pipeline OS; reload before saving")]
    ExternallyModified,
}

/// If a project still has the legacy `.vantadeck/` config folder and hasn't
/// been migrated to `.pipelineos/` yet, renames it in place. A no-op if
/// `.pipelineos` already exists (never clobbers a newer config) or if there's
/// no legacy folder to begin with.
pub fn migrate_legacy_config(root: &Path) {
    let legacy = root.join(LEGACY_CONFIG_DIR);
    let current = root.join(CONFIG_DIR);
    if legacy.is_dir() && !current.exists() {
        let _ = fs::rename(&legacy, &current);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectRevision(String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionedProject {
    pub config: ProjectConfig,
    pub revision: ProjectRevision,
}

pub fn infer_project(root: &Path, name: Option<&str>) -> Result<ProjectConfig, ProjectError> {
    if !root.is_dir() {
        return Err(ProjectError::InvalidRoot);
    }
    let mut linked_apps = Vec::new();
    let unity_version_file = root.join("ProjectSettings/ProjectVersion.txt");
    if unity_version_file.is_file() {
        linked_apps.push(LinkedApp {
            app_id: "unity".into(),
            preferred_version: read_unity_version(&unity_version_file),
            project_file: None,
            folder: Some(".".into()),
        });
    }

    let mut root_entries = fs::read_dir(root)?.collect::<Result<Vec<_>, _>>()?;
    root_entries.sort_by_key(|entry| entry.file_name());
    for entry in root_entries {
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) == Some("uproject") {
            linked_apps.push(LinkedApp {
                app_id: "unreal-engine".into(),
                preferred_version: None,
                project_file: Some(relative_path(root, &path)),
                folder: None,
            });
            break;
        }
    }
    if root.join("project.godot").is_file() {
        linked_apps.push(LinkedApp {
            app_id: "godot".into(),
            preferred_version: None,
            project_file: Some("project.godot".into()),
            folder: Some(".".into()),
        });
    }

    let mut creative_folders = BTreeMap::new();
    for entry in WalkDir::new(root)
        .max_depth(4)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
    {
        if entry.path().starts_with(root.join(CONFIG_DIR))
            || entry.path().starts_with(root.join(LEGACY_CONFIG_DIR))
        {
            continue;
        }
        let app_id = match entry
            .path()
            .extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("blend") => Some("blender"),
            Some("ma" | "mb") => Some("maya"),
            _ => None,
        };
        if let Some(app_id) = app_id {
            let parent = entry.path().parent().unwrap_or(root);
            creative_folders
                .entry(app_id)
                .or_insert_with(|| relative_path(root, parent));
        }
    }
    for (app_id, folder) in creative_folders {
        linked_apps.push(LinkedApp {
            app_id: app_id.into(),
            preferred_version: None,
            project_file: None,
            folder: Some(folder),
        });
    }

    let is_game_project = linked_apps
        .iter()
        .any(|app| matches!(app.app_id.as_str(), "unity" | "unreal-engine" | "godot"));
    Ok(ProjectConfig {
        schema_version: 1,
        name: name
            .map(str::to_owned)
            .or_else(|| {
                root.file_name()
                    .map(|value| value.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| "Untitled Project".into()),
        project_type: if is_game_project {
            "game-development".into()
        } else {
            "general-creative".into()
        },
        linked_apps,
        launch_profiles: Vec::new(),
        shortcuts: Vec::new(),
        version_control: None,
        enabled_health_checks: vec!["project-path".into(), "linked-apps".into()],
        thumbnail: None,
        tags: Vec::new(),
        category: None,
    })
}

/// Copies a chosen image into the project's `.pipelineos/` directory and records
/// the project-relative path in `project.toml`, so the thumbnail is portable and
/// travels with the repository. Returns the stored project-relative path.
pub fn set_project_thumbnail(root: &Path, source: &Path) -> Result<String, ProjectError> {
    if !source.is_file() {
        return Err(ProjectError::Io(io::Error::new(
            io::ErrorKind::NotFound,
            "thumbnail source image does not exist",
        )));
    }
    migrate_legacy_config(root);
    let extension = source
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .filter(|value| {
            matches!(
                value.as_str(),
                "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp"
            )
        })
        .unwrap_or_else(|| "png".into());
    let directory = root.join(CONFIG_DIR);
    fs::create_dir_all(&directory)?;
    // Remove any prior thumbnail with a different extension so only one remains.
    for previous in ["png", "jpg", "jpeg", "gif", "webp", "bmp"] {
        let candidate = directory.join(format!("thumbnail.{previous}"));
        if previous != extension && candidate.is_file() {
            let _ = fs::remove_file(candidate);
        }
    }
    let relative = format!("{CONFIG_DIR}/thumbnail.{extension}");
    fs::copy(source, root.join(&relative))?;
    let mut config = load_project(root)?;
    config.thumbnail = Some(relative.clone());
    save_project(root, &config)?;
    Ok(relative)
}

/// Reads the project's git-shared workspace document (notes, global to-dos,
/// references) from `.pipelineos/workspace.json`. Returns `None` when it
/// doesn't exist yet.
pub fn read_project_workspace(root: &Path) -> Result<Option<String>, ProjectError> {
    migrate_legacy_config(root);
    let file = root.join(CONFIG_DIR).join("workspace.json");
    match fs::read_to_string(&file) {
        Ok(content) => Ok(Some(content)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(ProjectError::Io(error)),
    }
}

/// Writes the project's git-shared workspace document. The caller supplies
/// the serialized JSON; this only owns the file location under `.pipelineos/`.
pub fn write_project_workspace(root: &Path, contents: &str) -> Result<(), ProjectError> {
    migrate_legacy_config(root);
    let directory = root.join(CONFIG_DIR);
    fs::create_dir_all(&directory)?;
    fs::write(directory.join("workspace.json"), contents.as_bytes())?;
    Ok(())
}

/// Reads the project's local-only document (e.g. a personal to-do list) from
/// `.pipelineos/local.json`. Returns `None` when it doesn't exist yet. Never
/// synced via git — see [`local_gitignore_pattern`].
pub fn read_project_local(root: &Path) -> Result<Option<String>, ProjectError> {
    migrate_legacy_config(root);
    let file = root.join(LOCAL_FILE);
    match fs::read_to_string(&file) {
        Ok(content) => Ok(Some(content)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(ProjectError::Io(error)),
    }
}

/// Writes the project's local-only document. The caller supplies the
/// serialized JSON; this only owns the file location under `.pipelineos/`.
pub fn write_project_local(root: &Path, contents: &str) -> Result<(), ProjectError> {
    migrate_legacy_config(root);
    let directory = root.join(CONFIG_DIR);
    fs::create_dir_all(&directory)?;
    fs::write(directory.join("local.json"), contents.as_bytes())?;
    Ok(())
}

/// The `.gitignore` pattern that keeps local-only project data out of git.
pub fn local_gitignore_pattern() -> &'static str {
    "/.pipelineos/local.json"
}

/// Sets the project's tags in the portable `project.toml`.
pub fn set_project_tags(root: &Path, tags: &[String]) -> Result<(), ProjectError> {
    let mut config = load_project(root)?;
    config.tags = tags.to_vec();
    save_project(root, &config)?;
    Ok(())
}

/// Sets the project's category in the portable `project.toml` (empty clears it).
pub fn set_project_category(root: &Path, category: Option<&str>) -> Result<(), ProjectError> {
    let mut config = load_project(root)?;
    config.category = category
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    save_project(root, &config)?;
    Ok(())
}

/// Adds an app to the project's linked apps (manual override alongside
/// auto-detection), if it isn't already linked. No-op otherwise.
pub fn add_linked_app(root: &Path, app_id: &str, folder: Option<&str>) -> Result<(), ProjectError> {
    let mut config = load_project(root)?;
    if !config.linked_apps.iter().any(|app| app.app_id == app_id) {
        config.linked_apps.push(LinkedApp {
            app_id: app_id.to_owned(),
            preferred_version: None,
            project_file: None,
            folder: folder.map(str::to_owned),
        });
        save_project(root, &config)?;
    }
    Ok(())
}

/// Removes an app from the project's linked apps, if present.
pub fn remove_linked_app(root: &Path, app_id: &str) -> Result<(), ProjectError> {
    let mut config = load_project(root)?;
    let before = config.linked_apps.len();
    config.linked_apps.retain(|app| app.app_id != app_id);
    if config.linked_apps.len() != before {
        save_project(root, &config)?;
    }
    Ok(())
}

/// Clears the project thumbnail: removes the stored image file and the
/// `thumbnail` entry from `project.toml`.
pub fn clear_project_thumbnail(root: &Path) -> Result<(), ProjectError> {
    let mut config = load_project(root)?;
    if let Some(relative) = config.thumbnail.take() {
        let file = root.join(&relative);
        if file.is_file() {
            let _ = fs::remove_file(file);
        }
        save_project(root, &config)?;
    }
    Ok(())
}

/// Registers a project folder. If it already has Pipeline OS metadata — a
/// teammate's clone from git, a project previously removed from this
/// machine's registry but never deleted from disk, or a legacy `.vantadeck`
/// folder — that existing config is adopted (loaded, migrated if needed) and
/// returned as-is rather than blocking the import. Only a folder with no
/// metadata at all gets a freshly inferred `project.toml`.
pub fn import_project(root: &Path, name: Option<&str>) -> Result<ProjectConfig, ProjectError> {
    migrate_legacy_config(root);
    if root.join(PROJECT_FILE).exists() {
        return load_project(root);
    }
    let config = infer_project(root, name)?;
    save_project(root, &config)?;
    Ok(config)
}

/// Repairs a project whose `.pipelineos/project.toml` is missing or can't be
/// read (e.g. the `PROJECT_CONFIG_INVALID` health check). Never deletes an
/// existing file: if one is present, it's renamed aside (`project.toml.broken`,
/// or `.broken.N` if that's already taken) so nothing is lost, then a fresh
/// config is regenerated the same way `import_project` builds one for a new
/// project and saved in its place.
pub fn repair_project(root: &Path, name: Option<&str>) -> Result<ProjectConfig, ProjectError> {
    migrate_legacy_config(root);
    let project_file = root.join(PROJECT_FILE);
    if project_file.is_file() {
        let mut backup = project_file.with_extension("toml.broken");
        let mut suffix = 1u32;
        while backup.exists() {
            backup = project_file.with_extension(format!("toml.broken.{suffix}"));
            suffix += 1;
        }
        fs::rename(&project_file, &backup)?;
    }
    let config = infer_project(root, name)?;
    save_project(root, &config)?;
    Ok(config)
}

fn read_unity_version(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok().and_then(|content| {
        content.lines().find_map(|line| {
            line.strip_prefix("m_EditorVersion:")
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
        })
    })
}

fn relative_path(root: &Path, path: &Path) -> String {
    let relative = path.strip_prefix(root).unwrap_or(path);
    if relative.as_os_str().is_empty() {
        return ".".into();
    }
    relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

pub fn load_project(root: &Path) -> Result<ProjectConfig, ProjectError> {
    Ok(load_project_versioned(root)?.config)
}

pub fn load_project_versioned(root: &Path) -> Result<VersionedProject, ProjectError> {
    migrate_legacy_config(root);
    let target = root.join(PROJECT_FILE);
    let backup = root.join(CONFIG_DIR).join("project.toml.bak");
    if !target.exists() && backup.exists() {
        fs::rename(&backup, &target)?;
    }
    let content = fs::read(target)?;
    let revision = revision_for(&content);
    let content = String::from_utf8(content)
        .map_err(|error| ProjectError::Io(io::Error::new(io::ErrorKind::InvalidData, error)))?;
    let config: ProjectConfig = toml::from_str(&content)?;
    validate_project(root, &config)?;
    Ok(VersionedProject { config, revision })
}

pub fn save_project(root: &Path, config: &ProjectConfig) -> Result<(), ProjectError> {
    validate_project(root, config)?;
    write_project(root, config, None).map(|_| ())
}

pub fn save_project_if_unchanged(
    root: &Path,
    config: &ProjectConfig,
    expected_revision: &ProjectRevision,
) -> Result<ProjectRevision, ProjectError> {
    validate_project(root, config)?;
    write_project(root, config, Some(expected_revision))
}

fn write_project(
    root: &Path,
    config: &ProjectConfig,
    expected_revision: Option<&ProjectRevision>,
) -> Result<ProjectRevision, ProjectError> {
    write_project_with_publisher(root, config, expected_revision, |publication, target| {
        fs::hard_link(publication, target)
    })
}

fn write_project_with_publisher<F>(
    root: &Path,
    config: &ProjectConfig,
    expected_revision: Option<&ProjectRevision>,
    publish: F,
) -> Result<ProjectRevision, ProjectError>
where
    F: FnOnce(&Path, &Path) -> io::Result<()>,
{
    let directory = root.join(CONFIG_DIR);
    fs::create_dir_all(&directory)?;
    let target = directory.join("project.toml");
    let proposal = directory.join("project.toml.proposal.tmp");
    let publication = directory.join("project.toml.publish.tmp");
    let backup = directory.join("project.toml.bak");
    let encoded = toml::to_string_pretty(config)?;
    fs::write(&proposal, encoded.as_bytes())?;
    if let Some(expected_revision) = expected_revision {
        let current = fs::read(&target)?;
        if revision_for(&current) != *expected_revision {
            preserve_conflict(&proposal, &directory)?;
            return Err(ProjectError::ExternallyModified);
        }
    }
    if publication.exists() {
        fs::remove_file(&publication)?;
    }
    fs::copy(&proposal, &publication)?;
    if target.exists() {
        if backup.exists() {
            fs::remove_file(&backup)?;
        }
        fs::rename(&target, &backup)?;
    }
    if let Err(error) = publish(&publication, &target) {
        if publication.exists() {
            fs::remove_file(&publication)?;
        }
        if error.kind() == io::ErrorKind::AlreadyExists {
            preserve_conflict(&proposal, &directory)?;
            if backup.exists() {
                fs::remove_file(backup)?;
            }
            return Err(ProjectError::ExternallyModified);
        }
        if backup.exists() && !target.exists() {
            fs::rename(&backup, &target)?;
        }
        preserve_conflict(&proposal, &directory)?;
        return Err(ProjectError::Io(error));
    }
    let target_content = match fs::read(&target) {
        Ok(content) => content,
        Err(error) => {
            if backup.exists() && !target.exists() {
                fs::rename(&backup, &target)?;
            }
            preserve_conflict(&proposal, &directory)?;
            if publication.exists() {
                fs::remove_file(&publication)?;
            }
            return Err(ProjectError::Io(error));
        }
    };
    if revision_for(&target_content) != revision_for(encoded.as_bytes()) {
        preserve_conflict(&proposal, &directory)?;
        if publication.exists() {
            fs::remove_file(&publication)?;
        }
        if backup.exists() {
            fs::remove_file(backup)?;
        }
        return Err(ProjectError::ExternallyModified);
    }
    if let Some(expected_revision) = expected_revision
        && backup.exists()
        && revision_for(&fs::read(&backup)?) != *expected_revision
    {
        fs::remove_file(&target)?;
        preserve_conflict(&proposal, &directory)?;
        fs::rename(&backup, &target)?;
        fs::remove_file(&publication)?;
        return Err(ProjectError::ExternallyModified);
    }
    fs::remove_file(&proposal)?;
    fs::remove_file(&publication)?;
    if backup.exists() {
        fs::remove_file(backup)?;
    }
    Ok(revision_for(encoded.as_bytes()))
}

fn preserve_conflict(temporary: &Path, directory: &Path) -> Result<(), ProjectError> {
    let conflict = directory.join("project.toml.conflict");
    if conflict.exists() {
        fs::remove_file(&conflict)?;
    }
    fs::rename(temporary, conflict)?;
    Ok(())
}

fn revision_for(content: &[u8]) -> ProjectRevision {
    // Stable FNV-1a fingerprint used only for optimistic concurrency, not security.
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in content {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    ProjectRevision(format!("{hash:016x}"))
}

fn validate_project(root: &Path, config: &ProjectConfig) -> Result<(), ProjectError> {
    if config.schema_version != 1 {
        return Err(ProjectError::UnsupportedSchema(config.schema_version));
    }
    for path in config
        .linked_apps
        .iter()
        .flat_map(|app| [app.project_file.as_deref(), app.folder.as_deref()])
        .flatten()
        .chain(
            config
                .shortcuts
                .iter()
                .map(|shortcut| shortcut.path.as_str()),
        )
    {
        resolve_within_root(root, Path::new(path)).map_err(|_| ProjectError::UnsafePath)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
            thumbnail: None,
            tags: Vec::new(),
            category: None,
        }
    }

    #[test]
    fn external_target_appearing_during_publication_wins() {
        let root = tempfile::tempdir().expect("temp project");
        save_project(root.path(), &project("Original")).expect("initial save");
        let loaded = load_project_versioned(root.path()).expect("versioned load");

        let result = write_project_with_publisher(
            root.path(),
            &project("Local Proposal"),
            Some(&loaded.revision),
            |publication, target| {
                fs::write(
                    target,
                    toml::to_string_pretty(&project("External Winner")).unwrap(),
                )?;
                fs::hard_link(publication, target)
            },
        );

        assert!(matches!(result, Err(ProjectError::ExternallyModified)));
        assert_eq!(load_project(root.path()).unwrap().name, "External Winner");
        let conflict = fs::read_to_string(root.path().join(".pipelineos/project.toml.conflict"))
            .expect("proposal preserved");
        assert!(conflict.contains("name = \"Local Proposal\""));
    }

    #[test]
    fn in_place_external_write_cannot_mutate_preserved_proposal() {
        let root = tempfile::tempdir().expect("temp project");
        save_project(root.path(), &project("Original")).expect("initial save");
        let loaded = load_project_versioned(root.path()).expect("versioned load");

        let result = write_project_with_publisher(
            root.path(),
            &project("Immutable Local Proposal"),
            Some(&loaded.revision),
            |publication, target| {
                fs::hard_link(publication, target)?;
                fs::write(
                    target,
                    toml::to_string_pretty(&project("External In Place")).unwrap(),
                )
            },
        );

        assert!(matches!(result, Err(ProjectError::ExternallyModified)));
        assert_eq!(load_project(root.path()).unwrap().name, "External In Place");
        let conflict = fs::read_to_string(root.path().join(".pipelineos/project.toml.conflict"))
            .expect("proposal preserved");
        assert!(conflict.contains("name = \"Immutable Local Proposal\""));
        assert!(!conflict.contains("External In Place"));
    }

    #[test]
    fn publication_failure_restores_backup() {
        let root = tempfile::tempdir().expect("temp project");
        save_project(root.path(), &project("Original")).expect("initial save");
        let loaded = load_project_versioned(root.path()).expect("versioned load");

        let result = write_project_with_publisher(
            root.path(),
            &project("Local Proposal"),
            Some(&loaded.revision),
            |_publication, _target| {
                Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "publication interrupted",
                ))
            },
        );

        assert!(matches!(result, Err(ProjectError::Io(_))));
        assert_eq!(load_project(root.path()).unwrap().name, "Original");
        assert!(!root.path().join(".pipelineos/project.toml.bak").exists());
        let conflict = fs::read_to_string(root.path().join(".pipelineos/project.toml.conflict"))
            .expect("proposal preserved");
        assert!(conflict.contains("name = \"Local Proposal\""));
    }
}
