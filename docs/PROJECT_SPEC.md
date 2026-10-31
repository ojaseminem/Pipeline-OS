# Project Format v1

A Pipeline OS project is a folder containing `.pipelineos/project.toml`. The file starts
with `schema_version = 1` and contains the project name/type, linked applications,
preferred versions, launch profiles, shortcuts, VCS root, and enabled health checks.

All referenced paths are relative to the project root. Machine-specific executable
locations and user preferences live in SQLite. Writes use a fully written local
proposal, a revision check, a recovery backup, and an independent same-directory
publication inode that is atomically linked into place without replacement. If an
external file appears or changes in place during publication, it wins and Pipeline OS
preserves the immutable local proposal as `project.toml.conflict`. Failed
publication restores the prior file, and loading recovers an interrupted swap when the
canonical file is absent. Clients must reload after `ExternallyModified`.

(Projects created by earlier versions used `.vantadeck/`; it is migrated to
`.pipelineos/` automatically.)

Import recognizes Unity `ProjectSettings/ProjectVersion.txt`, root Unreal `.uproject`
files, Godot `project.godot`, and Blender/Maya source files within four directory
levels. A folder without known markers imports as `general-creative`. Import refuses
to overwrite existing `.pipelineos/project.toml` metadata.

## Other team-shared files

| File | Contents |
| --- | --- |
| `.pipelineos/tasks.toml` | Pipeline tasks: `[[task]]` tables with `id`, `name`, `kind`, `program`, `args`, optional `cwd`, `env`, `description`. Argument vectors, never shell strings. |
| `.pipelineos/presets.toml` | The project's reusable task templates: `[[preset]]` tables, same fields as tasks. |
| `.pipelineos/issues/<id>.toml` | One file per board issue, so changes merge cleanly. |
| `.pipelineos/assets.toml` | Asset review status and notes. |
| `.pipelineos/tracker.toml` | The connected issue tracker (never the token). |
| `.pipelineos/context.md` | Optional generated project brief for AI agents. |
| `.pipelineos/layouts/*.json` | The team's folder layouts (see [LAYOUTS.md](LAYOUTS.md)). |

Task variables expanded in `program`, `args`, `cwd` and `env` values: `${project}`,
`${projectName}`, `${engine}`, `${engineDir}`, `${date}` (YYYY-MM-DD), `${time}`
(HHMMSS). `cwd` must stay inside the project. `.pipelineos/local.json` holds personal
data and is gitignored.
