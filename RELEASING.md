# Releasing Pipeline OS

## Branches and channels

| Branch / tag | Purpose |
| --- | --- |
| `dev` | Integration branch. Every push builds **PipelineOS Dev** and updates the rolling [`dev-latest`](https://github.com/ojaseminem/Pipeline-OS/releases/tag/dev-latest) pre-release for testers. |
| `main` | Stable trunk, promoted from `dev` once a change has been tested. Always releasable. |
| `release/X.Y` | Optional stabilization line for a minor version, cut from `main` when a release needs fixes without new features. |
| tags `vX.Y.Z` | Immutable release points. Pushing one builds, signs and publishes the release. |

Feature work → branch → merge into `dev` → test the dev build → merge `dev` into
`main` → tag.

## Versioning (SemVer)

Keep these in lockstep (`MAJOR.MINOR.PATCH`):

- `Cargo.toml` → `[workspace.package] version`
- `apps/desktop/package.json` → `version`
- `apps/desktop/src-tauri/tauri.conf.json` → `version` (what the app reports and the
  updater compares against)

Then refresh the lockfiles: `cargo update -w` and
`npm install --package-lock-only --ignore-scripts`.

## Cutting a release

```bash
# On an up-to-date main with green CI:
#   bump the three version fields and refresh lockfiles, then
git commit -am "release: vX.Y.Z"
git push origin main
git tag -a vX.Y.Z -m "Pipeline OS X.Y.Z"   # annotated (add -s to GPG-sign)
git push origin vX.Y.Z
```

The tag runs [`.github/workflows/release.yml`](.github/workflows/release.yml), which:

1. checks the tag is annotated;
2. builds Windows x64, macOS Apple Silicon, macOS Intel and Linux x64 on native
   runners (Mac updater bundles are named by architecture);
3. signs the auto-update packages and writes `latest.json` for `windows-x86_64`,
   `darwin-aarch64`, `darwin-x86_64` and `linux-x86_64`;
4. writes `SHA256SUMS.txt`, source and artifact SBOMs, Sigstore signatures and
   GitHub build-provenance attestations;
5. **publishes** the GitHub release, which installed apps then offer as an update.

Add a [CHANGELOG.md](CHANGELOG.md) entry in the release commit. If a release has to
be stopped, cancel the run and delete the tag before it publishes; never move or reuse
a tag after a release is public (see [docs/release/RECOVERY.md](docs/release/RECOVERY.md)).

## One-time GitHub configuration

Repository secrets (Settings → Secrets and variables → Actions):

- **`TAURI_SIGNING_PRIVATE_KEY`**: the updater signing key (generated locally,
  never committed).
- **`TAURI_SIGNING_PRIVATE_KEY_PASSWORD`**: its password (empty if none).

The matching public key is embedded in `tauri.conf.json`. Apps read the update feed
from `https://github.com/ojaseminem/Pipeline-OS/releases/latest/download/latest.json`.

## How auto-update works for users

1. On launch (if *Automatically check for updates* is on) and from Settings →
   Updates, the app fetches `latest.json` from the latest release.
2. If a newer version exists for its platform, it offers *Install & restart*.
3. The download is verified against the embedded public key before installing.

## Dev channel

- Every push to `dev` runs [`.github/workflows/dev-build.yml`](.github/workflows/dev-build.yml)
  (also runnable by hand from the Actions tab).
- It builds **PipelineOS Dev** (identifier `com.vantadeck.launcher.dev`, config
  `apps/desktop/src-tauri/tauri.dev.conf.json`), versioned `X.Y.(Z+1)-<run>`, for
  Windows x64, macOS Apple Silicon and macOS Intel.
- The installers, update packages, `latest.json` and checksums replace the
  `dev-latest` pre-release. Pre-releases are never "latest", so stable installs
  never see them.
- Dev installs alongside stable with its own data and updates itself from
  `dev-latest`.

macOS builds (dev and stable) are ad-hoc signed but not notarized: on first launch
use right-click → *Open*, or *System Settings → Privacy & Security → Open Anyway*.

## Continuous builds

[`.github/workflows/build.yml`](.github/workflows/build.yml) builds unsigned
installers on pushes to `main`, `release/**` and pull requests as a check. Only tagged
`release.yml` runs produce signed, updatable releases.
