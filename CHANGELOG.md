# Changelog

All notable changes to Pipeline OS. Versions follow [Semantic Versioning](https://semver.org/);
each one is a [GitHub release](https://github.com/ojaseminem/Pipeline-OS/releases) with
installers for Windows, macOS (Apple Silicon and Intel) and Linux. Installed apps update
themselves from the latest release.

## 0.5.4 — 2026-10-24

### Added
- AI agents can author pipelines over MCP: `list_task_presets`, `save_task`,
  `delete_task`, `save_task_preset` and `delete_task_preset`. Any change to a task
  revokes trust, so an agent can propose a pipeline but it only runs after you review
  and trust it.
- Project presets in `.pipelineos/presets.toml`, shared through version control and
  listed first in Add tasks. Save any task as a preset from its menu.
- Copy one task or the whole pipeline as TOML, and import pasted TOML or a `.toml`
  file. The exact commands are shown before anything is saved.
- The project brief is now a card at the top of Overview that opens a large reader
  with a Markdown source view, copy, and save to `context.md`.

### Fixed
- The task editor edits environment variables and no longer drops them on save.

## 0.5.3 — 2026-10-24

### Fixed
- macOS updates: the update feed now has separate Apple Silicon and Intel entries.
  0.5.0 on Apple Silicon reported it was up to date because the feed only listed an
  (incorrectly labelled) Intel build.

### Added
- A macOS Intel installer alongside Apple Silicon.

## 0.5.2 — 2026-10-24

0.5.1 was cancelled before it published; its changes ship here.

### Added
- Branch lists (switcher, merge picker, Branches & tags) show when each branch was
  last updated, with the exact date on hover.
- The branch switcher sizes to the longest branch name and keeps New branch and Merge
  pinned below the scrolling list.

### Fixed
- The app works offline. Screens no longer hang loading on macOS when the network is
  down; Git, GitHub and tracker actions say the remote isn't reachable instead of
  showing raw errors.
- A manual update check no longer claims you're up to date when it couldn't reach
  the server.

## 0.5.0 — 2026-10-23

First public release as Pipeline OS (formerly Vantadeck).

- Projects: import or discover Unity, Unreal, Godot, Blender and other projects, with
  health checks, pinning, and launching into the right editor version.
- Applications: detects installed engines and DCCs on Windows, macOS and Linux, with
  per-machine version overrides.
- Source Control: Git status, diffs, commits, branches, merges, stashes, and GitHub pull
  requests and checks.
- Pipeline: build, cook, render, export and test tasks stored in
  `.pipelineos/tasks.toml`, with built-in templates and live output.
- Board: a git-native issue board with optional sync from GitHub Issues, Jira, Linear,
  Plane, Gitea/Forgejo and ClickUp.
- Assets: an asset library and studio folder layouts.
- AI: the `vantadeck mcp` server, one-click setup for Claude, Codex, Cursor, VS Code and
  others, and an AI-ready project brief.
- The `vantadeck` command line with a stable `--json` envelope.
- Signed in-app updates.
