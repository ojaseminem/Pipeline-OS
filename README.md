<div align="center">

# Pipeline OS

### Your whole game, film or VFX pipeline — in one local, open-source app.

Find every creative app and version on your machine, open projects in the right one, run builds and renders, track the work, and drive Git — from one desktop app and a scriptable CLI.<br>
**No account. No cloud. No telemetry. Works offline.**

[![Latest release](https://img.shields.io/github/v/release/ojaseminem/Pipeline-OS?label=download&color=2bb3a6)](https://github.com/ojaseminem/Pipeline-OS/releases/latest)
[![CI](https://github.com/ojaseminem/Pipeline-OS/actions/workflows/ci.yml/badge.svg)](https://github.com/ojaseminem/Pipeline-OS/actions/workflows/ci.yml)
[![Security](https://github.com/ojaseminem/Pipeline-OS/actions/workflows/security.yml/badge.svg)](https://github.com/ojaseminem/Pipeline-OS/actions/workflows/security.yml)
[![License: MIT](https://img.shields.io/github/license/ojaseminem/Pipeline-OS)](LICENSE)
![Windows · macOS · Linux](https://img.shields.io/badge/platforms-Windows%20·%20macOS%20·%20Linux-555)

[Download](#download) · [Tour](#a-quick-tour) · [Features](#features) · [Works offline](#works-offline) · [CLI](#command-line) · [Build from source](#build-from-source) · [Contributing](#contributing)

<br>

<img src="docs/screenshots/home.png" alt="Pipeline OS home: continue where you left off, pinned projects, a running build, open work and project health" width="100%">

</div>

## Why Pipeline OS

A single game or film project leans on a dozen tools: an engine (often one specific version), DCC apps, editors, source control, build scripts, a tracker. Pipeline OS puts all of it in one place, on your machine.

| | |
| --- | --- |
| 🧭 **One home for every project** | Pinned and recent projects, what changed, health warnings and open work, the moment you open the app. |
| 🎯 **The right version, every time** | Detects 60+ creative apps with every installed version side by side, and opens each project in the version it needs. |
| ⚙️ **One-click pipeline** | Builds, cooks, renders, exports and tests from templates, with live progress, logs and history. |
| 🌿 **Git without the friction** | Changes, diffs, branches (with when each was last updated), merges, pull requests and CI checks. |
| 🗂️ **Work tracking that lives in the repo** | A Kanban board and asset reviews stored as plain files, so they sync through Git. |
| 🤖 **AI-ready** | A project brief and a built-in MCP server let Claude, Cursor, Codex and others work with your projects safely. |
| 🔒 **Yours** | MIT-licensed, local-first, no account, no telemetry, and it [works offline](#works-offline). |

## Download

| Platform | Get it |
| --- | --- |
| Windows 10/11 (x64) | `PipelineOS_<version>_x64-setup.exe` or `.msi` |
| macOS (Apple Silicon) | `PipelineOS_<version>_aarch64.dmg` |
| Linux (x64) | `.AppImage`, `.deb` or `.rpm` |

All on the **[latest release](https://github.com/ojaseminem/Pipeline-OS/releases/latest)**. Once installed, the app updates itself.

Installers aren't code-signed by Microsoft or Apple yet, so the first launch needs one extra step:

- **Windows:** SmartScreen shows "unknown publisher". Choose *More info → Run anyway*.
- **macOS:** right-click the app → *Open* → *Open*, or run `xattr -dr com.apple.quarantine "/Applications/PipelineOS.app"`.

Update files are signed, and every release ships checksums (`SHA256SUMS.txt`), SBOMs and Sigstore build provenance.

**Dev builds.** To try the newest work early, install **PipelineOS Dev** from the rolling [`dev-latest` pre-release](https://github.com/ojaseminem/Pipeline-OS/releases/tag/dev-latest) (Windows, macOS Apple Silicon and Intel). It installs alongside the stable app with its own data, and updates itself from the `dev` branch.

## A quick tour

### Source control, built for creative projects
Stage, diff and commit with pre-commit checks, then sync in one click. The branch menu shows when every branch was last updated, and keeps *New branch* and *Merge* pinned at the bottom.

<img src="docs/screenshots/source-control.png" alt="Source control: changed files with a colored diff of PlayerController.cs" width="100%">

<table>
  <tr>
    <td width="50%"><img src="docs/screenshots/branches.png" alt="Branch menu with last-updated times and pinned New branch and Merge actions"></td>
    <td width="50%"><img src="docs/screenshots/project-overview.png" alt="Project overview with active time and health checks"></td>
  </tr>
  <tr>
    <td align="center"><sub>Branches with last-updated times</sub></td>
    <td align="center"><sub>Active time and project health</sub></td>
  </tr>
</table>

### One-click pipeline
Unity, Unreal, Godot and Blender templates (or your own commands) become buttons. Runs show live progress and logs, and history stays with the project. Tasks that arrive through the repo must be reviewed and trusted on your machine before they run.

<img src="docs/screenshots/pipeline.png" alt="Pipeline tasks: a Windows build running at 64% with run history" width="100%">

### Plan the work where the work lives
Every project gets a Kanban board stored as plain files in `.pipelineos/issues/`, so it merges cleanly through Git. Planning rolls up progress, workload and recent changes across all of your projects.

<table>
  <tr>
    <td width="50%"><img src="docs/screenshots/board.png" alt="Project board with backlog, to do, in progress, review and done columns"></td>
    <td width="50%"><img src="docs/screenshots/planning.png" alt="Planning dashboard with progress by project and team workload"></td>
  </tr>
  <tr>
    <td align="center"><sub>Board per project</sub></td>
    <td align="center"><sub>Planning across every project</sub></td>
  </tr>
</table>

### Assets and reviews
Concept art, references, models, audio and video in one library, with shared review status (WIP, In review, Approved) and studio folder layouts.

<img src="docs/screenshots/assets.png" alt="Asset library with previews and review status badges" width="100%">

### Made for AI agents
Open Claude Code, Codex, Gemini or a terminal right in the project. A generated brief tells agents the engine, how to build and what to leave alone, and the built-in MCP server connects your AI apps in one click.

<img src="docs/screenshots/ai.png" alt="AI tab: launch agents, connect AI apps over MCP, and the generated project brief" width="100%">

### Every app, every version
Pipeline OS finds the engines, DCCs, editors and tools you already have, with each installed version side by side, ready to launch or pin to Quick Launch.

<img src="docs/screenshots/applications.png" alt="Applications grouped by category with every installed version" width="100%">

### Make it yours
11 themes (including Glass, High contrast and light themes), custom accent colors, density and motion settings.

<table>
  <tr>
    <td width="50%"><img src="docs/screenshots/settings.png" alt="Appearance settings with theme previews and accent colors"></td>
    <td width="50%"><img src="docs/screenshots/home-light.png" alt="Home screen in the Daylight theme"></td>
  </tr>
  <tr>
    <td align="center"><sub>Themes and accents</sub></td>
    <td align="center"><sub>Daylight theme</sub></td>
  </tr>
</table>

<sub>Screenshots show the real app with sample projects.</sub>

## Features

### Apps and projects
- **Finds your creative apps automatically**, including multiple versions side by side: 60+ apps such as Unity, Unreal Engine, Godot, Blender, Maya, Houdini, 3ds Max, Cinema 4D, ZBrush, Substance, Photoshop, Krita, Aseprite, Spine, DaVinci Resolve, Nuke, Wwise, FMOD, Rider, VS Code, Visual Studio, Xcode, Git and Perforce.
- **Detects on every OS**: Windows (Registry, Unity Hub, Epic, Steam, shortcuts), macOS (app bundles in Applications folders) and Linux (desktop entries, Flatpak, Snap, AppImage), plus PATH and common install folders.
- **Imports projects** for Unity, Unreal, Godot, Blender/Maya and any creative folder, and finds projects already on your disk.
- **Opens projects in the right version**, with preferred and fallback versions, launch profiles, and per-machine overrides that are never committed.
- **Suggests the tools a project uses** (Maya, Substance, Houdini…) from the files it contains.
- **Pins, search and recent projects**, with a quick filter next to *Pinned / Recent*.

### Studio workflows
- **Pipeline tasks.** One-click builds, cooks, renders, exports and tests from templates (Unity batch mode, Unreal BuildCookRun, Godot export, Blender render, or your own). Live logs, cancel, and run history. Commands that arrive through the repo must be reviewed and trusted on your machine before they run.
- **Board.** A Kanban board per project, stored as one small file per issue under `.pipelineos/issues/`, so it merges cleanly through Git. *Open work* on Home and *Planning* show everything across projects. Optional sync with GitHub Issues, Jira, Linear and others.
- **Asset library.** Concept art, references, models, audio and video with review status (WIP / In review / Approved), notes and studio folder layouts.
- **Engine activity and time.** See what the engine is doing from its logs (lighting builds, shader compiles, cooks), and how long you've worked on each project (kept locally).

### Source control
- Git status, diffs, history, branches, stash on switch, commit, pull, push and one-click sync, in the style of GitHub Desktop.
- Every branch list shows when each branch was last updated; the branch menu sizes to your branch names and keeps *New branch* and *Merge* pinned.
- Merge-conflict help (including automatic merging of TOML settings), pre-commit checks, and guarded branch deletion.
- GitHub pull requests and CI checks for the current branch, with failed-job logs. This uses an optional token kept in your OS keychain.
- Git LFS checks that only warn when a project actually has large files.
- Perforce support (experimental; not yet tested against a live server).

### AI-ready
- Start Claude Code, Codex, Gemini, Aider or a terminal inside any project.
- A generated **project brief** tells agents the engine, how to build, and which folders to leave alone.
- A built-in **MCP server** (`vantadeck mcp`) lets Claude Desktop, Claude Code, Cursor and others work with your projects, tasks and issues. The app can set this up for you. See [AI_INTEGRATION.md](docs/AI_INTEGRATION.md).

### Desktop experience
- 11 themes, including light, dark, glass and high-contrast, plus custom accent colors and reduced motion.
- A command palette (`Ctrl/⌘+K`) for everything, plus shortcuts for new project, undo/redo and back/forward. Press `?` to see them all.
- An activity screen showing every running task across projects.

## Works offline

Pipeline OS runs entirely on your machine. With no internet connection you can still:

- browse, search, pin, import and open projects, and launch apps;
- detect apps and discover projects on disk;
- run pipeline tasks and see their logs and history;
- use the board, asset library, project brief and MCP server;
- use Git locally: status, diffs, history, branches, stash and commits.

Only these features need a connection:

| Feature | Offline behavior |
| --- | --- |
| Git fetch / pull / push / sync | Tries once more, then says the remote isn't reachable. Local commits are safe. |
| GitHub pull requests and checks | Shows "GitHub isn't reachable", and refreshes on its own once you're back online. |
| Tracker sync (GitHub Issues, Jira, Linear…) | Says the tracker isn't reachable; your local board is unaffected. |
| Update check (at launch if auto-update is on, or from Settings) | Fails quietly at launch. A manual check says it couldn't reach the server instead of claiming you're up to date. |

## Command line

The `vantadeck` CLI ships inside the desktop app (it powers the MCP server) and can also be built from source with `cargo run -p vantadeck`. It exposes the same features. Every command supports `--json`, which returns a stable envelope (`schemaVersion`, `command`, `success`, `data`, `warnings`, `errors`). Commands that change something or touch a remote require `--yes`.

```bash
vantadeck scan apps                                   # detect installed apps
vantadeck project import ~/Projects/MyGame            # add a project
vantadeck project launch ~/Projects/MyGame editor     # open it with a launch profile
vantadeck project vcs ~/Projects/MyGame sync --yes    # fetch, pull and push
vantadeck tasks run ~/Projects/MyGame build --yes     # run a pipeline task
vantadeck issues list ~/Projects/MyGame               # the project's board
vantadeck context ~/Projects/MyGame                   # AI-ready project brief
vantadeck discover                                    # projects not added yet
vantadeck mcp                                         # serve AI agents over MCP (stdio)
```

Run `vantadeck --help` or `vantadeck <command> --help` for everything. Automation details and exit codes are in [HEADLESS_MODE.md](docs/HEADLESS_MODE.md).

## Project files

Shared, team-owned settings live in the project's `.pipelineos/` folder and are meant to be committed:

```toml
# .pipelineos/project.toml
schema_version = 1
name = "My Game"
project_type = "game-development"

[[linked_apps]]
app_id = "unity"
preferred_version = "6000.0.0"

[[launch_profiles]]
id = "editor"
name = "Open Editor"
app_id = "unity"
arguments = ["-projectPath", "{projectRoot}"]
```

Pipeline tasks live in `.pipelineos/tasks.toml` and issues in `.pipelineos/issues/`. Absolute paths, credentials and personal preferences never go in these files; they stay in the app's local data. The full contract is in [PROJECT_SPEC.md](docs/PROJECT_SPEC.md). Older `.vantadeck/` folders are migrated automatically.

## Build from source

Requirements: [Rust 1.96](https://www.rust-lang.org/tools/install), [Node.js 24](https://nodejs.org/) with npm 11, and Git. Also install the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your OS (WebView2 on Windows, Xcode Command Line Tools on macOS, WebKitGTK on Linux).

```bash
git clone https://github.com/ojaseminem/Pipeline-OS.git
cd Pipeline-OS
npm install

# Desktop app with hot reload
npm run tauri --workspace @vantadeck/desktop -- dev

# CLI
cargo run -p vantadeck -- --help
```

Before opening a pull request, run the same checks CI runs:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
npm run typecheck && npm run lint && npm test && npm run build
```

## Architecture

```text
Desktop app (Tauri 2 + React) ──┐
                                ├── Rust application services ── SQLite (local state)
CLI + MCP server (vantadeck) ───┘           │
                                            ├── detection   app discovery per OS
                                            ├── projects    import, validation, safe writes
                                            ├── launcher    structured, shell-free launches
                                            ├── vcs         Git, Git LFS, Perforce
                                            ├── studio      tasks, board, assets, trackers, GitHub
                                            └── health      project health checks
```

The code lives in a Cargo workspace (`crates/`) plus two apps (`apps/desktop`, `apps/cli`). The internal crate and package names still use the project's original name, **Vantadeck**. See [ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Privacy and security

- No account, telemetry, analytics or ads. The network is only used for the features listed in [Works offline](#works-offline). The launch-time update check can be turned off in Settings.
- Tokens are stored in the OS keychain, never in SQLite or project files. Git and Perforce use your existing CLI credentials.
- Apps are launched with structured arguments, never through a shell. Task commands from a repository need your review before they run.
- Downloaded content is never executed automatically.

Report vulnerabilities privately as described in [SECURITY.md](SECURITY.md).

## Status

**0.5** is the current release. It is stable for daily use on Windows, and macOS and Linux builds ship with every release. Still open before 1.0: Microsoft/Apple installer signing, clean-machine validation on each OS, and live Perforce testing. See [BLOCKERS.md](docs/release/BLOCKERS.md) and the [roadmap](ROADMAP.md).

## Contributing

Contributions from developers, artists, technical artists, pipeline engineers, writers and testers are welcome.

1. Read [CONTRIBUTING.md](CONTRIBUTING.md) and the [Code of Conduct](CODE_OF_CONDUCT.md).
2. For behavior or public-contract changes, start with an issue or an [RFC](docs/RFC_PROCESS.md).
3. Add tests, note any network or privacy impact, and run the checks above.
4. Sign off commits (`git commit -s`) per the [DCO](DCO.txt).

More: [Governance](GOVERNANCE.md) · [Releasing](RELEASING.md) · [Privacy](PRIVACY.md) · [Support](SUPPORT.md) · [Funding](FUNDING_DISCLOSURE.md) · [Trademark](TRADEMARK.md)

## License

[MIT](LICENSE) for code; documentation is CC BY 4.0 unless a file says otherwise.
