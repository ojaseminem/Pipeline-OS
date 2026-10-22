//! Key-level three-way merge for Pipeline OS data files.
//!
//! `.pipelineos/` files (tasks, asset reviews, project settings, issues) are
//! TOML, so two people editing different keys of the same file should never
//! see a git conflict. [`merge_toml`] merges by key instead of by line: a key
//! changed on one side takes that side, identical changes agree, tables added
//! on both sides are unioned, deletions apply, and `[[task]]`-style arrays
//! whose entries carry an `id` merge entry by entry. Only a key both sides
//! changed differently is a conflict; it defaults to "ours" in the merged text
//! and is reported so a person can choose ([`resolve_with_choices`]).
//!
//! The repo helpers read the three index stages of each conflicted file
//! (`git show :1:` base, `:2:` ours, `:3:` theirs), write the merge, and stage
//! files that resolved cleanly.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    process::{Command, Stdio},
};

use serde::{Deserialize, Serialize};
use toml_edit::{ArrayOfTables, DocumentMut, Item, Table};

use crate::{METADATA_DIR, StudioError};

/// Which version to keep for a conflicted key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Ours,
    Theirs,
    Base,
}

/// A key both sides changed differently. Values are TOML snippets; `None`
/// means the key does not exist on that side (deleted or never added).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TomlConflict {
    /// Dotted key path, e.g. `asset."Art/hero.png".status` or
    /// `task[id="build"].args`.
    pub key_path: String,
    pub ours: Option<String>,
    pub theirs: Option<String>,
    pub base: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeResult {
    /// The merged document; conflicts take "ours".
    pub merged: String,
    pub conflicts: Vec<TomlConflict>,
}

/// Three-way merges TOML documents key by key. `base` may be empty (both
/// sides added the file).
pub fn merge_toml(base: &str, ours: &str, theirs: &str) -> Result<MergeResult, StudioError> {
    merge_documents(base, ours, theirs, &BTreeMap::new())
}

/// Merges like [`merge_toml`], settling conflicts with `choices` (keyed by
/// [`TomlConflict::key_path`]); conflicts without a choice keep "ours".
pub fn resolve_with_choices(
    base: &str,
    ours: &str,
    theirs: &str,
    choices: &BTreeMap<String, Side>,
) -> Result<String, StudioError> {
    merge_documents(base, ours, theirs, choices).map(|result| result.merged)
}

fn parse(label: &str, raw: &str) -> Result<DocumentMut, StudioError> {
    raw.parse::<DocumentMut>().map_err(|e| StudioError::Parse {
        path: label.into(),
        message: e.to_string(),
    })
}

fn merge_documents(
    base: &str,
    ours: &str,
    theirs: &str,
    choices: &BTreeMap<String, Side>,
) -> Result<MergeResult, StudioError> {
    let base_doc = parse("base", base)?;
    let ours_doc = parse("ours", ours)?;
    let theirs_doc = parse("theirs", theirs)?;
    let mut merger = Merger {
        choices,
        conflicts: Vec::new(),
    };
    let merged_root = merger.merge_tables(
        "",
        Some(base_doc.as_table()),
        ours_doc.as_table(),
        theirs_doc.as_table(),
    );
    let mut doc = ours_doc.clone();
    *doc.as_table_mut() = merged_root;
    let mut position = 0;
    renumber(doc.as_table_mut(), &mut position);
    let merged = doc.to_string();
    // Never hand back something that is not valid TOML.
    parse("merged", &merged)?;
    Ok(MergeResult {
        merged,
        conflicts: merger.conflicts,
    })
}

/// Gives every table a document position in tree order, so tables taken from
/// "theirs" print after their parents instead of colliding with ours.
fn renumber(table: &mut Table, position: &mut usize) {
    for (_, item) in table.iter_mut() {
        match item {
            Item::Table(child) => {
                child.set_position(*position);
                *position += 1;
                renumber(child, position);
            }
            Item::ArrayOfTables(array) => {
                for child in array.iter_mut() {
                    child.set_position(*position);
                    *position += 1;
                    renumber(child, position);
                }
            }
            _ => {}
        }
    }
}

struct Merger<'a> {
    choices: &'a BTreeMap<String, Side>,
    conflicts: Vec<TomlConflict>,
}

