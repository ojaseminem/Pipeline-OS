# AI & Automation

Pipeline OS exposes everything the desktop app knows through three equivalent
surfaces that share one Rust application service:

1. **Desktop:** the AI tab in every project.
2. **CLI:** `vantadeck` (bundled with installed builds, next to the app).
3. **MCP server:** `vantadeck mcp`, for Claude Desktop, Claude Code and any
   Model Context Protocol client.

The CLI and MCP server read the same database as the desktop app, so they see
the same projects and detected applications. Use `--channel dev` to target the
PipelineOS Dev app instead, or `VANTADECK_DATABASE_PATH` to point anywhere.

## Connect Claude

**Claude Code**

```sh
claude mcp add pipeline-os -- "C:/Program Files/PipelineOS/vantadeck.exe" mcp
```

**Claude Desktop** (`claude_desktop_config.json`)

```json
{
  "mcpServers": {
    "pipeline-os": {
      "command": "C:/Program Files/PipelineOS/vantadeck.exe",
      "args": ["mcp"]
    }
  }
}
```

Settings → AI & automation and the project AI tab show these snippets with the
real path on your machine.

## MCP tools

| Tool | Purpose | Read-only |
| --- | --- | --- |
| `list_projects` | Registered projects (optional `query`) | yes |
| `project_brief` | Markdown brief: engine, generated folders, layout, tasks, open issues, health, team notes | yes |
| `project_config` | `.pipelineos/project.toml` as JSON | yes |
| `project_health` | Health checks with remediation | yes |
| `vcs_status` | Branch, ahead/behind, changed files | yes |
| `list_apps` | Detected engine/DCC/IDE installations | yes |
| `list_tasks` | Pipeline tasks | yes |
| `run_task` | Run a task; returns status, exit code, last 200 log lines. Only for task files you have trusted on this machine | no |
| `list_task_presets` | The project's own presets (`.pipelineos/presets.toml`) plus built-in Unity/Unreal/Godot/Blender presets ranked for the project | yes |
| `save_task` / `delete_task` | Add, replace (by id) or remove a pipeline task. Any change revokes trust, so you review it before it runs | no |
| `save_task_preset` / `delete_task_preset` | Keep reusable task templates in the project's `presets.toml` | no |
| `list_issues` | Board issues (one project, or open issues everywhere) | yes |
| `create_issue` / `update_issue` | Track work on the shared board | no |
| `sync_issues` | Pull issues from the project's connected tracker | no |
| `list_assets` | Creative assets with review status | yes |
| `list_layouts` | Folder layouts: the project's own, this machine's, and built-in | yes |
| `plan_organize` | Preview which loose files a layout would move and where (changes nothing) | yes |
| `save_layout` | Add or replace a folder layout in `.pipelineos/layouts/`; moves no files. Moving files is done by you in the app or CLI | no |
| `open_in_engine` | Open the project in its pinned engine version | no |

MCP clients ask for your approval before calling tools; read-only tools are
annotated so clients can treat them accordingly.

## The project brief

`vantadeck context <path>` (or the brief card on a project's Overview tab) produces a compact brief that keeps
agents out of trouble in game/CG projects: it names generated folders such as
`Library/` and `Intermediate/`, Unity's `.meta` rule, how to build via tasks,
and what is in flight. `--write` saves it to `.pipelineos/context.md` so agents
that read files (Claude Code, Codex, Cursor…) can pick it up. Reference it from
your `CLAUDE.md` or `AGENTS.md`:

```md
Read .pipelineos/context.md before changing anything in this project.
```

## CLI for scripts and CI

Every command takes `--json` and returns a stable envelope
(`{ "success", "command", "data", "errors" }`).

```sh
vantadeck --json project list
vantadeck tasks list D:/Projects/Voidline
vantadeck tasks plan D:/Projects/Voidline unity-build-win64   # show the exact command
vantadeck tasks run  D:/Projects/Voidline unity-build-win64 --yes
vantadeck issues add D:/Projects/Voidline --title "Fix jump" --kind bug --priority high
vantadeck issues move D:/Projects/Voidline 3f9c1a done
vantadeck issues sync D:/Projects/Voidline                    # pull from Jira/GitHub/Linear/…
vantadeck --json issues list --all                            # open work everywhere
vantadeck assets list D:/Projects/Voidline --limit 50
vantadeck context D:/Projects/Voidline --write
```

Mutating commands (`tasks run`, `tasks trust`, VCS sync/commit/push,
`assets apply-layout`) require `--yes`.

### Task trust

`.pipelineos/tasks.toml` is code from whoever committed it. Agents can only
run tasks whose exact file content you have reviewed: trust it in the app
(project → Pipeline → Review & trust) or with
`vantadeck tasks trust <path> --yes`, which prints every command first. Any
later change to the file (for example after a pull, or an agent calling
`save_task`) revokes trust until you review it again. So you can ask Claude to
"add a nightly build that runs the tests, builds Windows and zips it", review
the commands in the app, and only then run them.

### Custom pipelines and presets

A task runs one program with arguments (no shell), with `${engine}`,
`${project}`, `${projectName}`, `${date}`, `${time}` and `${engineDir}`
expanded in the program, arguments, working folder and environment values.
For a multi-step pipeline (test, build, zip, upload), point a task at a script
in the repository, for example `python Tools/build_and_zip.py`.

Presets are reusable task templates. Save any task as a project preset from
its menu in the Pipeline tab; presets live in `.pipelineos/presets.toml` and
are shared through version control. **Copy as TOML** / **Copy all** copy tasks
as `[[task]]` tables, and **Import** accepts pasted TOML or a `.toml` file
(another project's `tasks.toml` or `presets.toml` works), showing the exact
commands before anything is saved.

## Project files

| File | Shared via VCS | Contents |
| --- | --- | --- |
| `.pipelineos/project.toml` | yes | Identity, linked apps, launch profiles |
| `.pipelineos/tasks.toml` | yes | Pipeline tasks (argv, never shell strings) |
| `.pipelineos/presets.toml` | yes | The project's reusable task presets |
| `.pipelineos/issues/<id>.toml` | yes | One file per issue |
| `.pipelineos/assets.toml` | yes | Asset review status and notes |
| `.pipelineos/tracker.toml` | yes | Connected issue tracker (no secrets) |
| `.pipelineos/context.md` | your choice | Generated project brief |
| `.pipelineos/local.json` | no (gitignored) | Personal, machine-local data |

Task trust decisions and run history are stored in the app's data folder, not in
the project.
