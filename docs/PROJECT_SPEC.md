# Project Format v1

A Vantadeck project is a folder containing `.vantadeck/project.toml`. The file starts
with `schema_version = 1` and contains the project name/type, linked applications,
preferred versions, launch profiles, shortcuts, VCS root, and enabled health checks.

All referenced paths are relative to the project root. Machine-specific executable
locations and user preferences live in SQLite. Writes use a fully written local
proposal, a revision check, a recovery backup, and an independent same-directory
publication inode that is atomically linked into place without replacement. If an
external file appears or changes in place during publication, it wins and Vantadeck
preserves the immutable local proposal as `project.toml.vantadeck-conflict`. Failed
publication restores the prior file, and loading recovers an interrupted swap when the
canonical file is absent. Clients must reload after `ExternallyModified`.

Import recognizes Unity `ProjectSettings/ProjectVersion.txt`, root Unreal `.uproject`
files, Godot `project.godot`, and Blender/Maya source files within four directory
levels. A folder without known markers imports as `general-creative`. Import refuses
to overwrite existing `.vantadeck/project.toml` metadata.
