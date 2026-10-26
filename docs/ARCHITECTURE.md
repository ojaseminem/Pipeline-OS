# Architecture

Pipeline OS is a Rust workspace. The Tauri 2 desktop app (React UI) and the
`vantadeck` CLI, which also serves MCP to AI apps, call the same application
services, so behavior and data are identical across all three surfaces. (The
crates and CLI keep the project's original name, Vantadeck.)

```text
Desktop app (Tauri 2 + React) ──┐
                                ├── vantadeck-application ── storage (SQLite)
CLI + MCP server (vantadeck) ───┘           │
                                            ├── detection   app discovery per OS
                                            ├── projects    import, validation, safe writes
                                            ├── launcher    structured, shell-free launches
                                            ├── vcs         Git, Git LFS, Perforce
                                            ├── studio      tasks, presets, board, assets,
                                            │               trackers, GitHub, brief
                                            ├── health      project health checks
                                            ├── manifests   app manifest validation
                                            └── security    path containment, artifact checks
```

`vantadeck-application::ApplicationService` owns orchestration: manifest catalog
scanning, override precedence, project import and registration, Git operations,
pipeline task resolution and combined health results. Clients don't rebuild these
workflows from lower-level crates.

## State

- **Team-owned, in the repo:** `.pipelineos/` holds `project.toml`, `tasks.toml`,
  `presets.toml`, `issues/`, `assets.toml` and `tracker.toml`. Paths are relative;
  no machine paths or credentials.
- **Machine-local:** SQLite in the OS data folder holds registered projects,
  overrides, preferences, activity and caches. Task trust decisions and run history
  live in the same data folder. Tokens live in the OS keychain.

SQLite uses idempotent migrations. App scan results replace prior results per
application so removed installations never linger.

## Safety

Launches and pipeline tasks are an executable plus an argument vector and a working
directory, never a shell string. Tasks from a repository (or written by an AI agent
over MCP) run only after the user trusts the exact file content; any change revokes
trust. Manifests are declarative and cannot install or run code.

## Desktop

The React UI talks to typed Tauri commands. Reads and actions run against the local
backend regardless of network state, so the app works offline. Long operations
stream progress from Rust in batched events. The browser build can run against
deterministic demo fixtures (`?demo=true`) for UI work and screenshots.

See [ADR 0001](adr/0001-keep-tauri.md) for why the app uses Tauri.
