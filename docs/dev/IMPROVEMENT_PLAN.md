# Pipeline OS — Product & Improvement Plan

> Living document. It records why Pipeline OS should exist, what shipped in the
> dev channel, and what comes next in priority order. Work lands on the dev
> channel first (see [Release channels](#release-channels)), is tested there, and
> is promoted to a production release only once it is stable.

## 1. Why would anyone use this instead of opening their apps directly?

Opening Blender or Unity is easy. Everything *around* opening them is what
eats a creative team's day:

| Pain today | What people do now | What Pipeline OS does |
| --- | --- | --- |
| Five engine versions installed; the project needs exactly one | Unity Hub for Unity only, Epic Launcher for Unreal only, memory for everything else | One list of every engine/DCC version on the machine; each project remembers its version and launches with it |
| "How do I make a build again?" | Wiki pages, a senior's head, a CI config nobody can run locally | **Pipeline tasks**: one-click builds/cooks/renders/exports with live progress, shared in the repo so everyone (and CI, and AI agents) runs the same thing |
| Art, code and tasks live in five tools | Jira/ClickUp + Drive + Discord + Git client + file explorer | Per project: source control, a **git-native board**, an **asset library with review status**, notes, references, health |
| New teammate onboarding takes days | Screen-share and hope | Clone, add project, and the board, tasks, folder layout and **project brief** are already there |
| AI coding agents don't understand game/CG projects (meta files, binary assets, generated folders) | Paste context by hand; agents edit `Library/` | **Project brief** + **MCP server**: agents learn the engine, the folders never to touch, how to build, and what is in flight |
| Paid trackers are per-seat and cloud-only | Pay, or use a spreadsheet | Free, offline, version-controlled issues; connect Jira/Linear/GitHub/Plane when a studio already uses them |

**Positioning:** *the local-first home for creative projects*, combining a
launcher (Unity Hub / Epic / JetBrains Toolbox), a light production layer
(Anchorpoint / Kitsu territory) and an AI bridge. No account, no telemetry,
works offline, and every shared artifact is a plain file in the repo.

The wedge is **launch + build + board for a single project in two minutes,
free**. Studios adopt because it costs nothing to try and lives in their repo;
they stay because the team's tasks, board and briefs accumulate there.

## 2. Product principles

1. **Local-first & private.** No account. Network only on explicit action.
2. **Files over databases for anything shared.** Tasks, issues, asset review,
   layouts and briefs are TOML/Markdown under `.pipelineos/`: diffable,
   mergeable, reviewable, and readable without Pipeline OS.
3. **Safe by default.** Tasks are argument vectors, never shell strings; shared
   commands run only after the user trusts that exact content; destructive
   actions name themselves and default focus to Cancel.
4. **One brain, three faces.** Desktop, CLI and MCP share the same Rust
   application service, so anything a human can do, a script or agent can too.
5. **Calm UI.** Dense where pros need it, quiet by default, keyboard-first,
   fast. No modal nags; toasts with undo instead of confirmations wherever an
   action is reversible.

## 3. Shipped on the dev channel (this iteration)

UX audit findings (screenshots, before and after) and their fixes:

| Finding | Fix |
| --- | --- |
| Right column clipped at the 1100 px minimum window width | Responsive grid with `minmax(0,…)`; two columns only at `xl` |
| Fresh install = four "No X yet" boxes, no next step | Getting-started checklist + empty states that each carry the action that fixes them |
| Two competing searches; header dropdown not keyboard-navigable | Header opens the command palette: projects, launch-in-engine, Claude Code, apps, actions, themes |
| Launching (the core job) buried in an overflow menu | **Launch** is the primary row action; clicking a row opens the workspace |
| 19 native `window.confirm` dialogs | Accessible confirm dialog: named actions, destructive styling, safe default focus |
| Double padding on every card; noisy chrome ("All systems operational", duplicated offline badges) | Tightened density app-wide; one "Local & private" indicator; version + channel in the sidebar |
| Onboarding asked for role/interests it never used | 3 steps: what it does → theme → scan |
| Long developer-speak copy | Rewritten UX copy throughout |

New capabilities:

- **Pipeline tab:** task templates (Unity batch build/tests, Unreal
  BuildCookRun, Godot export, Blender render, custom), argv editor, trust
  review, live progress parsing (`42%`, `[3/40]`), streaming log viewer,
  cancellation of the whole process tree, run history that survives restarts.
- **Activity screen + Home "Pipeline" card + sidebar indicator:** every running
  task across projects, from anywhere in the app.
- **Board tab:** git-native Kanban (`.pipelineos/issues/<id>.toml`, one file per
  issue, so no merge conflicts), drag & drop + keyboard "Move to", quick add,
  filters, issue editor; **Open work** card on Home across all projects.
- **Assets tab:** gitignore-aware library of concept art, references, source
  art, models, audio, video and docs; lazy thumbnails; review status (WIP / In
  review / Approved / Changes needed) + feedback notes shared in
  `.pipelineos/assets.toml`; "create issue about this asset"; studio folder
  layouts (Game team, Studio pipeline `00_Admin…05_Delivery`, Film/VFX).
- **AI tab:** start Claude Code / Codex / Gemini / Aider or a terminal in the
  project (with the CLI on PATH); live **project brief**; copy-ready MCP setup.
- **MCP server** (`vantadeck mcp`): 13 tools: projects, brief, config,
  health, VCS status, apps, tasks + `run_task`, issues CRUD, assets,
  open-in-engine. See [AI_INTEGRATION.md](../AI_INTEGRATION.md).
- **CLI:** `context`, `tasks`, `issues`, `assets`; `apps list` shows real
  installs; **bug fixed**: the CLI used a different database than the desktop
  app, so it never saw the user's projects.
- **Installers bundle the CLI** (Tauri sidecar), so automation works out of
  the box.
- **Seven themes:** Midnight, Daylight, Graphite (DCC-neutral), Nordic, Ember,
  Paper, High contrast. Visible focus rings, reduced-motion support.
- **Dev channel** (below).

## 4. Roadmap (priority order)

Priorities are scored by *reach × pain ÷ effort*, biased toward things that make
the free product indispensable for one person first, then for a team.

### P0: next dev iterations

1. **Tracker connectors.** Pull GitHub Issues, Gitea/Forgejo, Jira (Cloud &
   Data Center), Linear, Plane and ClickUp into the board with an `external`
   link; status write-back where the API allows. Tokens in the OS keychain,
   never in project files. Includes **one-click self-hosting** of Forgejo
   (git + LFS + issues in one container) for studios that want their own server.
2. **Embedded per-project terminal** (xterm.js + portable-pty) docked under
   the workspace, with sessions that survive tab switches; agent sessions
   (Claude Code) as first-class terminals with the brief preloaded.
3. **Filesystem watching** (`notify`) for project status, assets and issues
   so screens update without refresh; incremental app detection.
4. **Native notifications** when long tasks finish while the app is in the
   background; taskbar progress on Windows.
5. **Code-signed Windows installer** (Azure Artifact Signing, ~$10/month)
   to remove the SmartScreen warning: the single biggest adoption blocker.
6. **winget + Scoop manifests** generated by `release.yml`.

### P1: team features (still free, still local-first)

- **Perforce in the UI** (provider exists): AAA studios live on P4.
  Changelists, locks, shelving in the Source Control tab.
- **File locking UX** for Git LFS (`git lfs lock`) on binary assets, visible
  in the asset library ("locked by ana").
- **Asset review mode:** side-by-side version compare (from Git history),
  annotation overlays, approve/reject from the board.
- **Engine plugins** (Unity/Unreal/Blender add-ons) that show the board and
  run tasks from inside the editor; Blender/Maya "save as new version" helpers.
- **Task chains & schedules:** "nightly: build → test → upload", task
  dependencies, parallel runners, artifacts list per run.
- **Project templates:** new project from studio template (folder layout,
  tasks, `.gitattributes` for LFS, ignore files, board columns).
- **Localization** (EN first; JA/ZH/KO/DE/FR/ES/PT-BR are large gamedev
  markets).

### P2: studio scale

- Shared **studio profile** repo (approved engine versions, task templates,
  layouts, policies) pulled by every seat.
- Render-farm / build-agent hooks (Deadline, Flamenco, Unreal Horde, TeamCity,
  Jenkins, GitHub Actions) surfaced as tasks.
- License visibility for DCC seats; SSO and audit log (enterprise edition).
- macOS/Linux parity with signing/notarization.

## 5. Architecture decisions

- **Keep Tauri.** See [ADR 0001](../adr/0001-keep-tauri.md). The feel of the app
  is a frontend craft problem (motion, density, latency budgets), not a shell
  problem.
- **Split the two 1,000-line components** (`App.tsx`, `project-detail.tsx`) into
  per-screen modules with a router; this iteration already moved new surfaces
  into components and shared constants into `lib/`.
- **Generate TS types from Rust** (`ts-rs`) so IPC contracts can't drift.
- **Performance budgets:** cold start < 1.5 s to interactive; screen switch
  < 100 ms; no frame > 50 ms while a task streams 5k lines/s (output is
  already batched every 120 ms in the backend).

## 6. Release channels

| Channel | App identity | Built by | Published to | Updates from |
| --- | --- | --- | --- | --- |
| **Dev** | "PipelineOS Dev" (`com.vantadeck.launcher.dev`), own data dir | `dev-build.yml` on every push to `dev`, `claude/**`, `codex/**` | Rolling **`dev-latest` pre-release** (never "latest") | `releases/download/dev-latest/latest.json` |
| **Stable** | "PipelineOS" (`com.vantadeck.launcher`) | `release.yml` on an annotated `vX.Y.Z` tag | GitHub Release (latest), SBOM, Sigstore, provenance | `releases/latest/download/latest.json` |

Dev builds are versioned `X.Y.(Z+1)-<run>`, so they sort above the current
stable release and below the next one. They install side by side with stable
and never touch its data. Promotion: dev → PR to `main` → `release/X.Y` →
tag. The CLI follows the same split: `vantadeck --channel dev …`.

## 7. Competitive landscape (Oct 2026)

| Product | What it is | Price | Gap Pipeline OS fills |
| --- | --- | --- | --- |
| Unity Hub / Epic Launcher | Single-engine version launcher | Free | Every engine and DCC, per-project pinning, builds, board |
| Anchorpoint | Git for artists, file locking, Unity/Unreal plugins | Free tier; paid ~€20–25/user/mo | Free, open, local issue board, AI bridge, all engines |
| Kitsu | Open-source production tracker (AGPL) | Self-host free; hosted from ~€20/user/mo | Desktop-native, works offline, launches tools, builds |
| ShotGrid / Flow, ftrack | Studio production tracking | ~$25–33/user/mo | Zero-setup for indies; connects to them later |
| Prism / AYON | DCC pipeline frameworks | Open source | Simpler: no server, no Python plugin stack required |
| Jira / Linear / ClickUp | General trackers | Per seat | Lives next to the files; connectors instead of replacement |

## 8. If open source alone doesn't sustain it: productization path

**Open-core, never paywalling what a solo creator needs.**

- **Free (MIT) forever:** launcher, versions, source control, pipeline tasks,
  board, assets, AI/MCP, themes, CLI.
- **Team (~$8–12/user/month, or a flat studio license):** studio profile sync,
  task schedules & remote runners, review mode with annotations, tracker
  two-way sync at scale, Perforce power features, priority support.
- **Enterprise:** SSO/SCIM, audit log, policy enforcement, offline license
  server, signed internal builds, SLA.
- **Other revenue:** sponsorships (GitHub Sponsors / Open Collective), paid
  onboarding for studios, a hosted Forgejo/Plane bundle for teams that don't
  want to run servers.

Trademark and governance docs already exist (TRADEMARK.md, GOVERNANCE.md), so
this path keeps the community edition credible.

## 9. Go-to-market

1. Signed installer + winget + a 60-second demo video (launch → build → board → Claude Code).
2. Docs site with "recipes": Unity CI-free builds, Unreal BuildCookRun,
   Blender render farm-lite, using Claude Code on a Unity project safely.
3. Launch in r/gamedev, r/Unity3D, r/unrealengine, r/blender, r/vfx,
   Polycount, 80.lv, Hacker News (Show HN), the Godot community, and game jams
   (free tool sponsorship).
4. Template gallery: community-contributed task templates and folder layouts.

## 10. Success metrics

- **Activation:** first project + first launch within 2 minutes of install (target 70%).
- **Habit:** weekly active installs; launches per user per week; % of users with ≥1 pipeline task.
- **Team pull:** projects with `.pipelineos/issues/` committed by >1 author.
- **Quality:** crash-free sessions (opt-in reporting only), p95 cold start.
- **Community:** stars, contributors, template submissions.

Because there is no telemetry, these are measured through opt-in surveys,
release download counts, GitHub insights, and an opt-in anonymous ping
(off by default) proposed in a future RFC.

## 11. Risks

| Risk | Mitigation |
| --- | --- |
| Shared tasks become a code-execution vector | Trust gate keyed to content fingerprint; argv only; re-trust on change |
| Feature sprawl dilutes the calm UI | Every feature lives inside the project workspace; Home stays an overview |
| Tracker APIs churn | Connectors isolated per provider; import is idempotent by external key |
| Windows-only reputation | Linux/macOS builds already in CI; validate and sign next |
| Solo-maintainer bus factor | Contributor ladder, good-first-issues from this roadmap, RFC process |