fn is_bare_key(key: &str) -> bool {
    !key.is_empty()
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn join(path: &str, key: &str) -> String {
    let key = if is_bare_key(key) {
        key.to_owned()
    } else {
        format!("{key:?}")
    };
    if path.is_empty() {
        key
    } else {
        format!("{path}.{key}")
    }
}

/// Canonical value of an item for equality (formatting and comments ignored).
fn canonical(item: Option<&Item>) -> Option<toml::Value> {
    let item = item?;
    if item.is_none() {
        return None;
    }
    let mut doc = DocumentMut::new();
    doc.insert("v", item.clone());
    toml::from_str::<toml::Table>(&doc.to_string())
        .ok()
        .and_then(|mut table| table.remove("v"))
}

/// A TOML snippet for showing one side of a conflict.
fn snippet(item: Option<&Item>) -> Option<String> {
    let item = item?;
    match item {
        Item::None => None,
        Item::Value(value) => Some(value.to_string().trim().to_owned()),
        other => {
            let mut doc = DocumentMut::new();
            doc.insert("v", other.clone());
            Some(doc.to_string().trim().to_owned())
        }
    }
}

fn as_table(item: Option<&Item>) -> Option<&Table> {
    item.and_then(Item::as_table)
}

/// The `id` of each array-of-tables entry, when every entry has a unique one.
fn keyed(array: &ArrayOfTables) -> Option<Vec<String>> {
    let mut seen = BTreeSet::new();
    array
        .iter()
        .map(|table| {
            let id = table.get("id")?.as_str()?.to_owned();
            seen.insert(id.clone()).then_some(id)
        })
        .collect()
}

impl Merger<'_> {
    fn merge_item(
        &mut self,
        path: &str,
        base: Option<&Item>,
        ours: Option<&Item>,
        theirs: Option<&Item>,
    ) -> Option<Item> {
        // Both sides still have a table: merge inside it.
        if let (Some(Item::Table(o)), Some(Item::Table(t))) = (ours, theirs) {
            let merged = self.merge_tables(path, as_table(base), o, t);
            return Some(Item::Table(merged));
        }
        if let (Some(Item::ArrayOfTables(o)), Some(Item::ArrayOfTables(t))) = (ours, theirs) {
            let base_array = base.and_then(Item::as_array_of_tables);
            let base_ids = match base_array {
                Some(b) => keyed(b),
                None => Some(Vec::new()),
            };
            if let (Some(base_ids), Some(ours_ids), Some(theirs_ids)) =
                (base_ids, keyed(o), keyed(t))
            {
                return Some(Item::ArrayOfTables(self.merge_keyed_array(
                    path,
                    base_array.map(|b| (b, base_ids)),
                    (o, ours_ids),
                    (t, theirs_ids),
                )));
            }
        }
        self.merge_leaf(path, base, ours, theirs)
    }

    fn merge_leaf(
        &mut self,
        path: &str,
        base: Option<&Item>,
        ours: Option<&Item>,
        theirs: Option<&Item>,
    ) -> Option<Item> {
        let (b, o, t) = (canonical(base), canonical(ours), canonical(theirs));
        if o == t || b == t {
            return ours.cloned();
        }
        if b == o {
            return theirs.cloned();
        }
        let choice = self.choices.get(path).copied();
        if choice.is_none() {
            self.conflicts.push(TomlConflict {
                key_path: path.to_owned(),
                ours: snippet(ours),
                theirs: snippet(theirs),
                base: snippet(base),
            });
        }
        match choice.unwrap_or(Side::Ours) {
            Side::Ours => ours.cloned(),
            Side::Theirs => theirs.cloned(),
            Side::Base => base.cloned(),
        }
    }

    fn merge_tables(
        &mut self,
        path: &str,
        base: Option<&Table>,
        ours: &Table,
        theirs: &Table,
    ) -> Table {
        // Start from ours so its formatting, comments and order survive.
        let mut result = ours.clone();
        let mut keys: Vec<String> = ours.iter().map(|(k, _)| k.to_owned()).collect();
        for (key, _) in theirs.iter() {
            if !ours.contains_key(key) {
                keys.push(key.to_owned());
            }
        }
        if let Some(base) = base {
            for (key, _) in base.iter() {
                if !keys.iter().any(|k| k == key) {
                    keys.push(key.to_owned());
                }
            }
        }
        for key in keys {
            let child_path = join(path, &key);
            let merged = self.merge_item(
                &child_path,
                base.and_then(|b| b.get(&key)),
                ours.get(&key),
                theirs.get(&key),
            );
            match merged {
                // Assign in place so the key keeps its decor (comments).
                Some(item) => match result.get_mut(&key) {
                    Some(slot) => *slot = item,
                    None => {
                        result.insert(&key, item);
                    }
                },
                None => {
                    result.remove(&key);
                }
            }
        }
        result
    }

    fn merge_keyed_array(
        &mut self,
        path: &str,
        base: Option<(&ArrayOfTables, Vec<String>)>,
        ours: (&ArrayOfTables, Vec<String>),
        theirs: (&ArrayOfTables, Vec<String>),
    ) -> ArrayOfTables {
        let find = |array: &ArrayOfTables, ids: &[String], id: &str| -> Option<Item> {
            ids.iter()
                .position(|candidate| candidate == id)
                .and_then(|index| array.get(index))
                .map(|table| Item::Table(table.clone()))
        };
        let mut order: Vec<String> = ours.1.clone();
        for id in &theirs.1 {
            if !order.contains(id) {
                order.push(id.clone());
            }
        }
        if let Some((_, base_ids)) = &base {
            for id in base_ids {
                if !order.contains(id) {
                    order.push(id.clone());
                }
            }
        }
        let mut result = ArrayOfTables::new();
        for id in order {
            let entry_path = format!("{path}[id={id:?}]");
            let b = base.as_ref().and_then(|(array, ids)| find(array, ids, &id));
            let o = find(ours.0, &ours.1, &id);
            let t = find(theirs.0, &theirs.1, &id);
            if let Some(Item::Table(table)) =
                self.merge_item(&entry_path, b.as_ref(), o.as_ref(), t.as_ref())
            {
                result.push(table);
            }
        }
        result
    }
}

