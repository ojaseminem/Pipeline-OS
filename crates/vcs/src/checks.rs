//! Pre-commit checks tuned for game and CG repositories: the mistakes that
//! are cheap to catch before a commit and expensive after it (a 300 MB FBX in
//! history, Unity's Library/ folder, a leaked token, a prefab whose `.meta`
//! stayed behind, a file Windows teammates can't check out).
//!
//! Checks never echo secret values — only `path:line` and the kind of secret.

use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    io::Read,
    path::{Component, Path, PathBuf},
    sync::LazyLock,
};

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::{GitProvider, VcsError, VcsOperationResult, ensure_gitignore_entry};

/// Files above this size should go through Git LFS.
pub const LFS_RECOMMENDED_BYTES: u64 = 50 * 1024 * 1024;
/// GitHub rejects pushes containing files above this size.
pub const GITHUB_HARD_LIMIT_BYTES: u64 = 100 * 1024 * 1024;
/// Text scanning skips files larger than this.
const MAX_SCAN_BYTES: u64 = 2 * 1024 * 1024;
/// And stops after this many files, so a giant first commit stays fast.
const MAX_SCAN_FILES: usize = 4000;
/// Locations listed per finding before truncating.
const MAX_LOCATIONS: usize = 50;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum CheckSeverity {
    Error,
    Warning,
    Info,
}

/// An automatic fix the user can apply with one click.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum FixKind {
    /// Track these patterns with Git LFS, then re-stage `paths` so they go
    /// through the LFS filter.
    TrackWithLfs {
        patterns: Vec<String>,
        paths: Vec<String>,
    },
    /// Append these entries to `.gitignore` (creating it if needed).
    AddGitignore { entries: Vec<String> },
    /// Remove these paths from the index (`git rm -r --cached`, files stay
    /// on disk) and ignore them.
    UntrackPaths {
        paths: Vec<String>,
        gitignore: Vec<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CheckFinding {
    /// Stable identifier of the check (e.g. `large-file`, `secrets`).
    pub id: String,
    pub severity: CheckSeverity,
    pub title: String,
    pub detail: String,
    /// Repository-relative paths involved.
    pub paths: Vec<String>,
    /// `path:line` locations, where the check is line-specific.
    #[serde(default)]
    pub locations: Vec<String>,
    pub fix: Option<FixKind>,
}

/// The game engine a project uses, as far as its layout tells.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ProjectEngine {
    Unity,
    Unreal,
    Godot,
}

impl ProjectEngine {
    fn label(self) -> &'static str {
        match self {
            Self::Unity => "Unity",
            Self::Unreal => "Unreal",
            Self::Godot => "Godot",
        }
    }

    /// Root-level generated folders that must never be committed (Unreal's
    /// `Binaries`/`Intermediate` also count inside `Plugins/*/`).
    fn generated_folders(self) -> &'static [&'static str] {
        match self {
            Self::Unity => &["Library", "Temp", "Logs", "obj", "UserSettings"],
            Self::Unreal => &["Binaries", "Intermediate", "Saved", "DerivedDataCache"],
            Self::Godot => &[".godot"],
        }
    }

    /// Recommended `.gitignore` entries (after GitHub's gitignore templates).
    pub fn gitignore_template(self) -> &'static [&'static str] {
        match self {
            Self::Unity => &[
                "/[Ll]ibrary/",
                "/[Tt]emp/",
                "/[Oo]bj/",
                "/[Bb]uild/",
                "/[Bb]uilds/",
                "/[Ll]ogs/",
                "/[Uu]ser[Ss]ettings/",
                "/[Mm]emoryCaptures/",
                "/[Rr]ecordings/",
                ".vs/",
                ".gradle/",
                "*.csproj",
                "*.sln",
                "*.suo",
                "*.user",
                "*.pidb",
                "*.tmp",
                "crashlytics-build.properties",
            ],
            Self::Unreal => &[
                "Binaries/",
                "Intermediate/",
                "Saved/",
                "DerivedDataCache/",
                ".vs/",
                "*.sln",
                "*.VC.db",
                "*.opensdf",
                "*.sdf",
                "*.suo",
                "*.xcodeproj",
                "*.xcworkspace",
            ],
            Self::Godot => &[".godot/", "/android/"],
        }
    }
}

/// Detects the engine from the project layout.
pub fn detect_engine(root: &Path) -> Option<ProjectEngine> {
    if root.join("Assets").is_dir() && root.join("ProjectSettings").is_dir() {
        return Some(ProjectEngine::Unity);
    }
    if std::fs::read_dir(root).ok()?.flatten().any(|entry| {
        entry
            .path()
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("uproject"))
    }) {
        return Some(ProjectEngine::Unreal);
    }
    if root.join("project.godot").is_file() {
        return Some(ProjectEngine::Godot);
    }
    None
}

#[derive(Debug, Clone)]
struct Candidate {
    path: String,
    deleted: bool,
    added: bool,
}

