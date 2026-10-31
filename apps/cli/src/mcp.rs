//! Model Context Protocol server (`vantadeck mcp`).
//!
//! Exposes Pipeline OS to AI agents (Claude Desktop, Claude Code, and any MCP
//! client) over newline-delimited JSON-RPC on stdio. Tools reuse the same
//! application service as the desktop app and CLI, so an agent sees exactly
//! the projects, engines, tasks, and issues the user does.
//!
//! Register with Claude Code: `claude mcp add pipeline-os -- vantadeck mcp`.

use std::{
    io::{BufRead, Write},
    path::PathBuf,
};

use serde_json::{Value, json};
use vantadeck_application::ApplicationService;
use vantadeck_studio::{
    assets,
    issues::{self, IssueKind, IssuePriority, IssueStatus, NewIssue},
    organize::{self, LayoutScope},
    tasks,
};

const PROTOCOL_VERSION: &str = "2025-06-18";

const INSTRUCTIONS: &str = "Pipeline OS manages local creative projects (Unity, Unreal, Godot, \
Blender, DCC tools). Start with list_projects, then project_brief for the project you are \
working in: it explains the engine, which folders are generated (never edit those), the \
build/render tasks, and open issues. Prefer run_task over inventing build commands. To add or \
change a pipeline, use list_task_presets and save_task (or save_task_preset for a reusable \
template); the user reviews and trusts changed tasks before they run. Track work with \
list_issues/create_issue/update_issue so the team sees it on the project board.";

/// Serves MCP on stdin/stdout until stdin closes.
pub async fn serve(service: &ApplicationService) -> std::io::Result<()> {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let response = match serde_json::from_str::<Value>(&line) {
            Ok(message) => handle_message(service, &message).await,
            Err(error) => Some(error_response(
                Value::Null,
                -32700,
                &format!("parse error: {error}"),
            )),
        };
        if let Some(response) = response {
            writeln!(stdout, "{response}")?;
            stdout.flush()?;
        }
    }
    Ok(())
}

/// Handles one JSON-RPC message; notifications produce no response.
pub async fn handle_message(service: &ApplicationService, message: &Value) -> Option<Value> {
    let id = message.get("id").cloned();
    let method = message
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let params = message.get("params").cloned().unwrap_or(Value::Null);
    let id = id?; // notifications (no id) never get a response
    let result = match method {
        "initialize" => Ok(json!({
            "protocolVersion": params
                .get("protocolVersion")
                .and_then(Value::as_str)
                .unwrap_or(PROTOCOL_VERSION),
            "capabilities": { "tools": { "listChanged": false } },
            "serverInfo": { "name": "pipeline-os", "title": "Pipeline OS", "version": env!("CARGO_PKG_VERSION") },
            "instructions": INSTRUCTIONS,
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tool_definitions() })),
        "tools/call" => {
            let name = params
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let args = params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            Ok(match call_tool(service, name, &args).await {
                Ok(value) => tool_result(&value, false),
                Err(message) => tool_result(&Value::String(message), true),
            })
        }
        _ => Err((-32601, format!("method not found: {method}"))),
    };
    Some(match result {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err((code, message)) => error_response(id, code, &message),
    })
}

