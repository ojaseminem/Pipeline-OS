# Vantadeck

Vantadeck is an Apache-2.0, local-first launcher for game development, CG, VFX,
animation, and creative production. It brings projects, installed creative apps,
health checks, and source-control workflows into one desktop app and headless CLI.

Vantadeck does not require an account, does not enable telemetry, and does not make
background network requests by default.

## Status

Vantadeck is pre-alpha. The current vertical slice provides:

- Shared Rust domain, security, project, manifest, detection, launch, VCS, health,
  storage, and application crates.
- Versioned `.vantadeck/project.toml` and JSON contracts.
- A Rust CLI with human and versioned JSON output.
- Recursive app scanning with semantic version grouping and persisted manual overrides.
- Windows launcher metadata plus macOS and Linux detection adapters.
- Portable Unity, Unreal, Godot, Blender/Maya, and generic-folder project import.
- Revision-checked project writes with recovery backups, search/pinning, and preferred/fallback launch profiles.
- Executable Git status/sync/commit/push plus Git LFS health diagnostics.
- A typed Perforce provider with confirmed mutation, timeout, and cancellation contracts.
- A Tauri 2 and React desktop app using real local data with system, dark, and light themes.
- Strict Tools Hub validation, offline caching, and SHA-256 artifact verification.
- A validated built-in catalog for representative creative applications.

## Development

Requirements: Rust 1.96, Node.js 24, and npm 11.

```powershell
npm install
cargo test --workspace
npm test
npm run build
cargo run -p vantadeck -- --json apps list
cargo run -p vantadeck -- --json scan apps --root "D:/Creative Apps"
cargo run -p vantadeck -- --json project import "D:/Projects/MyGame"
cargo run -p vantadeck -- --json project vcs "D:/Projects/MyGame" status
cargo run -p vantadeck -- --json project list --query "MyGame"
cargo run -p vantadeck -- --json project launch "D:/Projects/MyGame" editor
cargo run -p vantadeck -- --json tools list "https://tools.vantadeck.org/v1/index.json"
```

See [CONTRIBUTING.md](CONTRIBUTING.md), [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md),
and [ROADMAP.md](ROADMAP.md) before proposing a major change.

## License

Code is licensed under Apache-2.0. Documentation is licensed under CC BY 4.0 unless
noted otherwise. Contributions use Developer Certificate of Origin sign-off.
