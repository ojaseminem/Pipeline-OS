# Native Validation Record — Windows RC (2026-06-21)

Completed instance of [`NATIVE_VALIDATION.md`](NATIVE_VALIDATION.md). This records a
**local, unsigned Windows release-candidate build**, not a CI-tagged, signed, promoted
release. Empty fields are open gates, not implied passes. See
[`BLOCKERS.md`](BLOCKERS.md) for everything still required before a published 1.0.

## Release identity

- Tag: none (release-candidate; not tagged)
- Commit: `fa10926` on branch `claude/elegant-bell-ad0f96`, with uncommitted RC changes
  (desktop UI button wiring, `tauri.conf.json` bundle icon list, README/BLOCKERS docs)
- Workflow run URL: N/A — built locally, not via `release.yml`
- Draft release URL: N/A
- Evidence reviewer and timestamp: local build/validation, 2026-06-21

## Artifact evidence

- `SHA256SUMS.txt` verified: N/A (no release workflow run); per-artifact hashes below
- Sigstore bundles verified against expected workflow identity: open — requires `release.yml`
- GitHub build provenance verified: open — requires `release.yml`
- GitHub SBOM attestation verified: open — requires `release.yml`
- Source SBOM reviewed for Rust and Node coverage: open — requires `release.yml`
- Artifact SBOM reviewed: open — requires `release.yml`

## Platform matrix

| Platform / version / architecture | Artifact and SHA-256 | Platform signature result | Install | Launch | Basic project workflow | Uninstall / cleanup | Tester, time, evidence |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Windows 11 Pro 10.0.26200 / x64 | `Vantadeck_0.1.0_x64_en-US.msi` (6,746,112 B) sha256 `541324867c854a252501e844920ccb366c6b5ad676711191b70b754dfbfedec2`; `Vantadeck_0.1.0_x64-setup.exe` (5,249,043 B) sha256 `37c8eb1a91df8ddfecbb852b002ed65881f6c3b3d4ee9399ce3b8833df34eac5` | Unsigned — Authenticode gate OPEN | Not run on clean VM (gate OPEN) | PASS — `vantadeck-desktop.exe` launches, window responsive | PASS — CLI end-to-end: `apps list`, `project import/show/health`, `project vcs status` return valid v1 JSON envelopes; Git status + health (`REPO_UNCOMMITTED_CHANGES`) correct | Not run on clean VM (gate OPEN) | local dev machine, 2026-06-21 |
| macOS | | | | | | | |
| Linux AppImage | | N/A or distribution signature evidence | | | | | |
| Linux DEB | | N/A or repository signature evidence | | | | | |
| Linux RPM | | N/A or repository signature evidence | | | | | |

## Quality gate (this commit, Windows host)

- `cargo fmt --check`: PASS
- `cargo clippy --workspace --all-targets -- -D warnings`: PASS
- `cargo test --workspace`: PASS
- `cargo build --release --workspace`: PASS
- `npm run typecheck`: PASS
- `npm run lint` (`--max-warnings 0`): PASS
- `npm test` (vitest): PASS — 20 tests
- `npm run validate:contracts`: PASS — 9 application manifests
- `npm run build`: PASS

## Decision

- Known limitations: unsigned installers; no clean-VM install/uninstall validation; no CI
  release-workflow evidence (SBOM/checksums/Sigstore/provenance); macOS, Linux, and live
  Perforce gates not exercised. See [`BLOCKERS.md`](BLOCKERS.md).
- Removed or withheld artifacts: macOS and Linux artifacts not produced on this host.
- Rollback/data-compatibility assessment: schemas unchanged (v1); see
  [`RECOVERY.md`](RECOVERY.md).
- First maintainer approval: pending
- Second maintainer approval: pending
- Promotion time or rejection reason: NOT promotable — external 1.0 gates open by design.