fn error_response(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn tool_result(value: &Value, is_error: bool) -> Value {
    let text = match value {
        Value::String(text) => text.clone(),
        other => serde_json::to_string_pretty(other).unwrap_or_default(),
    };
    let mut result = json!({ "content": [{ "type": "text", "text": text }], "isError": is_error });
    if value.is_object() && !is_error {
        result["structuredContent"] = value.clone();
    }
    result
}

fn path_schema() -> Value {
    json!({ "type": "string", "description": "Absolute path of the project root (from list_projects)." })
}

fn tool(
    name: &str,
    title: &str,
    description: &str,
    properties: Value,
    required: &[&str],
    read_only: bool,
) -> Value {
    json!({
        "name": name,
        "title": title,
        "description": description,
        "inputSchema": { "type": "object", "properties": properties, "required": required },
        "annotations": { "readOnlyHint": read_only, "openWorldHint": false },
    })
}

pub fn tool_definitions() -> Vec<Value> {
    vec![
        tool(
            "list_projects",
            "List projects",
            "Projects registered in Pipeline OS, with paths. Optional `query` filters by name.",
            json!({ "query": { "type": "string" } }),
            &[],
            true,
        ),
        tool(
            "project_brief",
            "Project brief",
            "Markdown brief of a project: engine and version, folder layout (generated folders flagged), file mix, pipeline tasks, open issues, health, team notes. Read this first.",
            json!({ "path": path_schema() }),
            &["path"],
            true,
        ),
        tool(
            "project_config",
            "Project config",
            "The project's portable .pipelineos/project.toml as JSON (linked apps, launch profiles, tags).",
            json!({ "path": path_schema() }),
            &["path"],
            true,
        ),
        tool(
            "project_health",
            "Project health",
            "Runs health checks (paths, linked apps, Git/LFS, disk, repo size) and returns issues with remediation.",
            json!({ "path": path_schema() }),
            &["path"],
            true,
        ),
        tool(
            "vcs_status",
            "Source control status",
            "Branch, ahead/behind, and changed files for the project's Git repository.",
            json!({ "path": path_schema() }),
            &["path"],
            true,
        ),
        tool(
            "list_apps",
            "Installed creative apps",
            "Detected installations (versions and executables) of engines, DCCs, IDEs and VCS tools.",
            json!({}),
            &[],
            true,
        ),
        tool(
            "list_tasks",
            "List pipeline tasks",
            "The project's build/cook/render/export/test tasks from .pipelineos/tasks.toml.",
            json!({ "path": path_schema() }),
            &["path"],
            true,
        ),
        tool(
            "run_task",
            "Run pipeline task",
            "Runs a project task to completion and returns status, exit code, duration, and the last 200 log lines. Only works for tasks the user has trusted on this machine. Long engine builds can take many minutes.",
            json!({ "path": path_schema(), "task_id": { "type": "string" } }),
            &["path", "task_id"],
            false,
        ),
        tool(
            "list_task_presets",
            "List task presets",
            "Ready-made task templates for the project: the team's own presets from .pipelineos/presets.toml (`project: true`) first, then built-in Unity, Unreal, Godot, Blender and general presets ranked for this project.",
            json!({ "path": path_schema() }),
            &["path"],
            true,
        ),
        tool(
            "save_task",
            "Add or update pipeline task",
            "Adds a task to .pipelineos/tasks.toml, or replaces the task with the same id. A task runs `program` with `args` directly (never through a shell); variables: ${project}, ${engine}, ${engineDir}, ${projectName}, ${date}, ${time}. Changing the tasks file revokes trust, so the user must review and trust it in Pipeline OS before it can run. For multi-step pipelines, point `program` at a script in the repo.",
            json!({ "path": path_schema(), "task": task_schema() }),
            &["path", "task"],
            false,
        ),
        tool(
            "delete_task",
            "Delete pipeline task",
            "Removes a task from .pipelineos/tasks.toml.",
            json!({ "path": path_schema(), "task_id": { "type": "string" } }),
            &["path", "task_id"],
            false,
        ),
        tool(
            "save_task_preset",
            "Add or update project preset",
            "Saves a reusable task template to the project's .pipelineos/presets.toml (shared with the team via version control), replacing any preset with the same id. Presets never run by themselves; the user adds them to the pipeline from Pipeline > Add tasks.",
            json!({ "path": path_schema(), "preset": task_schema() }),
            &["path", "preset"],
            false,
        ),
        tool(
            "delete_task_preset",
            "Delete project preset",
            "Removes a preset from the project's .pipelineos/presets.toml.",
            json!({ "path": path_schema(), "preset_id": { "type": "string" } }),
            &["path", "preset_id"],
            false,
        ),
        tool(
            "list_issues",
            "List issues",
            "Issues on a project's board, or open issues across all projects when `path` is omitted. Optional `status`: backlog, todo, in-progress, review, done.",
            json!({ "path": path_schema(), "status": { "type": "string" } }),
            &[],
            true,
        ),
        tool(
            "create_issue",
            "Create issue",
            "Adds an issue to the project board (a file under .pipelineos/issues, shared via version control).",
            json!({
                "path": path_schema(),
                "title": { "type": "string" },
                "body": { "type": "string", "description": "Markdown description." },
                "kind": { "type": "string", "enum": ["task", "bug", "feature", "art", "design"] },
                "priority": { "type": "string", "enum": ["none", "low", "medium", "high", "urgent"] },
                "assignee": { "type": "string" },
                "labels": { "type": "array", "items": { "type": "string" } }
            }),
            &["path", "title"],
            false,
        ),
        tool(
            "update_issue",
            "Update issue",
            "Changes an issue's status, title, body, priority, or assignee.",
            json!({
                "path": path_schema(),
                "id": { "type": "string" },
                "status": { "type": "string", "enum": ["backlog", "todo", "in-progress", "review", "done"] },
                "title": { "type": "string" },
                "body": { "type": "string" },
                "priority": { "type": "string", "enum": ["none", "low", "medium", "high", "urgent"] },
                "assignee": { "type": "string" }
            }),
            &["path", "id"],
            false,
        ),
        tool(
            "sync_issues",
            "Sync issues from tracker",
            "Pulls issues from the project's connected tracker (GitHub, Gitea/Forgejo, Jira, Linear, Plane, ClickUp) into the board.",
            json!({ "path": path_schema() }),
            &["path"],
            false,
        ),
        tool(
            "list_layouts",
            "List folder layouts",
            "Folder layouts available to the project: its own (.pipelineos/layouts/*.json, shared with the team), this machine's library, and built-in ones (game team, studio pipeline, film/VFX, client project). Each has folders and sorting rules.",
            json!({ "path": path_schema() }),
            &["path"],
            true,
        ),
        tool(
            "plan_organize",
            "Preview organizing a project",
            "Shows which loose files a folder layout would move and where, without changing anything. Engine-managed folders (Unity Assets, Unreal Content, Godot imports) are never moved. Moving files is done by the user in Pipeline OS.",
            json!({ "path": path_schema(), "layout_id": { "type": "string" } }),
            &["path", "layout_id"],
            true,
        ),
        tool(
            "save_layout",
            "Add or update a project folder layout",
            "Saves a folder layout to .pipelineos/layouts/<id>.json so the whole team can use it. `layout` is JSON: {\"id\", \"name\", \"description\", \"folders\": [\"Art/Concept\", ...], \"rules\": [{\"to\": \"Art/Concept\", \"match\": [\"*concept*\"], \"kinds\": [\"image\"]}], \"keep\": [\"README*\"]}. Kinds: image, source, model, scene, cache, animation, video, audio, document. Saving moves no files.",
            json!({ "path": path_schema(), "layout": { "type": "object" } }),
            &["path", "layout"],
            false,
        ),
        tool(
            "list_assets",
            "List creative assets",
            "Concept art, references, source art, models, audio and video in the project (engine caches and git-ignored files excluded), newest first, with review status.",
            json!({ "path": path_schema(), "limit": { "type": "integer", "minimum": 1, "maximum": 2000 } }),
            &["path"],
            true,
        ),
        tool(
            "open_in_engine",
            "Open project in engine",
            "Opens the project in its engine at the configured (or newest installed) version.",
            json!({ "path": path_schema() }),
            &["path"],
            false,
        ),
    ]
}

fn task_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "id": { "type": "string", "description": "1-48 lowercase letters, digits, or dashes." },
            "name": { "type": "string" },
            "kind": { "type": "string", "enum": ["build", "test", "render", "export", "cook", "script"] },
            "description": { "type": "string" },
            "program": { "type": "string", "description": "Executable or ${engine}." },
            "args": { "type": "array", "items": { "type": "string" } },
            "cwd": { "type": "string", "description": "Working directory relative to the project root." },
            "env": { "type": "object", "additionalProperties": { "type": "string" } }
        },
        "required": ["id", "name", "program"]
    })
}

