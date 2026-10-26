# Remaining 1.0 Blockers

Pipeline OS 0.5 is published for Windows, macOS (Apple Silicon and Intel) and Linux.
The gates below must be satisfied before 1.0. They need certificates, accounts or clean
machines, so they can't be completed in a development environment.

## Code-signing and distribution

- **Windows Authenticode certificate and timestamp** — Current builds are unsigned; platform signature validation requires a code-signing certificate and trusted timestamp.
- **macOS Developer ID signature, hardened runtime, and Apple notarization** — Macintosh builds require Developer ID signing, hardened runtime entitlements, and successful notarization with Apple.
- **Linux package and repository signing** — Distribution of DEB, RPM, and AppImage packages requires signing keys and repository setup.

## Native platform validation

- **Windows clean-machine validation** — Install the current release on a clean Windows VM, verify launch and basic project workflow (import, view, launch app), confirm uninstall and cleanup.
- **macOS clean-machine validation** — Install the signed and notarized build on clean Apple Silicon and Intel Macs, verify Gatekeeper launch and basic workflow.
- **Linux clean-machine validation** — Install each package format (DEB, RPM, AppImage) on clean supported distributions, verify launch and basic workflow for each.

## Integration testing

- **Perforce in the app** — A typed Perforce provider exists in the core library, with mutation, timeout, and cancellation contracts tested against command construction and parsing. It still needs to be wired into the app and CLI and tested against a live Perforce server.

## Auto-update signing — DONE

The in-app auto-updater is implemented and **live**. The signing keypair exists,
the repository secrets `TAURI_SIGNING_PRIVATE_KEY` / `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`
are configured, and pushing an annotated `vX.Y.Z` tag runs `release.yml`, which
builds and signs Windows, macOS (Apple Silicon and Intel) and Linux, generates `latest.json`, and publishes the
GitHub release. The in-app updater consumes it from the latest release.

> Note: this is distinct from **installer code-signing** (Windows Authenticode /
> macOS notarization), which is still pending — see "Code-signing and distribution"
> above. Updater artifacts are signed with the project key, but the installer is not
> yet code-signed, so Windows SmartScreen shows an "unknown publisher" prompt.

## Community infrastructure

- **Separately-governed Tools Hub index repository** — The Tools Hub catalog must be separated into a publicly-governed index repository independent of the main project.

## Current state

Pipeline OS 0.5.4 is published on GitHub Releases for Windows, macOS (Apple Silicon and
Intel) and Linux. CI builds every tag on all four targets and publishes signed,
auto-updating releases. The remaining 1.0 gates are external: platform code signing
(Windows Authenticode, Apple Developer ID and notarization, Linux package signing) and
clean-machine validation, plus bringing Perforce into the app. See
[RELEASE.md](RELEASE.md) for the release checklist and
[NATIVE_VALIDATION.md](NATIVE_VALIDATION.md) for the validation record template.
