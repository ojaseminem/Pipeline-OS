# Tools Hub Specification v1

> Status: planned. The CLI can already cache, list and verify a reviewed index
> (`vantadeck tools …`); the separately governed index repository and the in-app
> browser don't exist yet. See [ROADMAP.md](../ROADMAP.md).

The Tools Hub index will live in its own repository and contain metadata, not
executable code. Entries identify source, license, platforms, supported hosts,
provenance, review state, artifact SHA-256 digests, safety notes, and last verification
date. Every entry is validated before it replaces the existing SQLite cache.

V1 may browse metadata, open the upstream source, and download a checksum-verified
release artifact after a user action. It does not execute installers or scripts.
Submitted, reviewed, verified, stale, and withdrawn states must be visible.

The application never fetches the index implicitly. Offline list/search uses only the
validated cache. Artifact verification does not install or launch the artifact.
