# Remaining 1.0 Blockers

The following external and infrastructure gates must be satisfied before a published cross-platform 1.0 release. These cannot be completed in a local development environment.

## Code-signing and distribution

- **Windows Authenticode certificate and timestamp** — Current builds are unsigned; platform signature validation requires a code-signing certificate and trusted timestamp.
- **macOS Developer ID signature, hardened runtime, and Apple notarization** — Macintosh builds require Developer ID signing, hardened runtime entitlements, and successful notarization with Apple.
- **Linux package and repository signing** — Distribution of DEB, RPM, and AppImage packages requires signing keys and repository setup.

## Native platform validation

- **Windows clean-machine validation** — Install unsigned RC on a clean Windows VM, verify launch and basic project workflow (import, view, launch app), confirm uninstall and cleanup.
- **macOS clean-machine validation** — Install signed and notarized build on a clean supported macOS system, verify Gatekeeper launch and basic workflow.
- **Linux clean-machine validation** — Install each package format (DEB, RPM, AppImage) on clean supported distributions, verify launch and basic workflow for each.

## Integration testing

- **Live Perforce server integration** — Current Perforce support includes mutation, timeout, and cancellation contracts tested against command construction and parsing. Integration testing against a live Perforce server is required.

## Auto-update signing

The in-app auto-updater is implemented (it checks the GitHub releases feed and
verifies downloads against the public key in `tauri.conf.json`). To publish
working updates, the release pipeline must sign the updater artifacts:

- Generate the keypair locally with `npm run tauri --workspace @vantadeck/desktop -- signer generate -w apps/desktop/.tauri/updater.key` (the public key is already embedded in `tauri.conf.json`; the private key is gitignored and must never be committed).
- Add two repository secrets used by `release.yml`:
  - `TAURI_SIGNING_PRIVATE_KEY` — contents of `apps/desktop/.tauri/updater.key`.
  - `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` — the key password (empty if none).
- A tagged `release.yml` run then emits signed updater artifacts and `latest.json`, which the app's updater consumes from the latest release.

## Community infrastructure

- **Separately-governed Tools Hub index repository** — The Tools Hub catalog must be separated into a publicly-governed index repository independent of the main project.

## Current state

Vantadeck is an unsigned Windows Release Candidate. All in-code features are complete and tested. See [RELEASE.md](RELEASE.md) for the promotion process and evidence requirements that these gates feed into. See [NATIVE_VALIDATION.md](NATIVE_VALIDATION.md) for the validation record template.