fn task_arg(args: &Value, key: &str) -> Result<tasks::TaskDefinition, String> {
    let value = args
        .get(key)
        .ok_or_else(|| format!("missing required argument `{key}`"))?;
    serde_json::from_value(value.clone()).map_err(|e| format!("invalid `{key}`: {e}"))
}

/// Inserts `item`, replacing an existing entry with the same id in place.
fn upsert(list: &mut Vec<tasks::TaskDefinition>, item: tasks::TaskDefinition) -> &'static str {
    match list.iter_mut().find(|existing| existing.id == item.id) {
        Some(existing) => {
            *existing = item;
            "updated"
        }
        None => {
            list.push(item);
            "added"
        }
    }
}

fn str_arg<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key)
        .and_then(Value::as_str)
        .filter(|v| !v.is_empty())
}

fn path_arg(args: &Value) -> Result<PathBuf, String> {
    str_arg(args, "path")
        .map(PathBuf::from)
        .ok_or_else(|| "missing required argument `path`".to_owned())
}

fn enum_arg<T: serde::de::DeserializeOwned>(args: &Value, key: &str) -> Result<Option<T>, String> {
    str_arg(args, key)
        .map(|value| {
            serde_json::from_value(Value::String(value.to_owned()))
                .map_err(|_| format!("unsupported {key} {value:?}"))
        })
        .transpose()
}

