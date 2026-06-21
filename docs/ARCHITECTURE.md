# Architecture

Vantadeck uses a Rust workspace so the Tauri desktop app and `vantadeck` CLI call the
same application services. Domain types contain versioned contracts. Platform and
integration crates implement detection, launching, VCS, health, project files,
security, manifests, and SQLite persistence behind narrow interfaces.

`vantadeck-application::ApplicationService` owns orchestration for both client
surfaces: manifest catalog scanning, override precedence, project import and
registration, Git operations, and combined health results. The CLI does not recreate
these workflows from lower-level crates.

`.vantadeck/project.toml` is canonical team-owned state. SQLite stores machine-local
paths, overrides, preferences, activity, and disposable caches. Absolute machine
paths and credentials never enter project files.

SQLite initialization uses idempotent migrations for preferences, manual overrides,
detected installations, registered projects, and activity history. App scan results
replace prior results per application so removed installations cannot remain stale.

Launches are executable plus argument vectors and working directory. They never pass
through a shell. Declarative manifests are the only v1 extension surface.

The React UI depends on typed Tauri commands. Browser development can use deterministic
fixture snapshots; production commands are supplied by `vantadeck-application`.