/// Rejects paths that could escape the repository or be read as options.
fn ensure_safe_relative(path: &str) -> Result<(), VcsError> {
    let candidate = Path::new(path);
    let bad = path.trim().is_empty()
        || path.starts_with('-')
        || candidate.is_absolute()
        || candidate
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::Prefix(_)));
    if bad {
        return Err(VcsError::Refused(format!(
            "'{path}' isn't a path inside the repository"
        )));
    }
    Ok(())
}

impl GitProvider {
    /// Runs every pre-commit check over the files about to be committed:
    /// `paths` when given (a partial commit), otherwise every changed file.
    pub async fn pre_commit_checks(
        &self,
        root: &Path,
        paths: Option<&[String]>,
    ) -> Result<Vec<CheckFinding>, VcsError> {
        let status = self.status(root).await?;
        let mut candidates: Vec<Candidate> = status
            .changed_files
            .iter()
            .map(|file| {
                let untracked = file.status == "untracked";
                let mut letters = file.status.chars();
                let index = letters.next().unwrap_or('.');
                let worktree = letters.next().unwrap_or('.');
                Candidate {
                    path: file.path.clone(),
                    deleted: !untracked && (index == 'D' || worktree == 'D'),
                    added: untracked || index == 'A',
                }
            })
            .collect();
        if let Some(selected) = paths.filter(|p| !p.is_empty()) {
            candidates.retain(|candidate| {
                selected.iter().any(|selected| {
                    let selected = selected.trim_end_matches('/');
                    candidate.path == selected
                        || candidate
                            .path
                            .strip_prefix(selected)
                            .is_some_and(|rest| rest.starts_with('/'))
                })
            });
        }
        let engine = detect_engine(root);
        let mut findings = Vec::new();
        let present: Vec<String> = candidates
            .iter()
            .filter(|c| !c.deleted)
            .map(|c| c.path.clone())
            .collect();

        findings.extend(windows_name_findings(&present));
        if let Some(engine) = engine {
            findings.extend(generated_folder_findings(engine, &present));
            if let Some(finding) = self.gitignore_finding(root, engine).await {
                findings.push(finding);
            }
            if engine == ProjectEngine::Unity {
                findings.extend(self.unity_meta_findings(root, &candidates).await);
            }
        }
        findings.extend(self.large_file_findings(root, &present).await);
        if let Some(finding) = self.case_collision_finding(root, &present).await {
            findings.push(finding);
        }

        let scan_root = root.to_path_buf();
        let scan_paths = present.clone();
        let scan = tokio::task::spawn_blocking(move || scan_contents(&scan_root, &scan_paths))
            .await
            .unwrap_or_default();
        findings.extend(scan.into_findings());

        findings.sort_by(|a, b| a.severity.cmp(&b.severity).then(a.id.cmp(&b.id)));
        Ok(findings)
    }

    /// Applies an automatic fix from a finding.
    pub async fn apply_fix(
        &self,
        root: &Path,
        fix: &FixKind,
    ) -> Result<VcsOperationResult, VcsError> {
        match fix {
            FixKind::TrackWithLfs { patterns, paths } => {
                for path in paths {
                    ensure_safe_relative(path)?;
                }
                let mut result = self.lfs_track_patterns(root, patterns.clone()).await?;
                if !paths.is_empty() {
                    // Re-stage already-staged files so they're stored as LFS
                    // pointers; `--renormalize` only touches indexed files.
                    let mut arguments = vec!["add", "--renormalize", "--"];
                    arguments.extend(paths.iter().map(String::as_str));
                    let _ = self.run_raw(root, &arguments).await;
                    result
                        .stdout
                        .push_str("\nRe-staged the affected files through LFS.");
                }
                Ok(result)
            }
            FixKind::AddGitignore { entries } => {
                let added = add_gitignore_entries(root, entries)?;
                Ok(VcsOperationResult {
                    stdout: if added.is_empty() {
                        ".gitignore already has these entries.".into()
                    } else {
                        format!("Added to .gitignore: {}", added.join(", "))
                    },
                    stderr: String::new(),
                    attempts: 1,
                })
            }
            FixKind::UntrackPaths { paths, gitignore } => {
                if paths.is_empty() {
                    return Err(VcsError::Refused("no paths to untrack".into()));
                }
                for path in paths {
                    ensure_safe_relative(path)?;
                }
                let mut arguments = vec!["rm", "-r", "--cached", "--ignore-unmatch", "-q", "--"];
                arguments.extend(paths.iter().map(String::as_str));
                self.run(root, &arguments).await?;
                let added = add_gitignore_entries(root, gitignore)?;
                let mut stdout = format!(
                    "Stopped tracking {} (files stay on disk).",
                    paths.join(", ")
                );
                if !added.is_empty() {
                    stdout.push_str(&format!(" Ignored: {}.", added.join(", ")));
                }
                Ok(VcsOperationResult {
                    stdout,
                    stderr: String::new(),
                    attempts: 1,
                })
            }
        }
    }

