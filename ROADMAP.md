# Roadmap

Roadmap labels describe intent, not shipped support.

- **Alpha 1:** Windows detection, grouped versions, manual overrides, safe launch,
  portable projects, Git/Git LFS health, CLI JSON, and the desktop dashboard.
- **Alpha 2:** deeper Windows sources, project import, launch profiles, background
  operations, activity logs, and Perforce workspace status.
- **Beta:** curated Tools Hub, packaging, update checks, accessibility and performance
  budgets, plus macOS and Linux parity.
- **1.0:** stable schemas and CLI contracts, signed cross-platform artifacts, SBOMs,
  migration guarantees, recovery documentation, and supported-version policy.

Changes to these outcomes require a public RFC and roadmap pull request.

## Current implementation status

The current local implementation includes recursive filesystem discovery;
Windows Registry, Unity Hub, Epic, Steam, shortcut, macOS bundle, Linux desktop,
Flatpak, Snap, AppImage, and common-path sources; local overrides; portable project
import and revision-checked recovery-backed writes; Git/LFS workflows; a typed Perforce provider;
project/app/health/tool management screens; offline validated Tools Hub caching;
durable activity; and matching JSON CLI contracts.

The following are release gates, not implied shipped support:

- macOS and Linux adapters require native compilation and hands-on validation.
- Perforce requires integration testing against a disposable live server.
- The community tool index requires its separately governed public repository.
- Platform signing, notarization, package signing, and clean-machine packaging tests
  require external identities and native environments.
- A public 1.0 tag remains blocked until `docs/release/` contains complete evidence.
