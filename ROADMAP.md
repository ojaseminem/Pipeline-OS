# Roadmap

Roadmap items describe intent, not shipped support. Changes to major outcomes go
through a public [RFC](docs/RFC_PROCESS.md) and a roadmap pull request.

## Shipped (0.5)

- App detection on Windows, macOS and Linux for 60+ creative tools, with every
  installed version side by side and per-machine overrides.
- Portable projects (`.pipelineos/`), launch profiles, recent and pinned projects,
  project discovery on disk, and health checks.
- Source control: Git and Git LFS with diffs, history, branches (with last-updated
  times), stash on switch, merge help, pre-commit checks, GitHub pull requests and CI
  checks. A typed Perforce provider exists in the core library but isn't in the app
  or CLI yet.
- Pipeline tasks with live progress, logs, run history and a trust review; built-in
  Unity, Unreal, Godot and Blender presets; per-project presets; copy and import as TOML.
- A git-native board per project, cross-project planning, tracker sync (GitHub
  Issues, Jira, Linear, Plane, Gitea/Forgejo, ClickUp), and an asset library with
  review status.
- Folder layouts as plain JSON (built-in, per machine, per project) and a safe
  organizer with preview, engine protection and undo (unreleased).
- AI: project brief, agent launch, and an MCP server that can read projects and
  author tasks and presets (user-trusted before they run).
- Works offline; signed auto-updates; Windows, macOS (Apple Silicon and Intel) and
  Linux builds on every release; dev channel.

## Next

Ordered by how much each helps the core problem: projects that explain themselves
(see [VISION.md](docs/VISION.md)).

- **Organize, further:** a health check that spots loose files and suggests a layout;
  naming conventions (`T_Rock_Albedo`, `SM_Crate`) as part of a layout; engine-safe
  moves inside Unity `Assets/` that carry `.meta` files along.
- **Set up this project:** one screen for a newly cloned project: the engine version
  it needs, missing apps, folder layout, build tasks and Git LFS, each with a fix.
- **Studio packs:** one shareable file with a team's layouts and task presets, imported
  into any project in one step.
- **Multi-step pipelines:** chain tasks (test → build → package → upload) with
  stop-on-failure, without needing a script.
- **Art in version control:** view, review and compare versions of art files through
  Git, from the launcher.
- **Perforce in the desktop app**, validated against a live server.
- **Platform signing:** Windows Authenticode and Apple notarization, so installers
  open without a security prompt.
- **Clean-machine validation** of installers on each OS.
- **Tools Hub:** a separately governed, reviewed index of community tools.

## Toward 1.0

Stable schemas and CLI/MCP contracts, signed cross-platform installers, migration
guarantees, recovery documentation, and a supported-version policy. The remaining
external gates are tracked in [docs/release/BLOCKERS.md](docs/release/BLOCKERS.md).
