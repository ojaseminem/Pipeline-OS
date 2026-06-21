# Vantadeck — Path to an Industry-Standard Launcher

> Status: proposal / north-star plan. Supersedes nothing; complements
> [ROADMAP.md](../../../ROADMAP.md) and the
> [1.0 plan](2026-06-21-vantadeck-1-0.md). Build top-to-bottom; phases are
> ordered by dependency, not calendar.

## Where we are today

- **Backend:** 10 Rust crates (domain, detection, launcher, projects, vcs, health,
  manifests, security, storage, application) + CLI. Feature-complete for a v1 slice,
  ~46 test groups green. Detection now spans all drives + manifest `knownPaths`,
  hides console windows, and launch is architecture-safe with real icon extraction.
- **Desktop:** a single 275-line `App.tsx` + one 185-line `styles.css`, talking to
  ~20 Tauri commands. Functional but monolithic; no design system, routing, component
  library, streaming progress, toasts, or e2e tests.
- **Infra:** CI build + signed release pipeline, Tauri auto-updater wired, schema
  validation, SBOM/Sigstore.

**The gap to "industry standard"** (Unity Hub / Epic / JetBrains Toolbox / GOG Galaxy
class) is mostly: (1) a real UI architecture + design system, (2) streaming/async UX for
long operations, (3) feature depth (version management, rich library, profiles), and
(4) reliability/observability at scale.

## Design principles (the bar)

1. **Local-first & private** — no accounts, no telemetry, no surprise network. Every
   networked action is explicit and auditable. (Already a project value; keep it sacred.)
2. **Fast & responsive** — nothing blocks the UI; every long op streams progress and is
   cancellable. Cold start < 1s to interactive; cached dashboard instant.
3. **Keyboard-first** — a real command palette; every action reachable without a mouse.
4. **Trustworthy** — predictable, recoverable, never destructive without confirmation;
   clear errors with remediation.
5. **Beautiful & legible** — a coherent design system, dense where power users want it,
   calm by default. Accessible (WCAG 2.1 AA).

---

## Phase 1 — Frontend architecture for scale (refactor foundation)

**Why first:** the monolithic `App.tsx` can't carry the redesign or new features.

**Workstreams**
- **Routing & structure.** Introduce a router (TanStack Router or React Router) and split
  into `routes/` (Home, Library, Applications, Health, Tools, Activity, Settings) and
  `components/`, `features/`, `lib/`. Keep `bridge.ts` as the single typed IPC boundary.
- **State management.** Adopt TanStack Query for all IPC reads (caching, background
  refetch, loading/error states for free) + a small Zustand store for UI state. Remove
  ad-hoc `useState` data loading.
- **Typed IPC contract.** Generate TS types from Rust (ts-rs or schemars→JSON Schema→TS)
  so `DesktopApp`, `AppInstallation`, etc. can never drift. Wire into
  `npm run validate:contracts`.
- **Error boundaries + toasts.** Replace the single `status` line with a toast/notification
  system and route-level error boundaries.

**Acceptance:** no component > ~200 lines; all data goes through TanStack Query; types are
generated, not hand-written; typecheck/lint/tests stay green; existing behavior preserved.

---

## Phase 2 — Streaming, cancellable operations (backend + UX)

**Why:** scanning, VCS, and downloads must feel instant and controllable. The roadmap
already promises "background operations, activity logs, cancellation."

**Workstreams**
- **Progress events.** Emit Tauri events (`scan://progress`, per-source status, counts)
  from `application::scan_apps`; the UI shows a live progress bar + per-source breakdown
  instead of a spinner. Thread a `CancellationToken` so "Cancel" actually stops a scan.
- **Operation/Task model.** A first-class `Task` (id, kind, status, progress, started/ended,
  log) persisted in SQLite, surfaced in an **Activity** screen. All long ops (scan, sync,
  commit, push, update download) become tasks. Single-flight locking per resource.
- **Filesystem watching.** Watch known roots + project dirs (`notify` crate) to
  auto-refresh detection and project status without manual rescans.
- **Incremental detection.** Cache detection results with mtimes; re-scan only changed
  roots. Make full-drive scans bounded and parallel.

**Acceptance:** a scan shows real-time progress, can be cancelled mid-flight, never blocks
the UI, and re-running is near-instant when nothing changed.

---

## Phase 3 — UI/UX redesign & design system (the "next level" look)

**Why:** this is the visible leap. Reference: `docs/design/vantadeck-dashboard-reference.png`,
elevated.

**Design system**
- **Tokens.** Formalize color/spacing/radius/typography/elevation as CSS variables (light/
  dark/system + high-contrast). One source of truth in `styles/tokens.css`.
- **Components.** Use **Tailwind CSS + shadcn/ui** (Radix under the hood, MIT) generated
  into the repo via the shadcn CLI and themed on-brand: Button, Card, Dialog, Tooltip,
  Tabs, Select, Toast/Sonner, Command palette, Skeleton, Progress, Badge, Switch. Owned
  locally and editable.
- **Motion.** Subtle, purposeful transitions (Framer Motion or CSS) — list reordering,
  panel open, scan progress.

**Key screens (reimagined)**
- **Home / Continue.** Hero "continue project" with thumbnail, engine/branch/health at a
  glance; smart suggestions ("3 projects have uncommitted changes").
- **Library (Projects).** Card + table views, thumbnails, filters (engine, VCS, tag,
  drive), saved views, bulk actions, drag-to-pin, rich per-project detail drawer
  (profiles, linked apps, VCS, health, recent activity).
- **Applications.** JetBrains-Toolbox-style: each app shows all detected versions with
  real icons, disk usage, "set default," per-version launch, compatibility badges, and
  manual-override flow inline.