fn to_value<T: serde::Serialize>(value: T) -> Result<Value, String> {
    serde_json::to_value(value).map_err(|e| e.to_string())
}

async fn call_tool(
    service: &ApplicationService,
    name: &str,
    args: &Value,
) -> Result<Value, String> {
    let err = |e: &dyn std::fmt::Display| e.to_string();
    match name {
        "list_projects" => {
            let projects = service
                .search_projects(str_arg(args, "query").unwrap_or(""), 500)
                .await
                .map_err(|e| err(&e))?;
            to_value(json!({ "projects": projects }))
        }
        "project_brief" => {
            let brief = service
                .project_brief(&path_arg(args)?)
                .await
                .map_err(|e| err(&e))?;
            Ok(Value::String(brief))
        }
        "project_config" => {
            to_value(vantadeck_projects::load_project(&path_arg(args)?).map_err(|e| err(&e))?)
        }
        "project_health" => {
            to_value(json!({ "issues": service.project_health(&path_arg(args)?).await }))
        }
        "vcs_status" => to_value(
            service
                .vcs_status(&path_arg(args)?)
                .await
                .map_err(|e| err(&e))?,
        ),
        "list_apps" => {
            let mut apps = Vec::new();
            for id in crate::KNOWN_APP_IDS {
                let installations = service
                    .detected_installations(id)
                    .await
                    .map_err(|e| err(&e))?;
                if !installations.is_empty() {
                    apps.push(json!({ "id": id, "installations": installations }));
                }
            }
            to_value(json!({ "apps": apps }))
        }
        "list_tasks" => {
            to_value(json!({ "tasks": tasks::load_tasks(&path_arg(args)?).map_err(|e| err(&e))? }))
        }
        "list_task_presets" => to_value(json!({
            "presets": vantadeck_studio::task_presets::task_presets(&path_arg(args)?)
        })),
        "save_task" => {
            let path = path_arg(args)?;
            let task = task_arg(args, "task")?;
            let mut list = tasks::load_tasks(&path).map_err(|e| err(&e))?;
            let id = task.id.clone();
            let action = upsert(&mut list, task);
            tasks::save_tasks(&path, &list).map_err(|e| err(&e))?;
            to_value(json!({
                "task": id,
                "action": action,
                "note": "Saved to .pipelineos/tasks.toml. The tasks file changed, so the user must review and trust it in Pipeline OS (project > Pipeline) before any task runs."
            }))
        }
        "delete_task" => {
            let path = path_arg(args)?;
            let id = str_arg(args, "task_id").ok_or("missing required argument `task_id`")?;
            let mut list = tasks::load_tasks(&path).map_err(|e| err(&e))?;
            let before = list.len();
            list.retain(|task| task.id != id);
            if list.len() == before {
                return Err(format!("no task {id:?} in this project"));
            }
            tasks::save_tasks(&path, &list).map_err(|e| err(&e))?;
            to_value(json!({ "deleted": id }))
        }
        "save_task_preset" => {
            let path = path_arg(args)?;
            let preset = task_arg(args, "preset")?;
            let mut list = tasks::load_project_presets(&path).map_err(|e| err(&e))?;
            let id = preset.id.clone();
            let action = upsert(&mut list, preset);
            tasks::save_project_presets(&path, &list).map_err(|e| err(&e))?;
            to_value(json!({ "preset": id, "action": action }))
        }
        "delete_task_preset" => {
            let path = path_arg(args)?;
            let id = str_arg(args, "preset_id").ok_or("missing required argument `preset_id`")?;
            let mut list = tasks::load_project_presets(&path).map_err(|e| err(&e))?;
            let before = list.len();
            list.retain(|preset| preset.id != id);
            if list.len() == before {
                return Err(format!("no preset {id:?} in this project"));
            }
            tasks::save_project_presets(&path, &list).map_err(|e| err(&e))?;
            to_value(json!({ "deleted": id }))
        }
        "run_task" => {
            let path = path_arg(args)?;
            let task_id = str_arg(args, "task_id").ok_or("missing required argument `task_id`")?;
            // Agents may only run task files the user has reviewed: shared
            // tasks.toml content is code from whoever committed it.
            if !crate::trust_store().is_some_and(|store| store.is_trusted(&path)) {
                return Err(format!(
                    "This project's tasks haven't been trusted on this machine. Ask the user to review them in Pipeline OS (project > Pipeline > Review & trust) or run `vantadeck tasks trust \"{}\" --yes`.",
                    path.display()
                ));
            }
            let (_, resolved) = service
                .resolve_project_task(&path, task_id)
                .await
                .map_err(|e| err(&e))?;
            // The file may have changed while it was being read.
            if !crate::trust_store().is_some_and(|store| store.is_trusted(&path)) {
                return Err(
                    "This project's tasks changed while loading; review and trust them again."
                        .into(),
                );
            }
            let command_line = resolved.display();
            let (outcome, log) = crate::run_resolved_task(resolved, false)
                .await
                .map_err(|e| err(&e))?;
            to_value(json!({ "commandLine": command_line, "outcome": outcome, "log": log }))
        }
        "list_issues" => {
            let status = str_arg(args, "status")
                .map(|s| IssueStatus::parse(s).ok_or_else(|| format!("unknown status {s:?}")))
                .transpose()?;
            match str_arg(args, "path") {
                Some(path) => {
                    let (list, problems) =
                        issues::list_issues(&PathBuf::from(path)).map_err(|e| err(&e))?;
                    let list: Vec<_> = list
                        .into_iter()
                        .filter(|i| status.is_none_or(|s| i.status == s))
                        .collect();
                    to_value(json!({ "issues": list, "problems": problems }))
                }
                None => {
                    let list = service
                        .open_issues_everywhere()
                        .await
                        .map_err(|e| err(&e))?;
                    let list: Vec<_> = list
                        .into_iter()
                        .filter(|i| status.is_none_or(|s| i.issue.status == s))
                        .collect();
                    to_value(json!({ "issues": list }))
                }
            }
        }
        "create_issue" => {
            let path = path_arg(args)?;
            let new = NewIssue {
                title: str_arg(args, "title")
                    .ok_or("missing required argument `title`")?
                    .to_owned(),
                body: str_arg(args, "body").unwrap_or_default().to_owned(),
                kind: enum_arg::<IssueKind>(args, "kind")?,
                priority: enum_arg::<IssuePriority>(args, "priority")?,
                assignee: str_arg(args, "assignee").map(str::to_owned),
                labels: args
                    .get("labels")
                    .and_then(Value::as_array)
                    .map(|labels| {
                        labels
                            .iter()
                            .filter_map(Value::as_str)
                            .map(str::to_owned)
                            .collect()
                    })
                    .unwrap_or_default(),
                ..Default::default()
            };
            to_value(issues::create_issue(&path, new).map_err(|e| err(&e))?)
        }
        "update_issue" => {
            let path = path_arg(args)?;
            let id = str_arg(args, "id").ok_or("missing required argument `id`")?;
            let mut issue = issues::get_issue(&path, id).map_err(|e| err(&e))?;
            if let Some(status) = str_arg(args, "status") {
                issue.status = IssueStatus::parse(status)
                    .ok_or_else(|| format!("unknown status {status:?}"))?;
            }
            if let Some(title) = str_arg(args, "title") {
                issue.title = title.to_owned();
            }
            if let Some(body) = args.get("body").and_then(Value::as_str) {
                issue.body = body.to_owned();
            }
            if let Some(priority) = enum_arg::<IssuePriority>(args, "priority")? {
                issue.priority = priority;
            }
            if let Some(assignee) = args.get("assignee").and_then(Value::as_str) {
                issue.assignee = (!assignee.is_empty()).then(|| assignee.to_owned());
            }
            to_value(issues::update_issue(&path, issue).map_err(|e| err(&e))?)
        }
        "sync_issues" => to_value(
            vantadeck_studio::trackers::sync_tracker(&path_arg(args)?)
                .await
                .map_err(|e| err(&e))?,
        ),
        "list_layouts" => to_value(
            organize::list_layouts(
                Some(&path_arg(args)?),
                crate::DATA_DIR.get().map(PathBuf::as_path),
            )
            .map_err(|e| err(&e))?,
        ),
        "plan_organize" => {
            let root = path_arg(args)?;
            let id = args
                .get("layout_id")
                .and_then(Value::as_str)
                .ok_or("layout_id is required")?;
            let layout =
                organize::find_layout(Some(&root), crate::DATA_DIR.get().map(PathBuf::as_path), id)
                    .map_err(|e| err(&e))?;
            to_value(organize::plan_organize(&root, &layout).map_err(|e| err(&e))?)
        }
        "save_layout" => {
            let root = path_arg(args)?;
            let mut raw = args.get("layout").cloned().ok_or("layout is required")?;
            if let Some(object) = raw.as_object_mut() {
                object
                    .entry("format")
                    .or_insert(json!(organize::LAYOUT_FORMAT));
                object.entry("version").or_insert(json!(1));
            }
            let layout = organize::parse_layout(&raw.to_string()).map_err(|e| err(&e))?;
            let file = organize::save_layout(Some(&root), None, LayoutScope::Project, &layout)
                .map_err(|e| err(&e))?;
            to_value(json!({ "layout": layout, "file": file }))
        }
        "list_assets" => {
            let limit = args
                .get("limit")
                .and_then(Value::as_u64)
                .unwrap_or(200)
                .clamp(1, 2000) as usize;
            to_value(assets::index_assets(&path_arg(args)?, limit).map_err(|e| err(&e))?)
        }
        "open_in_engine" => to_value(
            service
                .open_project_in_engine(&path_arg(args)?, None)
                .await
                .map_err(|e| err(&e))?,
        ),
        other => Err(format!("unknown tool {other:?}")),
    }
}
