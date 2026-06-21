# RFC: Desktop UI Architecture

Status: Draft

## Problem

The desktop app is a single ~275-line `App.tsx` with one `styles.css`, ad-hoc
`useState` data loading, a single status line, and hand-written IPC types that can
drift from the Rust backend. This cannot carry the redesign and feature depth in
[the north-star plan](../docs/superpowers/plans/2026-06-21-industry-standard-launcher.md).
We need an architecture that scales to many screens, async/streaming operations, and a
real design system without sacrificing the project's local-first, lean ethos.

## Users and outcomes

Power users (game/CG/VFX) get a fast, keyboard-first, never-blocking UI. Contributors
get a modular codebase with typed boundaries and testable units.

## Design and public contracts

Chosen stack (rationale: mature, MIT-licensed, no network/runtime services, strong TS):

- **Data layer:** `@tanstack/react-query` for all IPC reads — caching, background
  refetch, and loading/error states out of the box. One typed query/mutation module
  wraps `bridge.ts`.
- **UI state:** `zustand` for small, local UI state (toasts, command-palette open, view
  prefs). No global redux.
- **Routing:** `@tanstack/react-router` (type-safe) for screen routing, adopted after the
  data layer lands to avoid destabilizing the current navigation/tests in one step.
- **Styling & components:** **Tailwind CSS** + **shadcn/ui** (Radix primitives under the
  hood, MIT). Components are generated into the repo via the shadcn CLI and owned/edited
  locally — fast to assemble, accessible by default, and themed on-brand via Tailwind
  tokens. All build-time and local; no runtime CSS service.
- **Type generation:** export TS types from the Rust IPC structs (`ts-rs`) so
  `DesktopApp`, `AppInstallation`, etc. cannot drift; wire the check into
  `validate:contracts`.

## Privacy and network behavior

No change. All libraries are build-time/runtime-local; no telemetry, no network. The
local-first guarantee is unchanged.

## Security model

No new IPC surface from this RFC. Generated types tighten the existing contract. Radix
and the chosen libs run in the existing webview sandbox under the current capabilities.

## Compatibility and migration

Incremental: (1) add the Query data layer and migrate reads screen-by-screen
(Applications first as reference); (2) generate IPC types and replace hand-written ones;
(3) introduce the router; (4) extract design tokens + component primitives; (5) redesign
screens. Each step keeps the gate (typecheck, lint, tests, clippy, cargo test) green.

## Alternatives

- **In-house Radix + CSS tokens:** maximum control and minimum deps, but every component
  is hand-built — slower to reach a polished, complete set than generating from shadcn/ui.
- **Redux Toolkit:** heavier than needed; Query + Zustand cover server and UI state.
- **No framework (status quo):** does not scale to the planned surface area.

## Test and rollout plan

Vitest unit/component tests per migrated screen; `tauri-driver` e2e for top journeys
(Phase 5). Land behind no flags — the migration is internal. Roll out screen-by-screen;
revert any step that regresses the gate.

## Decision

Adopt TanStack Query + Zustand + TanStack Router + Tailwind CSS + shadcn/ui + ts-rs,
migrated incrementally. Revisit the router choice if e2e or bundle-size budgets are not met.
