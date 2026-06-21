<div align="center">

# Vantadeck

**The local-first creative launcher for game development, CG, VFX, animation, and technical art.**

Manage projects, creative applications, health checks, launch profiles, and source-control workflows from one open-source desktop app and automation-friendly CLI.

[![CI](https://github.com/ojaseminem/Vantadeck/actions/workflows/ci.yml/badge.svg)](https://github.com/ojaseminem/Vantadeck/actions/workflows/ci.yml)
[![Security](https://github.com/ojaseminem/Vantadeck/actions/workflows/security.yml/badge.svg)](https://github.com/ojaseminem/Vantadeck/actions/workflows/security.yml)
[![License](https://img.shields.io/github/license/ojaseminem/Vantadeck)](LICENSE)
[![Stars](https://img.shields.io/github/stars/ojaseminem/Vantadeck?style=flat)](https://github.com/ojaseminem/Vantadeck/stargazers)
[![Forks](https://img.shields.io/github/forks/ojaseminem/Vantadeck?style=flat)](https://github.com/ojaseminem/Vantadeck/forks)
[![Issues](https://img.shields.io/github/issues/ojaseminem/Vantadeck)](https://github.com/ojaseminem/Vantadeck/issues)
[![Contributors](https://img.shields.io/github/contributors/ojaseminem/Vantadeck)](https://github.com/ojaseminem/Vantadeck/graphs/contributors)
[![Top language](https://img.shields.io/github/languages/top/ojaseminem/Vantadeck)](https://github.com/ojaseminem/Vantadeck)
[![Repository size](https://img.shields.io/github/repo-size/ojaseminem/Vantadeck)](https://github.com/ojaseminem/Vantadeck)
[![Last commit](https://img.shields.io/github/last-commit/ojaseminem/Vantadeck)](https://github.com/ojaseminem/Vantadeck/commits/main)

[Getting Started](#getting-started) | [Features](#features) | [CLI](#command-line-interface) | [Architecture](#architecture) | [Contributing](#contributing)

</div>

![Vantadeck dashboard](docs/design/vantadeck-dashboard-reference.png)

> [!IMPORTANT]
> Vantadeck is at **Windows release-candidate** stage. The Rust backend and CLI are feature-complete and tested, the Tauri 2 + React desktop app is functional, and an unsigned Windows build is the current validated target. The repository does not yet provide a published cross-platform V1: signed installers and fully validated macOS/Linux releases remain roadmap gates. See [docs/release/BLOCKERS.md](docs/release/BLOCKERS.md) for the exact remaining gates.

## Why Vantadeck?

Creative projects rarely use one application. A game project may depend on multiple engine versions, DCC packages, editors, source-control tools, local utilities, and platform-specific paths. Vantadeck provides one transparent local workspace for those dependencies without introducing another account or cloud service.

- **Local-first:** project discovery, preferences, caches, and activity remain on your machine.
- **Account-free:** no sign-in is required for core workflows.
- **Private by default:** no telemetry, advertising, analytics, or background network traffic.
- **Portable projects:** team-owned configuration uses relative paths in `.vantadeck/project.toml`.
- **Machine-aware:** absolute executable paths and local overrides remain outside source-controlled project files.
- **Automation-ready:** desktop and CLI clients use the same Rust application services.
- **Open governance:** roadmap, RFCs, security policy, funding disclosures, and contribution rules are public.

## Project Status

| Area | Status | Notes |
| --- | --- | --- |
| Windows desktop development build | Available | Native Tauri build and automated tests pass locally |
| Rust CLI | Available | Human-readable and versioned JSON output |
| Windows application detection | Implemented | Registry, Unity Hub, Epic, Steam, shortcuts, PATH, and filesystem sources |
| macOS/Linux adapters | Experimental | Fixture-tested; native validation is still required |
| Git and Git LFS | Implemented | Status, sync, commit, push, branch switching, and health diagnostics |
| Perforce | Experimental | Typed provider and fixture tests; live disposable-server validation remains |
| Tools Hub | Experimental | Validated offline cache and SHA-256 verification; hosted index is not yet launched |
| Signed public releases | Not available | Authenticode, notarization, package signing, and clean-machine testing remain gates |
| Public V1 | Not released | See [ROADMAP.md](ROADMAP.md) and [release gates](docs/release/RELEASE.md) |

## Features

### Application Discovery

- Detects and groups multiple installed versions of creative applications.
- Records evidence and confidence for every discovery.
- Supports machine-local executable overrides without modifying project metadata.
- Provides Windows Registry, Unity Hub, Epic Launcher, Steam, shortcut, PATH, and recursive filesystem sources.
- Includes experimental macOS bundle and Linux desktop, Flatpak, Snap, AppImage, PATH, and common-location adapters.
- Ships auditable manifests for Blender, Unreal Engine, Unity, Godot, VS Code, Rider, Git, Git LFS, and Perforce.

### Portable Projects

- Imports Unity, Unreal Engine, Godot, Blender/Maya, and generic creative folders.
- Stores canonical team configuration in `.vantadeck/project.toml`.
- Supports linked applications, preferred versions, fallbacks, launch profiles, shortcuts, VCS configuration, and enabled health checks.
- Uses revision-checked writes, recovery backups, no-replace publication, and conflict preservation for external edits.
- Keeps project pinning, search state, local paths, and activity in SQLite.

### Safe Launching

- Represents launches as executable, argument vector, and working directory fields.
- Never sends manifest values through a shell.
- Resolves preferred versions and explicit fallbacks from detected installations.
- Restricts native desktop launches to detected or explicitly configured executables.

### Source Control And Health

- Git status, branch switching, fast-forward pull, commit, and push operations.
- Git LFS installation, tracking, large-file, and missing-object diagnostics.
- Typed Perforce diagnosis, sync, opened files, reconcile, changelists, submit, locks, timeouts, and caller cancellation.
- Confirmation gates at both client and shared application-service boundaries for repository mutations.
- Project path, linked-application, launch-profile, Git, and LFS health reporting with stable error codes.

### Curated Tools Metadata

- Strict schema and semantic validation before metadata is cached.
- Review states for submitted, reviewed, verified, stale, and withdrawn tools.
- HTTPS source and artifact validation with platform and host metadata.
- Streaming SHA-256 verification for downloaded artifacts.
- No automatic execution of installers, scripts, or downloaded content.

### Desktop Experience

- Home dashboard, projects, applications, health, tools, and settings views.
- Real machine-local data in the native application; browser fixtures require explicit demo mode.
- System, dark, and light themes with reduced-motion support.
- Keyboard-accessible global search with `Ctrl+K`.
- Confirmation-gated Git controls and project launch profiles.

## Supported Applications

The built-in catalog currently includes:

| Category | Applications |
| --- | --- |
| Game engines | Unreal Engine, Unity, Godot |
| DCC and art | Blender |
| Code editors and IDEs | Visual Studio Code, JetBrains Rider |
| Version control | Git, Git LFS, Perforce CLI |

The manifest format is open and documented in [APP_MANIFEST_SPEC.md](docs/APP_MANIFEST_SPEC.md). New official manifests require schema validation, structured launch arguments, provenance, tests, and review.

## Getting Started

### Prerequisites

- Windows 10/11 for the currently validated desktop development workflow
- [Rust 1.96](https://www.rust-lang.org/tools/install)
- [Node.js 24](https://nodejs.org/) and npm 11
- Git
- Windows WebView2 Runtime for the Tauri desktop application

### Clone And Build

```powershell
git clone https://github.com/ojaseminem/Vantadeck.git
cd Vantadeck
npm install
cargo build --workspace
npm run build
```

### Run The Desktop Application

Development mode with hot reload:

```powershell
npm run tauri --workspace @vantadeck/desktop -- dev
```

Build and run the local debug executable:

```powershell
npm run tauri --workspace @vantadeck/desktop -- build --debug --no-bundle
.\target\debug\vantadeck-desktop.exe
```

### Run The CLI

```powershell
cargo run -p vantadeck -- --help
cargo run -p vantadeck -- --json apps list
```

## Command-Line Interface

Every JSON response uses a stable envelope containing `schemaVersion`, `command`, `success`, `data`, `warnings`, and structured `errors`.

### Applications

```powershell
# Zero-config platform scan
cargo run -p vantadeck -- --json scan apps

# Scan an explicit portable tools directory
cargo run -p vantadeck -- --json scan apps --root "D:/Creative Apps"

# Add a machine-local executable override
cargo run -p vantadeck -- --json apps override blender `
  --version 4.2.3 `
  --path "D:/Portable/Blender/blender.exe"
```

### Projects

```powershell
cargo run -p vantadeck -- --json project import "D:/Projects/MyGame" --name MyGame
cargo run -p vantadeck -- --json project show "D:/Projects/MyGame"
cargo run -p vantadeck -- --json project list --query "MyGame"
cargo run -p vantadeck -- --json project pin "D:/Projects/MyGame"
cargo run -p vantadeck -- --json project health "D:/Projects/MyGame"
cargo run -p vantadeck -- --json project launch "D:/Projects/MyGame" editor
```

### Git

Remote or mutating operations require `--yes` in non-interactive CLI usage.

```powershell
cargo run -p vantadeck -- --json project vcs "D:/Projects/MyGame" status
cargo run -p vantadeck -- --json project vcs "D:/Projects/MyGame" switch --branch develop --yes
cargo run -p vantadeck -- --json project vcs "D:/Projects/MyGame" sync --yes
cargo run -p vantadeck -- --json project vcs "D:/Projects/MyGame" commit --message "Update assets" --yes
cargo run -p vantadeck -- --json project vcs "D:/Projects/MyGame" push --yes
```

### Tools Metadata

```powershell
# Validate and cache a reviewed index file for offline use
cargo run -p vantadeck -- --json tools cache `
  "https://tools.vantadeck.org/v1/index.json" `
  --file ".\reviewed-index.json"

# Read the validated offline cache
cargo run -p vantadeck -- --json tools list `
  "https://tools.vantadeck.org/v1/index.json"

# Verify an artifact without executing it
cargo run -p vantadeck -- --json tools verify ".\tool.zip" --sha256 "<64-character-digest>"
```

See [HEADLESS_MODE.md](docs/HEADLESS_MODE.md) for automation and exit-code details.

## Project Configuration

Portable project metadata lives at `.vantadeck/project.toml`:

```toml
schema_version = 1
name = "My Game"
project_type = "game-development"
enabled_health_checks = ["project-path", "linked-apps", "git", "git-lfs"]

[[linked_apps]]
app_id = "unity"
preferred_version = "6000.0.0"
folder = "."

[[launch_profiles]]
id = "editor"
name = "Open Editor"
app_id = "unity"
arguments = ["-projectPath", "{projectRoot}"]
working_directory = "."
preferred_version = "^6000.0"
fallback_version = "2022.3.18"

[version_control]
provider = "git"
root = "."
```

Absolute executable paths, credentials, user preferences, and machine-specific overrides must not be committed to this file. See [PROJECT_SPEC.md](docs/PROJECT_SPEC.md) for the complete contract.

## Architecture

```text
React + Tauri Desktop ----+
                          |-- Rust application services
Rust CLI -----------------+         |
                                    +-- domain contracts
                                    +-- project files and migrations
                                    +-- application detection
                                    +-- safe process launching
                                    +-- Git / Git LFS / Perforce
                                    +-- health checks
                                    +-- SQLite machine-local storage
```

The Cargo workspace separates responsibilities into focused crates:

| Crate | Responsibility |
| --- | --- |
| `domain` | Versioned public types, CLI envelopes, project contracts |
| `application` | Shared orchestration used by desktop and CLI |
| `storage` | SQLite machine-local state and caches |
| `manifests` | Application and Tools Hub validation |
| `detection` | Portable and platform-specific discovery sources |
| `projects` | Import, inference, validation, recovery, and conflict-safe writes |
| `launcher` | Structured launch resolution and command creation |
| `vcs` | Git, Git LFS, and typed Perforce operations |
| `health` | Composable health checks and remediation guidance |
| `security` | Path containment and artifact integrity verification |

Read [ARCHITECTURE.md](docs/ARCHITECTURE.md) before changing ownership boundaries or public contracts.

## Privacy And Security

Vantadeck is designed to remain useful with networking disabled.

- No account, telemetry, analytics, advertising, or background networking by default.
- Network-enabled features must be independently opt-in and visibly identified.
- Credentials are never stored in SQLite or project files.
- Existing authenticated Git and Perforce CLIs are used instead of collecting credentials.
- Manifests cannot provide shell command strings.
- Remote tool metadata is treated as untrusted and validated before caching.
- Downloaded content is never executed automatically.
- Release automation produces checksums, SBOMs, provenance attestations, and Sigstore bundles; platform signing remains a release gate.

Report vulnerabilities privately according to [SECURITY.md](SECURITY.md). Do not open public issues containing exploit details, credentials, private repository URLs, or proprietary project data.

## Development

Run the complete local quality gate before opening a pull request:

```powershell
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
npm test
npm run typecheck
npm run lint
npm run build
npm run validate:contracts
```

Repository automation additionally performs:

- Windows, macOS, and Linux CI builds/tests
- dependency, advisory, source, and license policy checks
- secret scanning
- native release packaging
- SBOM, checksum, provenance, and Sigstore evidence generation
- draft-only release assembly pending manual native validation

## Repository Layout

```text
apps/
  cli/                 Rust command-line client
  desktop/             Tauri 2 + React desktop client
crates/
  application/         Shared use cases and orchestration
  detection/           Platform discovery adapters
  domain/              Public data contracts
  health/              Health checks
  launcher/            Safe process construction
  manifests/           Manifest validation
  projects/            Portable project handling
  security/            Path and artifact security
  storage/             SQLite persistence
  vcs/                 Git, Git LFS, and Perforce
docs/                  Architecture, specifications, and release runbooks
manifests/apps/         Built-in application catalog
schemas/               Versioned public JSON Schemas
rfcs/                   Public design proposals
.github/workflows/      CI, security, and release automation
```

## Roadmap

| Milestone | Primary outcome |
| --- | --- |
| Alpha 1 | Windows detection, launching, projects, Git/LFS, CLI JSON, desktop dashboard |
| Alpha 2 | Deeper workflows, background operations, activity, and Perforce validation |
| Beta | Governed Tools Hub, packaging, accessibility/performance budgets, macOS/Linux parity |
| 1.0 | Stable contracts, signed artifacts, migration guarantees, recovery documentation, supported-version policy |

Roadmap labels describe intent, not shipped support. See [ROADMAP.md](ROADMAP.md) for current evidence and remaining release gates.

## Contributing

Contributions are welcome from game developers, artists, animators, technical artists, pipeline engineers, tool authors, documentation writers, testers, and translators.

1. Read [CONTRIBUTING.md](CONTRIBUTING.md) and the [Code of Conduct](CODE_OF_CONDUCT.md).
2. Discuss behavior or public-contract changes through an issue or RFC.
3. Add focused tests and document privacy/network impact.
4. Run the complete quality gate.
5. Sign commits with `git commit -s` to certify the [Developer Certificate of Origin](DCO.txt).

Major decisions follow the public [RFC process](docs/RFC_PROCESS.md). Maintainer responsibilities and project governance are documented in [GOVERNANCE.md](GOVERNANCE.md).

## Transparency

- [Governance](GOVERNANCE.md)
- [Roadmap](ROADMAP.md)
- [Funding and relationships](FUNDING_DISCLOSURE.md)
- [Privacy statement](PRIVACY.md)
- [Security policy](SECURITY.md)
- [Support policy](SUPPORT.md)
- [Trademark policy](TRADEMARK.md)
- [Contributor ladder](docs/CONTRIBUTOR_LADDER.md)
- [Release and rollback process](docs/release/RELEASE.md)

## License

Vantadeck source code is licensed under the [Apache License 2.0](LICENSE). Documentation is licensed under CC BY 4.0 unless a file states otherwise. Contributions use DCO sign-off rather than a contributor license agreement.

---

<div align="center">

Built openly for people creating games, animation, VFX, CG, tools, and production pipelines.

</div>