- **Command palette (Ctrl/Cmd+K).** Fuzzy across projects, apps, actions, settings, docs.
- **Onboarding.** First-run wizard: pick scan locations, run first scan with live progress,
  import first project, choose theme.
- **Empty/loading/error states** for every surface (skeletons, helpful CTAs).

**Acceptance:** a designer-quality, accessible UI; keyboard-complete; passes the existing
`design:accessibility-review` bar (contrast, focus, targets) at desktop + reduced widths.

---

## Phase 4 — Core launcher feature depth

**Why:** match what power users expect from a creative-tools launcher.

- **Engine/app version management.** Detect + group versions (done); add "set preferred,"
  disk usage, reveal-in-explorer, uninstall (where the vendor supports it), and *guided*
  install by deep-linking Unity Hub / Epic rather than reimplementing downloads.
- **Launch profiles & per-project pinning.** Rich editor for `.vantadeck/project.toml`
  profiles (args, working dir, preferred/fallback versions) with validation and a dry-run
  "what will launch" preview (reusing `resolve_launch_profile`).
- **Project templates & creation.** "New project" from templates per engine; register and
  scaffold `.vantadeck/`.
- **Deeper VCS.** Surface the existing Git/LFS + Perforce providers fully: branch list,
  ahead/behind, stash, LFS status, conflict warnings; Perforce workspace view. Always
  confirmation-gated for mutations (already enforced in the service).
- **Health center.** Expand `vantadeck-health` with real `HealthCheck` impls (missing
  engine, broken profile, LFS not installed, large untracked files, disk space) and
  one-click remediations.
- **Tags, search, saved filters, recents** across projects and apps.

**Acceptance:** a user can discover, configure, diagnose, and launch any project/engine
without leaving Vantadeck or touching a terminal.

---

## Phase 5 — Reliability, performance & observability

- **Performance budgets.** Cold start, scan throughput, memory; track in CI. Virtualize
  long lists; lazy-load icons (cache extracted icons in SQLite, not per-render).
- **Crash & recovery.** Panic hooks → friendly recovery screen; DB migration safety nets;
  the existing compare-before-save recovery extended to all writes.
- **Local, private diagnostics.** Opt-in, on-device log viewer + "export diagnostics
  bundle" (no auto-upload) to make bug reports actionable without telemetry.
- **Testing pyramid.** Unit (Rust + Vitest), integration (service), **e2e** via
  `tauri-driver`/WebDriver for real click-through, and visual-regression snapshots of key
  screens. Add fixtures for each detection source on CI.
- **Fuzz/property tests** for manifest + project parsers; golden tests for CLI JSON
  envelopes (contract stability).

**Acceptance:** documented performance budgets enforced in CI; e2e covers the top 10 user
journeys; a crash leaves data intact and shows a recovery path.

---

## Phase 6 — Platform parity, packaging & update maturity

- **macOS & Linux** native validation (the standing 1.0 gates): real clean-machine
  install/launch/uninstall evidence per `docs/release/NATIVE_VALIDATION.md`.
- **Code signing** end-to-end: Authenticode (Win), Developer ID + notarization (macOS),
  repo/package signing (Linux). Wire secrets into `release.yml` (updater signing already
  scaffolded; see `docs/release/BLOCKERS.md`).
- **Auto-update UX.** Background check + changelog dialog + staged rollout + rollback;
  delta updates where supported. Verify the `latest.json` feed end-to-end on a tagged run.
- **Distribution.** Winget/Chocolatey, Homebrew cask, Flatpak/AUR — community-maintained.

**Acceptance:** signed, notarized, auto-updating builds on all three OSes with recorded
native-validation evidence; published 1.0 unblocked.

---

## Phase 7 — Ecosystem & community

- **Tools Hub** moved to a separately-governed, hosted index repo (open gate in
  `docs/release/BLOCKERS.md`); in-app browse/verify/cache stays offline-first and never
  auto-executes.
- **Plugin/extension surface** (sandboxed, manifest-declared) for new detection sources,
  health checks, and project types.
- **Integrations.** Optional, explicit connectors (issue trackers, asset stores) behind
  the same "network is opt-in" rule.
- **Governance.** RFCs for public contracts (process exists); contributor ladder; public
  roadmap.

---

## Definition of "industry standard" (success metrics)

- Cold start < 1s to interactive; first full scan streamed with progress and cancellable;
  incremental rescan < 1s when unchanged.
- Zero blocking modals from the OS; zero stray console windows (fixed); no data loss on
  crash.
- Keyboard-complete; WCAG 2.1 AA; light/dark/high-contrast.
- Signed, notarized, auto-updating on Win/macOS/Linux with native-validation evidence.
- e2e coverage of top journeys; performance budgets enforced in CI.
- A new user can go install → scan → import → launch in under 2 minutes, guided.

## Suggested sequencing

1 → 2 (architecture + streaming ops, enables everything) →
3 (redesign on the new foundation) →
4 (feature depth) in parallel with 5 (reliability) →
6 (platform/signing, unblocks 1.0) →
7 (ecosystem). Ship in vertical slices; keep the gate green at every step.

## Immediate next steps (proposed)

- [ ] Phase 1 spike: introduce router + TanStack Query + generated types; migrate the
      Applications screen first as the reference implementation.
- [ ] Phase 2 spike: add `scan://progress` Tauri events + cancellation; wire the live
      progress bar (replaces the current spinner).
- [ ] Phase 3 spike: extract design tokens from current `styles.css`; stand up the
      component primitives + command palette.
- [ ] Open an RFC for the UI architecture decision (router/state/component lib) per
      `docs/RFC_PROCESS.md`.