// ------------------------------------------------------------- repository

/// Merge outcome for one conflicted Pipeline OS file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileMergeReport {
    /// Path relative to the project root, with `/` separators.
    pub path: String,
    /// Merged without conflicts, written, and staged (`git add`).
    pub auto_resolved: bool,
    pub conflicts: Vec<TomlConflict>,
    /// Why the file could not be merged automatically (e.g. deleted on one
    /// side, or not valid TOML), when that is the case.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

fn git(root: &Path, args: &[&str]) -> Result<Vec<u8>, StudioError> {
    let mut command = Command::new("git");
    command
        .args(args)
        .current_dir(root)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let output = command
        .output()
        .map_err(|e| StudioError::Invalid(format!("could not run git: {e}")))?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(StudioError::Invalid(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        )))
    }
}

fn is_pipeline_toml(path: &str) -> bool {
    let path = path.replace('\\', "/");
    path.starts_with(&format!("{METADATA_DIR}/"))
        && path.ends_with(".toml")
        && !path.split('/').any(|part| part == "..")
}

/// Conflicted (unmerged) TOML files under `.pipelineos/`, relative to `root`.
pub fn pipeline_toml_conflicts(root: &Path) -> Result<Vec<String>, StudioError> {
    let raw = git(
        root,
        &["diff", "--name-only", "--diff-filter=U", "--relative", "-z"],
    )?;
    let mut files: Vec<String> = String::from_utf8_lossy(&raw)
        .split('\0')
        .filter(|path| !path.is_empty() && is_pipeline_toml(path))
        .map(str::to_owned)
        .collect();
    files.sort();
    files.dedup();
    Ok(files)
}

/// One index stage of a conflicted file; `None` when that side has no
/// version (added on one side only, or deleted).
fn stage(root: &Path, stage: u8, path: &str) -> Option<String> {
    git(root, &["show", &format!(":{stage}:./{path}")])
        .ok()
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
}

fn merge_file(
    root: &Path,
    path: &str,
    choices: &BTreeMap<String, Side>,
) -> Result<MergeResult, StudioError> {
    let base = stage(root, 1, path).unwrap_or_default();
    let (Some(ours), Some(theirs)) = (stage(root, 2, path), stage(root, 3, path)) else {
        return Err(StudioError::Invalid(format!(
            "{path} was deleted on one side; keep or delete it by hand"
        )));
    };
    merge_documents(&base, &ours, &theirs, choices)
}

fn write_and_maybe_stage(
    root: &Path,
    path: &str,
    merged: &str,
    stage_it: bool,
) -> Result<(), StudioError> {
    let full = root.join(path);
    crate::write_atomic(&full, merged)?;
    if stage_it {
        git(root, &["add", "--", path])?;
    }
    Ok(())
}