    async fn gitignore_finding(&self, root: &Path, engine: ProjectEngine) -> Option<CheckFinding> {
        let has_file = root.join(".gitignore").is_file();
        let probes: Vec<String> = engine
            .generated_folders()
            .iter()
            .map(|folder| format!("{folder}/pipelineos-probe"))
            .collect();
        let mut arguments = vec!["check-ignore", "--no-index", "--"];
        arguments.extend(probes.iter().map(String::as_str));
        let ignored: HashSet<String> = match self.run_raw(root, &arguments).await {
            Ok(output) => String::from_utf8_lossy(&output.stdout)
                .lines()
                .map(|line| line.trim().to_owned())
                .collect(),
            Err(_) => return None,
        };
        let missing: Vec<&str> = engine
            .generated_folders()
            .iter()
            .zip(&probes)
            .filter(|(_, probe)| !ignored.contains(probe.as_str()))
            .map(|(folder, _)| *folder)
            .collect();
        if has_file && missing.is_empty() {
            return None;
        }
        let existing: HashSet<String> = std::fs::read_to_string(root.join(".gitignore"))
            .unwrap_or_default()
            .lines()
            .map(|line| line.trim().to_owned())
            .collect();
        let entries: Vec<String> = engine
            .gitignore_template()
            .iter()
            .filter(|entry| !existing.contains(**entry))
            .map(|entry| (*entry).to_owned())
            .collect();
        let label = engine.label();
        Some(CheckFinding {
            id: "missing-gitignore".into(),
            severity: CheckSeverity::Warning,
            title: if has_file {
                format!(".gitignore doesn't cover {label}'s generated folders")
            } else {
                format!("No .gitignore for this {label} project")
            },
            detail: format!(
                "Not ignored: {}. These are rebuilt by the engine, are huge, and cause constant conflicts.",
                missing
                    .iter()
                    .map(|folder| format!("{folder}/"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            paths: vec![".gitignore".into()],
            locations: Vec::new(),
            fix: (!entries.is_empty()).then_some(FixKind::AddGitignore { entries }),
        })
    }

    async fn large_file_findings(&self, root: &Path, present: &[String]) -> Vec<CheckFinding> {
        let large: Vec<(String, u64)> = present
            .iter()
            .filter_map(|path| {
                let size = std::fs::metadata(root.join(path)).ok()?.len();
                (size > LFS_RECOMMENDED_BYTES).then(|| (path.clone(), size))
            })
            .collect();
        if large.is_empty() {
            return Vec::new();
        }
        let mut lfs = HashSet::new();
        for chunk in large.chunks(100) {
            let mut arguments = vec!["check-attr", "filter", "--"];
            arguments.extend(chunk.iter().map(|(path, _)| path.as_str()));
            if let Ok(output) = self.run_raw(root, &arguments).await {
                for line in String::from_utf8_lossy(&output.stdout).lines() {
                    if let Some(path) = line.strip_suffix(": filter: lfs") {
                        lfs.insert(path.to_owned());
                    }
                }
            }
        }
        let untracked: Vec<&(String, u64)> = large
            .iter()
            .filter(|(path, _)| !lfs.contains(path))
            .collect();
        let mut findings = Vec::new();
        for (over_limit, severity) in [
            (true, CheckSeverity::Error),
            (false, CheckSeverity::Warning),
        ] {
            let group: Vec<&(String, u64)> = untracked
                .iter()
                .copied()
                .filter(|(_, size)| (*size > GITHUB_HARD_LIMIT_BYTES) == over_limit)
                .collect();
            if group.is_empty() {
                continue;
            }
            let paths: Vec<String> = group.iter().map(|(path, _)| path.clone()).collect();
            let mut patterns: Vec<String> = paths.iter().map(|path| lfs_pattern(path)).collect();
            patterns.sort();
            patterns.dedup();
            let sizes = group
                .iter()
                .map(|(path, size)| format!("{path} ({:.0} MB)", *size as f64 / 1_048_576.0))
                .collect::<Vec<_>>()
                .join(", ");
            findings.push(CheckFinding {
                id: if over_limit {
                    "file-over-limit".into()
                } else {
                    "large-file".into()
                },
                severity,
                title: if over_limit {
                    "Files over GitHub's 100 MB limit aren't in Git LFS".into()
                } else {
                    "Large files aren't tracked by Git LFS".into()
                },
                detail: if over_limit {
                    format!("GitHub will reject the push: {sizes}.")
                } else {
                    format!("Files over 50 MB bloat every clone forever once committed: {sizes}.")
                },
                paths: paths.clone(),
                locations: Vec::new(),
                fix: Some(FixKind::TrackWithLfs { patterns, paths }),
            });
        }
        findings
    }

    async fn case_collision_finding(
        &self,
        root: &Path,
        present: &[String],
    ) -> Option<CheckFinding> {
        let output = self.run_raw(root, &["ls-files", "-z"]).await.ok()?;
        let tracked = String::from_utf8_lossy(&output.stdout).into_owned();
        let all = tracked
            .split('\0')
            .filter(|path| !path.is_empty())
            .map(str::to_owned)
            .chain(present.iter().cloned());
        let groups = case_collisions(all, present);
        if groups.is_empty() {
            return None;
        }
        let paths: Vec<String> = groups.iter().flatten().cloned().collect();
        Some(CheckFinding {
            id: "case-collision".into(),
            severity: CheckSeverity::Error,
            title: "Paths differ only by letter case".into(),
            detail: format!(
                "Windows and macOS treat these as the same file, so checkouts there break: {}.",
                groups
                    .iter()
                    .map(|group| group.join(" vs "))
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
            paths,
            locations: Vec::new(),
            fix: None,
        })
    }

    async fn tracked_subset(&self, root: &Path, paths: &[String]) -> HashSet<String> {
        let mut tracked = HashSet::new();
        for chunk in paths.chunks(200) {
            let mut arguments = vec!["ls-files", "--"];
            arguments.extend(chunk.iter().map(String::as_str));
            if let Ok(output) = self.run_raw(root, &arguments).await {
                tracked.extend(
                    String::from_utf8_lossy(&output.stdout)
                        .lines()
                        .map(str::to_owned),
                );
            }
        }
        tracked
    }

    async fn unity_meta_findings(
        &self,
        root: &Path,
        candidates: &[Candidate],
    ) -> Vec<CheckFinding> {
        let in_assets = |path: &str| path.starts_with("Assets/");
        let committing: HashSet<&str> = candidates
            .iter()
            .filter(|c| !c.deleted)
            .map(|c| c.path.as_str())
            .collect();
        let deleting: HashSet<&str> = candidates
            .iter()
            .filter(|c| c.deleted)
            .map(|c| c.path.as_str())
            .collect();

        // Metas every new asset (and each new folder above it) needs.
        let mut needed: BTreeSet<String> = BTreeSet::new();
        for candidate in candidates.iter().filter(|c| c.added && !c.deleted) {
            let path = candidate.path.as_str();
            if !in_assets(path) || path.ends_with(".meta") || unity_ignores(path) {
                continue;
            }
            needed.insert(format!("{path}.meta"));
            let mut parent = Path::new(path).parent();
            while let Some(dir) = parent {
                let dir_str = dir.to_string_lossy().replace('\\', "/");
                if dir_str == "Assets" || dir_str.is_empty() {
                    break;
                }
                needed.insert(format!("{dir_str}.meta"));
                parent = dir.parent();
            }
        }
        let needed: Vec<String> = needed
            .into_iter()
            .filter(|meta| !committing.contains(meta.as_str()))
            .collect();
        let tracked = self.tracked_subset(root, &needed).await;
        let mut missing_on_disk = Vec::new();
        let mut not_included = Vec::new();
        for meta in needed {
            if tracked.contains(&meta) {
                continue;
            }
            if root.join(&meta).exists() {
                not_included.push(meta);
            } else {
                missing_on_disk.push(meta);
            }
        }

        // Metas whose asset is gone (or being deleted while the meta stays).
        let mut orphans: Vec<String> = candidates
            .iter()
            .filter(|c| !c.deleted && in_assets(&c.path) && c.path.ends_with(".meta"))
            .filter(|c| !root.join(c.path.trim_end_matches(".meta")).exists())
            .map(|c| c.path.clone())
            .collect();
        for candidate in candidates.iter().filter(|c| c.deleted) {
            if !in_assets(&candidate.path) || candidate.path.ends_with(".meta") {
                continue;
            }
            let meta = format!("{}.meta", candidate.path);
            if !deleting.contains(meta.as_str()) && root.join(&meta).exists() {
                orphans.push(meta);
            }
        }
        orphans.sort();
        orphans.dedup();

        let mut findings = Vec::new();
        if !missing_on_disk.is_empty() {
            findings.push(CheckFinding {
                id: "unity-missing-meta".into(),
                severity: CheckSeverity::Error,
                title: "New Unity assets have no .meta file".into(),
                detail: "Open the project in Unity so it generates them; committing without .meta files breaks every reference to these assets for teammates.".into(),
                paths: missing_on_disk.iter().map(|m| m.trim_end_matches(".meta").to_owned()).collect(),
                locations: Vec::new(),
                fix: None,
            });
        }
        if !not_included.is_empty() {
            findings.push(CheckFinding {
                id: "unity-meta-not-included".into(),
                severity: CheckSeverity::Error,
                title: "Assets are being committed without their .meta files".into(),
                detail: "Include these .meta files in the same commit, or teammates get new GUIDs and broken references.".into(),
                paths: not_included,
                locations: Vec::new(),
                fix: None,
            });
        }
        if !orphans.is_empty() {
            findings.push(CheckFinding {
                id: "unity-orphan-meta".into(),
                severity: CheckSeverity::Warning,
                title: ".meta files without their asset".into(),
                detail: "These .meta files have no matching asset or folder; delete them together with the asset.".into(),
                paths: orphans,
                locations: Vec::new(),
                fix: None,
            });
        }
        findings
    }
}

/// Unity skips hidden files/folders and those ending in `~`.
fn unity_ignores(path: &str) -> bool {
    path.split('/')
        .any(|part| part.starts_with('.') || part.ends_with('~'))
}

fn lfs_pattern(path: &str) -> String {
    match Path::new(path).extension().and_then(|ext| ext.to_str()) {
        Some(extension) => format!("*.{extension}"),
        None => path.to_owned(),
    }
}

/// Appends entries to `.gitignore`, creating it when missing. Returns the
/// entries actually added.
pub fn add_gitignore_entries(root: &Path, entries: &[String]) -> Result<Vec<String>, VcsError> {
    let path = root.join(".gitignore");
    if !path.exists() {
        std::fs::write(&path, "# Generated by Pipeline OS\n")?;
    }
    let mut added = Vec::new();
    for entry in entries {
        let entry = entry.trim();
        if entry.is_empty() || entry.contains('\n') {
            continue;
        }
        if ensure_gitignore_entry(root, entry)? {
            added.push(entry.to_owned());
        }
    }
    Ok(added)
}

/// Groups of paths (files or folders) that collide case-insensitively, where
/// at least one member is in `focus` (the files being committed).
pub fn case_collisions(
    all: impl IntoIterator<Item = String>,
    focus: &[String],
) -> Vec<Vec<String>> {
    let mut by_lower: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for path in all {
        let mut prefix = String::new();
        for (index, part) in path.split('/').enumerate() {
            if index > 0 {
                prefix.push('/');
            }
            prefix.push_str(part);
            by_lower
                .entry(prefix.to_lowercase())
                .or_default()
                .insert(prefix.clone());
        }
    }
    let focus_lower: Vec<String> = focus.iter().map(|path| path.to_lowercase()).collect();
    let mut groups: Vec<Vec<String>> = by_lower
        .into_iter()
        .filter(|(_, variants)| variants.len() > 1)
        .filter(|(lower, _)| {
            focus_lower
                .iter()
                .any(|focus| focus == lower || focus.starts_with(&format!("{lower}/")))
        })
        .map(|(_, variants)| variants.into_iter().collect())
        .collect();
    // Report only the shallowest collision of a chain (a folder collision
    // implies collisions for everything below it).
    groups.sort_by_key(|group: &Vec<String>| group[0].len());
    let mut reported: Vec<Vec<String>> = Vec::new();
    for group in groups {
        let lower = group[0].to_lowercase();
        let covered = reported.iter().any(|existing| {
            let parent = existing[0].to_lowercase();
            lower.starts_with(&format!("{parent}/"))
        });
        if !covered {
            reported.push(group);
        }
    }
    reported
}

const WINDOWS_RESERVED: &[&str] = &[
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// Why a path component can't exist on Windows, if it can't.
pub fn windows_name_problem(component: &str) -> Option<&'static str> {
    if component
        .chars()
        .any(|c| matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*' | '\\') || (c as u32) < 32)
    {
        return Some("contains a character Windows doesn't allow (< > : \" | ? * \\)");
    }
    if component.ends_with('.') || component.ends_with(' ') {
        return Some("ends with a dot or space");
    }
    let stem = component.split('.').next().unwrap_or(component).trim_end();
    if WINDOWS_RESERVED.contains(&stem.to_ascii_lowercase().as_str()) {
        return Some("uses a name Windows reserves (CON, PRN, AUX, NUL, COM1-9, LPT1-9)");
    }
    None
}

fn windows_name_findings(present: &[String]) -> Vec<CheckFinding> {
    let mut paths = Vec::new();
    let mut reasons = BTreeSet::new();
    for path in present {
        if let Some(reason) = path.split('/').find_map(windows_name_problem) {
            paths.push(path.clone());
            reasons.insert(reason);
        }
    }
    if paths.is_empty() {
        return Vec::new();
    }
    vec![CheckFinding {
        id: "windows-invalid-name".into(),
        severity: CheckSeverity::Error,
        title: "File names Windows can't check out".into(),
        detail: format!(
            "Teammates on Windows won't be able to clone or pull: a name {}. Rename these files.",
            reasons.into_iter().collect::<Vec<_>>().join("; ")
        ),
        paths,
        locations: Vec::new(),
        fix: None,
    }]
}

/// The generated folder `path` lives in, if any (e.g. `Library` or
/// `Plugins/Foo/Binaries`), keeping the path's actual casing.
fn generated_prefix(engine: ProjectEngine, path: &str) -> Option<String> {
    let parts: Vec<&str> = path.split('/').collect();
    let matches = |part: &str| {
        engine
            .generated_folders()
            .iter()
            .any(|folder| folder.eq_ignore_ascii_case(part))
    };
    if parts.len() > 1 && matches(parts[0]) {
        return Some(parts[0].to_owned());
    }
    if engine == ProjectEngine::Unreal
        && parts
            .first()
            .is_some_and(|p| p.eq_ignore_ascii_case("Plugins"))
    {
        for (index, part) in parts.iter().enumerate().skip(1) {
            if index + 1 < parts.len()
                && (part.eq_ignore_ascii_case("Binaries")
                    || part.eq_ignore_ascii_case("Intermediate"))
            {
                return Some(parts[..=index].join("/"));
            }
        }
    }
    None
}

fn generated_folder_findings(engine: ProjectEngine, present: &[String]) -> Vec<CheckFinding> {
    let mut folders: BTreeMap<String, usize> = BTreeMap::new();
    for path in present {
        if let Some(prefix) = generated_prefix(engine, path) {
            *folders.entry(prefix).or_default() += 1;
        }
    }
    if folders.is_empty() {
        return Vec::new();
    }
    let paths: Vec<String> = folders.keys().map(|folder| format!("{folder}/")).collect();
    let mut gitignore: Vec<String> = folders
        .keys()
        .map(|folder| {
            if folder.contains('/') {
                // Nested plugin build output: ignore at any depth.
                format!("{}/", folder.rsplit('/').next().unwrap_or(folder))
            } else if engine == ProjectEngine::Unreal {
                format!("{folder}/")
            } else {
                format!("/{folder}/")
            }
        })
        .collect();
    gitignore.sort();
    gitignore.dedup();
    vec![CheckFinding {
        id: "generated-folders".into(),
        severity: CheckSeverity::Error,
        title: format!("{} generated folders are being committed", engine.label()),
        detail: format!(
            "These are rebuilt by the engine on every machine and must not be in Git: {}.",
            folders
                .iter()
                .map(|(folder, count)| format!("{folder}/ ({count} files)"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        paths,
        locations: Vec::new(),
        fix: Some(FixKind::UntrackPaths {
            paths: folders.keys().cloned().collect(),
            gitignore,
        }),
    }]
}

// ------------------------------------------------------------ content scan

struct SecretRule {
    kind: &'static str,
    regex: Regex,
}

static SECRET_RULES: LazyLock<Vec<SecretRule>> = LazyLock::new(|| {
    let rule = |kind, pattern: &str| SecretRule {
        kind,
        regex: Regex::new(pattern).expect("valid secret pattern"),
    };
    vec![
        rule(
            "private key",
            r"-----BEGIN (?:[A-Z0-9]+ )*PRIVATE KEY(?: BLOCK)?-----",
        ),
        rule("AWS access key", r"\b(?:AKIA|ASIA)[0-9A-Z]{16}\b"),
        rule(
            "GitHub token",
            r"\b(?:gh[pousr]_[A-Za-z0-9]{36,255}|github_pat_[A-Za-z0-9_]{22,255})\b",
        ),
        rule("Slack token", r"\bxox[abposr]-[A-Za-z0-9-]{10,}"),
    ]
});

static ENV_SECRET: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r##"(?i)^\s*(?:export\s+)?[A-Z0-9_.-]*(?:PASSWORD|PASSWD|SECRET|TOKEN|API_?KEY|PRIVATE_KEY)[A-Z0-9_.-]*\s*[=:]\s*['"]?([^\s'"#]*)"##,
    )
    .expect("valid env pattern")
});

/// `.env`, `.env.local`, `prod.env`… but not `.env.example` and friends.
fn is_env_file(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase();
    let is_env = name == ".env" || name.starts_with(".env.") || name.ends_with(".env");
    let template = ["example", "sample", "template", "dist", "defaults"]
        .iter()
        .any(|word| name.contains(word));
    is_env && !template
}

fn is_placeholder(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.is_empty()
        || lower.starts_with("${")
        || lower.starts_with('<')
        || lower.starts_with("your")
        || matches!(
            lower.as_str(),
            "changeme"
                | "change_me"
                | "xxx"
                | "null"
                | "none"
                | "true"
                | "false"
                | "placeholder"
                | "secret"
                | "password"
                | "todo"
        )
}

/// Secret kinds found on `line`, without ever returning the value.
pub fn secrets_in_line(line: &str, env_file: bool) -> Vec<&'static str> {
    let mut kinds: Vec<&'static str> = SECRET_RULES
        .iter()
        .filter(|rule| rule.regex.is_match(line))
        .map(|rule| rule.kind)
        .collect();
    if env_file
        && let Some(captures) = ENV_SECRET.captures(line)
        && !is_placeholder(captures.get(1).map_or("", |m| m.as_str()))
    {
        kinds.push("password/secret in .env");
    }
    kinds
}

#[derive(Default)]
struct ContentScan {
    conflicts: Vec<(String, usize)>,
    secrets: Vec<(String, usize, &'static str)>,
    mixed_endings: Vec<String>,
    skipped: usize,
}

impl ContentScan {
    fn into_findings(self) -> Vec<CheckFinding> {
        let mut findings = Vec::new();
        let unique = |locations: &mut dyn Iterator<Item = &String>| -> Vec<String> {
            let mut seen = Vec::new();
            for path in locations {
                if !seen.contains(path) {
                    seen.push(path.clone());
                }
            }
            seen
        };
        if !self.conflicts.is_empty() {
            findings.push(CheckFinding {
                id: "conflict-markers".into(),
                severity: CheckSeverity::Error,
                title: "Unresolved conflict markers".into(),
                detail: "These files still contain <<<<<<< / >>>>>>> markers from a merge.".into(),
                paths: unique(&mut self.conflicts.iter().map(|(path, _)| path)),
                locations: self
                    .conflicts
                    .iter()
                    .take(MAX_LOCATIONS)
                    .map(|(path, line)| format!("{path}:{line}"))
                    .collect(),
                fix: None,
            });
        }
        if !self.secrets.is_empty() {
            let kinds: BTreeSet<&str> = self.secrets.iter().map(|(_, _, kind)| *kind).collect();
            findings.push(CheckFinding {
                id: "secrets".into(),
                severity: CheckSeverity::Error,
                title: "Possible secrets in files being committed".into(),
                detail: format!(
                    "Found what looks like: {}. Remove them and rotate any real credential — Git history is forever.",
                    kinds.into_iter().collect::<Vec<_>>().join(", ")
                ),
                paths: unique(&mut self.secrets.iter().map(|(path, _, _)| path)),
                locations: self
                    .secrets
                    .iter()
                    .take(MAX_LOCATIONS)
                    .map(|(path, line, kind)| format!("{path}:{line} ({kind})"))
                    .collect(),
                fix: None,
            });
        }
        if !self.mixed_endings.is_empty() {
            findings.push(CheckFinding {
                id: "mixed-line-endings".into(),
                severity: CheckSeverity::Warning,
                title: "Mixed line endings".into(),
                detail: "These files mix CRLF and LF line endings, which makes diffs noisy. Normalize them (or add `* text=auto` to .gitattributes).".into(),
                paths: self.mixed_endings,
                locations: Vec::new(),
                fix: None,
            });
        }
        if self.skipped > 0 {
            findings.push(CheckFinding {
                id: "scan-limit".into(),
                severity: CheckSeverity::Info,
                title: "Some files weren't scanned".into(),
                detail: format!(
                    "{} file(s) were skipped by the content scan (only the first {MAX_SCAN_FILES} files are checked for secrets and conflict markers).",
                    self.skipped
                ),
                paths: Vec::new(),
                locations: Vec::new(),
                fix: None,
            });
        }
        findings
    }
}

fn read_text_file(path: &Path) -> Option<String> {
    let metadata = std::fs::metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_SCAN_BYTES {
        return None;
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    std::fs::File::open(path)
        .ok()?
        .read_to_end(&mut bytes)
        .ok()?;
    let head = &bytes[..bytes.len().min(8192)];
    if head.contains(&0) {
        return None; // binary
    }
    String::from_utf8(bytes).ok()
}

fn scan_contents(root: &Path, paths: &[String]) -> ContentScan {
    let mut scan = ContentScan::default();
    let mut scanned = 0usize;
    for path in paths {
        if scanned >= MAX_SCAN_FILES {
            scan.skipped += 1;
            continue;
        }
        let full: PathBuf = root.join(path);
        let Some(text) = read_text_file(&full) else {
            continue;
        };
        scanned += 1;
        scan_text(path, &text, &mut scan);
    }
    scan
}

fn scan_text(path: &str, text: &str, scan: &mut ContentScan) {
    let env_file = is_env_file(path);
    let mut conflict_start = None;
    let mut has_end = false;
    let mut crlf = 0usize;
    let mut lf = 0usize;
    let total = text.split('\n').count();
    for (index, line) in text.split('\n').enumerate() {
        let number = index + 1;
        if line.ends_with('\r') {
            crlf += 1;
        } else if index + 1 < total {
            lf += 1;
        }
        let line = line.trim_end_matches('\r');
        if conflict_start.is_none() && (line.starts_with("<<<<<<< ") || line == "<<<<<<<") {
            conflict_start = Some(number);
        }
        if line.starts_with(">>>>>>> ") || line == ">>>>>>>" {
            has_end = true;
        }
        for kind in secrets_in_line(line, env_file) {
            scan.secrets.push((path.to_owned(), number, kind));
        }
    }
    if let (Some(line), true) = (conflict_start, has_end) {
        scan.conflicts.push((path.to_owned(), line));
    }
    if crlf > 0 && lf > 0 {
        scan.mixed_endings.push(path.to_owned());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_secret_kinds_without_values() {
        assert_eq!(
            secrets_in_line("-----BEGIN OPENSSH PRIVATE KEY-----", false),
            vec!["private key"]
        );
        assert_eq!(
            secrets_in_line("-----BEGIN PRIVATE KEY-----", false),
            vec!["private key"]
        );
        assert_eq!(
            secrets_in_line("aws_key = AKIAIOSFODNN7EXAMPLE", false),
            vec!["AWS access key"]
        );
        let token = format!("token: ghp_{}", "a1B2".repeat(9));
        assert_eq!(secrets_in_line(&token, false), vec!["GitHub token"]);
        let pat = format!("github_pat_{}", "x".repeat(30));
        assert_eq!(secrets_in_line(&pat, false), vec!["GitHub token"]);
        assert_eq!(
            secrets_in_line("SLACK=xoxb-1234567890-abcdef", false),
            vec!["Slack token"]
        );
        assert!(secrets_in_line("DB_PASSWORD=hunter22", false).is_empty());
        assert_eq!(
            secrets_in_line("DB_PASSWORD=hunter22", true),
            vec!["password/secret in .env"]
        );
        assert!(secrets_in_line("DB_PASSWORD=", true).is_empty());
        assert!(secrets_in_line("DB_PASSWORD=${FROM_VAULT}", true).is_empty());
        assert!(secrets_in_line("let x = 1;", false).is_empty());
    }

    #[test]
    fn env_file_detection_skips_templates() {
        assert!(is_env_file(".env"));
        assert!(is_env_file("server/.env.local"));
        assert!(is_env_file("prod.env"));
        assert!(!is_env_file(".env.example"));
        assert!(!is_env_file("config/.env.sample"));
        assert!(!is_env_file("environment.txt"));
    }

    #[test]
    fn scan_finds_conflicts_and_mixed_endings() {
        let mut scan = ContentScan::default();
        scan_text(
            "a.cs",
            "x\n<<<<<<< HEAD\nours\n=======\ntheirs\n>>>>>>> feature\n",
            &mut scan,
        );
        scan_text("b.txt", "one\r\ntwo\nthree\r\n", &mut scan);
        scan_text("c.txt", "one\r\ntwo\r\n", &mut scan);
        assert_eq!(scan.conflicts, vec![("a.cs".to_owned(), 2)]);
        assert_eq!(scan.mixed_endings, vec!["b.txt".to_owned()]);
    }

    #[test]
    fn windows_names() {
        assert!(windows_name_problem("CON").is_some());
        assert!(windows_name_problem("con.txt").is_some());
        assert!(windows_name_problem("Lpt1.log").is_some());
        assert!(windows_name_problem("file.").is_some());
        assert!(windows_name_problem("file ").is_some());
        assert!(windows_name_problem("what?.png").is_some());
        assert!(windows_name_problem("a:b").is_some());
        assert!(windows_name_problem("console.txt").is_none());
        assert!(windows_name_problem("Hero.prefab").is_none());
    }

    #[test]
    fn case_collisions_report_shallowest_group() {
        let all = vec![
            "Assets/Art/a.png".to_owned(),
            "Assets/art/b.png".to_owned(),
            "Readme.md".to_owned(),
            "README.md".to_owned(),
            "src/ok.rs".to_owned(),
        ];
        let groups = case_collisions(all.clone(), &["Assets/art/b.png".to_owned()]);
        assert_eq!(
            groups,
            vec![vec!["Assets/Art".to_owned(), "Assets/art".to_owned()]]
        );
        let groups = case_collisions(all, &["README.md".to_owned()]);
        assert_eq!(
            groups,
            vec![vec!["README.md".to_owned(), "Readme.md".to_owned()]]
        );
    }

    #[test]
    fn generated_prefixes() {
        assert_eq!(
            generated_prefix(ProjectEngine::Unity, "Library/x/y.asset"),
            Some("Library".into())
        );
        assert_eq!(
            generated_prefix(ProjectEngine::Unity, "Assets/Library/x.cs"),
            None
        );
        assert_eq!(
            generated_prefix(ProjectEngine::Unreal, "Plugins/Foo/Binaries/Win64/x.dll"),
            Some("Plugins/Foo/Binaries".into())
        );
        assert_eq!(
            generated_prefix(ProjectEngine::Unreal, "Saved/Logs/x.log"),
            Some("Saved".into())
        );
    }

    #[test]
    fn safe_relative_paths() {
        assert!(ensure_safe_relative("Library").is_ok());
        assert!(ensure_safe_relative("../x").is_err());
        assert!(ensure_safe_relative("/etc").is_err());
        assert!(ensure_safe_relative("-rf").is_err());
    }
}
