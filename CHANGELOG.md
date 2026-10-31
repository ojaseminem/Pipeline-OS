# Changelog

What changed in each Pipeline OS release. The same notes appear on each
[GitHub release](https://github.com/ojaseminem/Pipeline-OS/releases). Installed apps
update themselves from the latest release.

## Unreleased

### What's new

- **Organize a messy project.** Pick a folder layout and see exactly which loose
  files would move where, untick anything to leave alone, then apply. Engine folders
  (Unity `Assets/`, Unreal `Content/`, Godot imports) are never touched, nothing is
  overwritten, and the last organize can be undone.
- **Your own folder layouts.** Customize or duplicate a layout for the project (shared
  through Git) or keep it on your computer. Layouts are plain JSON files; copy,
  import and export them, or import a simple `.txt` folder list.
- **New built-in layout** for client and freelance projects.
- **AI agents can propose layouts** and preview an organize; moving files stays with
  you.
- `vantadeck organize` commands for layouts, previews, applying and undo.

### Smaller changes

- Task import also accepts JSON.

## 0.5.4 — 2026-10-24

### What's new

- **AI agents can build your pipeline.** Claude, Cursor and other AI apps connected to
  Pipeline OS can now create, edit and remove build tasks and presets for you. Nothing
  they write runs until you've looked at the commands and trusted them.
- **Your own presets, per project.** Save any task as a preset and it shows up first in
  Add tasks. Presets live in the project (`.pipelineos/presets.toml`), so everyone on the
  team gets them through Git.
- **Copy and import tasks.** Copy one task or the whole pipeline as text, and paste it
  into another project, or open a `.toml` file. You see exactly what each task will run
  before anything is saved.
- **Project brief on Overview.** The brief now sits at the top of the Overview tab.
  Click it to read the whole thing in a large window, view the Markdown, copy it, or
  save it for AI agents.

### Fixes

- Editing a task no longer loses its environment variables.

## 0.5.3 — 2026-10-24

### What's new

- **Intel Mac support.** There's now a separate installer for Intel Macs alongside
  Apple Silicon.

### Fixes

- Macs now get updates. Apple Silicon Macs on 0.5.0 said they were up to date even when
  a newer version was out.

## 0.5.2 — 2026-10-24

### What's new

- **See when each branch was last updated.** The branch switcher, the merge picker and
  Branches & tags show how long ago each branch changed. Hover for the exact date.
- **Easier branch switcher.** It widens to fit long branch names, and New branch and
  Merge stay pinned at the bottom while the list scrolls.

### Fixes

- **Works offline.** Screens no longer get stuck loading on a Mac with no network. Git,
  GitHub and tracker actions now say they can't reach the server instead of showing
  raw errors.
- Checking for updates while offline no longer says you're up to date.

0.5.1 was cancelled before release; its changes are included here.

## 0.5.0 — 2026-10-23

The first public release as Pipeline OS (formerly Vantadeck).

- **Projects:** add or discover Unity, Unreal, Godot, Blender and other projects, see
  their health, pin them, and open each in the right app version.
- **Applications:** finds 60+ engines and creative apps on Windows, macOS and Linux,
  including several versions side by side.
- **Source Control:** changes, diffs, commits, branches, merges, and GitHub pull
  requests and checks.
- **Pipeline:** one-click builds, cooks, renders, exports and tests from templates, with
  live logs and run history.
- **Board:** an issue board stored in the project, with optional sync from GitHub
  Issues, Jira, Linear, Plane, Gitea/Forgejo and ClickUp.
- **Assets:** an asset library and ready-made studio folder layouts.
- **AI:** connect Claude, Codex, Cursor, VS Code and others in one click, plus a
  project brief that tells agents how your project works.
- **Command line:** everything above from scripts with the `vantadeck` CLI.
- **Automatic updates,** signed so only real releases install.