/// Merges every conflicted `.pipelineos/` TOML file key by key. Files that
/// merge cleanly are written and staged; files with real conflicts are
/// written with "ours" for those keys (no conflict markers) but left
/// unstaged, to be finished with [`apply_toml_choices`].
pub fn auto_merge_pipeline_files(root: &Path) -> Result<Vec<FileMergeReport>, StudioError> {
    let mut reports = Vec::new();
    for path in pipeline_toml_conflicts(root)? {
        let report = match merge_file(root, &path, &BTreeMap::new()) {
            Ok(result) => {
                let clean = result.conflicts.is_empty();
                match write_and_maybe_stage(root, &path, &result.merged, clean) {
                    Ok(()) => FileMergeReport {
                        path,
                        auto_resolved: clean,
                        conflicts: result.conflicts,
                        error: None,
                    },
                    Err(error) => FileMergeReport {
                        path,
                        auto_resolved: false,
                        conflicts: result.conflicts,
                        error: Some(error.to_string()),
                    },
                }
            }
            Err(error) => FileMergeReport {
                path,
                auto_resolved: false,
                conflicts: Vec::new(),
                error: Some(error.to_string()),
            },
        };
        reports.push(report);
    }
    Ok(reports)
}

/// Finishes one conflicted file with the user's per-key choices, writes it
/// and stages it. Keys without a choice keep "ours".
pub fn apply_toml_choices(
    root: &Path,
    path: &str,
    choices: &BTreeMap<String, Side>,
) -> Result<(), StudioError> {
    let path = path.replace('\\', "/");
    if !is_pipeline_toml(&path) || Path::new(&path).is_absolute() {
        return Err(StudioError::Invalid(format!(
            "{path:?} is not a Pipeline OS TOML file in this project"
        )));
    }
    let result = merge_file(root, &path, choices)?;
    write_and_maybe_stage(root, &path, &result.merged, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value(doc: &str, path: &[&str]) -> Option<toml::Value> {
        let mut current = toml::Value::Table(toml::from_str(doc).unwrap());
        for key in path {
            current = current.get(key)?.clone();
        }
        Some(current)
    }

    #[test]
    fn one_sided_and_identical_changes_merge_cleanly() {
        let base = "name = \"Game\"\nversion = 1\n\n[asset.\"Art/a.png\"]\nstatus = \"wip\"\n";
        let ours = "name = \"Game\"\nversion = 2\n\n[asset.\"Art/a.png\"]\nstatus = \"wip\"\n";
        let theirs = "name = \"Renamed\"\nversion = 2\n\n[asset.\"Art/a.png\"]\nstatus = \"approved\"\n\n[asset.\"Art/b.png\"]\nstatus = \"review\"\n";
        let result = merge_toml(base, ours, theirs).unwrap();
        assert!(result.conflicts.is_empty(), "{:?}", result.conflicts);
        let merged = &result.merged;
        assert_eq!(value(merged, &["name"]).unwrap().as_str(), Some("Renamed"));
        assert_eq!(value(merged, &["version"]).unwrap().as_integer(), Some(2));
        assert_eq!(
            value(merged, &["asset", "Art/a.png", "status"])
                .unwrap()
                .as_str(),
            Some("approved")
        );
        assert_eq!(
            value(merged, &["asset", "Art/b.png", "status"])
                .unwrap()
                .as_str(),
            Some("review")
        );
    }

    #[test]
    fn additions_union_and_deletions_apply() {
        let base = "[asset.\"a.png\"]\nstatus = \"wip\"\n\n[asset.\"old.png\"]\nstatus = \"wip\"\n";
        let ours = "[asset.\"a.png\"]\nstatus = \"wip\"\n\n[asset.\"old.png\"]\nstatus = \"wip\"\n\n[asset.\"mine.png\"]\nstatus = \"review\"\n";
        let theirs = "[asset.\"a.png\"]\nstatus = \"wip\"\nnote = \"push colors\"\n\n[asset.\"theirs.png\"]\nstatus = \"approved\"\n";
        let result = merge_toml(base, ours, theirs).unwrap();
        assert!(result.conflicts.is_empty(), "{:?}", result.conflicts);
        let m = &result.merged;
        assert!(value(m, &["asset", "mine.png"]).is_some());
        assert!(value(m, &["asset", "theirs.png"]).is_some());
        assert!(
            value(m, &["asset", "old.png"]).is_none(),
            "deleted by theirs"
        );
        assert_eq!(
            value(m, &["asset", "a.png", "note"]).unwrap().as_str(),
            Some("push colors")
        );
    }

    #[test]
    fn true_conflicts_default_to_ours_and_are_reported() {
        let base = "[asset.\"a.png\"]\nstatus = \"wip\"\nowner = \"ana\"\n";
        let ours = "[asset.\"a.png\"]\nstatus = \"approved\"\nowner = \"ana\"\n";
        let theirs = "[asset.\"a.png\"]\nstatus = \"rejected\"\nowner = \"sam\"\n";
        let result = merge_toml(base, ours, theirs).unwrap();
        assert_eq!(result.conflicts.len(), 1);
        let conflict = &result.conflicts[0];
        assert_eq!(conflict.key_path, "asset.\"a.png\".status");
        assert_eq!(conflict.ours.as_deref(), Some("\"approved\""));
        assert_eq!(conflict.theirs.as_deref(), Some("\"rejected\""));
        assert_eq!(conflict.base.as_deref(), Some("\"wip\""));
        let m = &result.merged;
        assert_eq!(
            value(m, &["asset", "a.png", "status"]).unwrap().as_str(),
            Some("approved")
        );
        assert_eq!(
            value(m, &["asset", "a.png", "owner"]).unwrap().as_str(),
            Some("sam")
        );

        let choices = BTreeMap::from([(conflict.key_path.clone(), Side::Theirs)]);
        let resolved = resolve_with_choices(base, ours, theirs, &choices).unwrap();
        assert_eq!(
            value(&resolved, &["asset", "a.png", "status"])
                .unwrap()
                .as_str(),
            Some("rejected")
        );
        let choices = BTreeMap::from([(conflict.key_path.clone(), Side::Base)]);
        let resolved = resolve_with_choices(base, ours, theirs, &choices).unwrap();
        assert_eq!(
            value(&resolved, &["asset", "a.png", "status"])
                .unwrap()
                .as_str(),
            Some("wip")
        );
    }

    #[test]
    fn delete_versus_edit_is_a_conflict() {
        let base = "a = 1\nb = 1\n";
        let ours = "b = 1\n";
        let theirs = "a = 2\nb = 1\n";
        let result = merge_toml(base, ours, theirs).unwrap();
        assert_eq!(result.conflicts.len(), 1);
        assert_eq!(result.conflicts[0].ours, None);
        assert!(value(&result.merged, &["a"]).is_none());
    }

    #[test]
    fn task_arrays_merge_entry_by_entry_and_keep_comments() {
        let base = "# Pipeline OS tasks\nschemaVersion = 1\n\n[[task]]\nid = \"build\"\nname = \"Build\"\nprogram = \"${engine}\"\nargs = [\"-a\"]\n";
        let ours = "# Pipeline OS tasks\nschemaVersion = 1\n\n[[task]]\nid = \"build\"\nname = \"Build game\"\nprogram = \"${engine}\"\nargs = [\"-a\"]\n\n[[task]]\nid = \"test\"\nname = \"Test\"\nprogram = \"x\"\n";
        let theirs = "# Pipeline OS tasks\nschemaVersion = 1\n\n[[task]]\nid = \"build\"\nname = \"Build\"\nprogram = \"${engine}\"\nargs = [\"-a\", \"-b\"]\n\n[[task]]\nid = \"render\"\nname = \"Render\"\nprogram = \"blender\"\n";
        let result = merge_toml(base, ours, theirs).unwrap();
        assert!(result.conflicts.is_empty(), "{:?}", result.conflicts);
        let m = &result.merged;
        assert!(m.starts_with("# Pipeline OS tasks"), "{m}");
        let parsed: toml::Table = toml::from_str(m).unwrap();
        let tasks = parsed["task"].as_array().unwrap();
        let ids: Vec<_> = tasks.iter().map(|t| t["id"].as_str().unwrap()).collect();
        assert_eq!(ids, ["build", "test", "render"]);
        assert_eq!(tasks[0]["name"].as_str(), Some("Build game"));
        assert_eq!(tasks[0]["args"].as_array().unwrap().len(), 2);
        // Round-trips through the real loader.
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".pipelineos")).unwrap();
        std::fs::write(dir.path().join(".pipelineos/tasks.toml"), m).unwrap();
        assert_eq!(crate::tasks::load_tasks(dir.path()).unwrap().len(), 3);
    }

    #[test]
    fn conflicting_task_fields_report_an_entry_path() {
        let base = "[[task]]\nid = \"build\"\nprogram = \"a\"\n";
        let ours = "[[task]]\nid = \"build\"\nprogram = \"b\"\n";
        let theirs = "[[task]]\nid = \"build\"\nprogram = \"c\"\n";
        let result = merge_toml(base, ours, theirs).unwrap();
        assert_eq!(result.conflicts[0].key_path, "task[id=\"build\"].program");
        let choices = BTreeMap::from([("task[id=\"build\"].program".to_owned(), Side::Theirs)]);
        let resolved = resolve_with_choices(base, ours, theirs, &choices).unwrap();
        assert!(resolved.contains("program = \"c\""), "{resolved}");
    }

    #[test]
    fn both_added_file_and_invalid_input() {
        let result = merge_toml("", "a = 1\n", "b = 2\n").unwrap();
        assert!(result.conflicts.is_empty());
        assert!(value(&result.merged, &["a"]).is_some() && value(&result.merged, &["b"]).is_some());
        assert!(merge_toml("", "a = ", "").is_err());
    }

    #[test]
    fn auto_merges_conflicted_files_in_a_git_repo() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let run = |args: &[&str]| {
            Command::new("git")
                .args(args)
                .current_dir(root)
                .env("GIT_AUTHOR_NAME", "T")
                .env("GIT_AUTHOR_EMAIL", "t@example.com")
                .env("GIT_COMMITTER_NAME", "T")
                .env("GIT_COMMITTER_EMAIL", "t@example.com")
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
        };
        if !run(&["init", "-q", "-b", "main"]) {
            eprintln!("git not available; skipping");
            return;
        }
        let write = |file: &str, body: &str| {
            let path = root.join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, body).unwrap();
        };
        let commit = |message: &str| {
            assert!(run(&["add", "-A"]));
            assert!(run(&[
                "-c",
                "commit.gpgsign=false",
                "commit",
                "-q",
                "-m",
                message
            ]));
        };
        write(
            ".pipelineos/assets.toml",
            "[asset]\n\"a.png\" = { status = \"wip\" }\n\"b.png\" = { status = \"wip\" }\n",
        );
        write(".pipelineos/project.toml", "name = \"Game\"\nversion = 1\n");
        commit("base");
        assert!(run(&["checkout", "-q", "-b", "other"]));
        write(
            ".pipelineos/assets.toml",
            "[asset]\n\"a.png\" = { status = \"approved\" }\n\"b.png\" = { status = \"wip\" }\n",
        );
        write(".pipelineos/project.toml", "name = \"Game\"\nversion = 3\n");
        commit("theirs");
        assert!(run(&["checkout", "-q", "main"]));
        write(
            ".pipelineos/assets.toml",
            "[asset]\n\"a.png\" = { status = \"wip\" }\n\"b.png\" = { status = \"review\" }\n",
        );
        write(".pipelineos/project.toml", "name = \"Game\"\nversion = 2\n");
        commit("ours");
        assert!(
            !run(&["merge", "-q", "--no-edit", "other"]),
            "expected a conflict"
        );

        let conflicts = pipeline_toml_conflicts(root).unwrap();
        assert_eq!(
            conflicts,
            [".pipelineos/assets.toml", ".pipelineos/project.toml"]
        );
        let reports = auto_merge_pipeline_files(root).unwrap();
        let assets = reports
            .iter()
            .find(|r| r.path.ends_with("assets.toml"))
            .unwrap();
        assert!(assets.auto_resolved, "{assets:?}");
        let merged = std::fs::read_to_string(root.join(".pipelineos/assets.toml")).unwrap();
        assert!(
            merged.contains("approved") && merged.contains("review"),
            "{merged}"
        );
        let project = reports
            .iter()
            .find(|r| r.path.ends_with("project.toml"))
            .unwrap();
        assert!(!project.auto_resolved);
        assert_eq!(project.conflicts[0].key_path, "version");
        assert_eq!(
            pipeline_toml_conflicts(root).unwrap(),
            [".pipelineos/project.toml"]
        );

        let choices = BTreeMap::from([("version".to_owned(), Side::Theirs)]);
        apply_toml_choices(root, ".pipelineos/project.toml", &choices).unwrap();
        assert!(pipeline_toml_conflicts(root).unwrap().is_empty());
        let project = std::fs::read_to_string(root.join(".pipelineos/project.toml")).unwrap();
        assert!(project.contains("version = 3"), "{project}");
        assert!(apply_toml_choices(root, "../evil.toml", &choices).is_err());
    }
}
