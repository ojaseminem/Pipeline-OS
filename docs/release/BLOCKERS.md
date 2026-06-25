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

## Auto-update signing — DONE

The in-app auto-updater is implemented and **live**. The signing keypair exists,
the repository secrets `TAURI_SIGNING_PRIVATE_KEY` / `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`
are configured, and pushing an annotated `vX.Y.Z` tag runs `release.yml`, which
builds and signs Windows/macOS/Linux, generates `latest.json`, and publishes the
GitHub release. The in-app updater consumes it from the latest release.

> Note: this is distinct from **installer code-signing** (Windows Authenticode /
> macOS notarization), which is still pending — see "Code-signing and distribution"
> above. Updater artifacts are signed with the project key, but the installer is not
> yet code-signed, so Windows SmartScreen shows an "unknown publisher" prompt.

## Community infrastructure

- **Separately-governed Tools Hub index repository** — The Tools Hub catalog must be separated into a publicly-governed index repository independent of the main project.

## Current state

Pipeline OS (repo `Vantadeck`) is a Windows-first app at public-V1 stage. All in-code features are complete and tested, and CI auto-publishes signed, auto-updating releases on each tag. The remaining V1 gates are external: a **code-signed Windows installer** (to clear SmartScreen) and **clean-machine validation**. macOS/Linux signing/notarization and live-Perforce validation are deferred post-V1. See [RELEASE.md](RELEASE.md) for the promotion process and [NATIVE_VALIDATION.md](NATIVE_VALIDATION.md) for the validation record template.
