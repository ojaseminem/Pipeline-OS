# Releasing Vantadeck

Vantadeck uses **trunk-based development with release branches and SemVer tags** —
the model used by most desktop apps (VS Code, etc.) and the one our CI is built
around. It cleanly separates day-to-day development from shippable releases.

## Branches

| Branch | Purpose | Who commits |
| --- | --- | --- |
| `main` | **Development trunk.** All features and fixes land here (directly or via PRs). Always buildable. | everyone |
| `release/X.Y` | **Release line** for a minor version (e.g. `release/0.2`). Cut from `main` when preparing the `X.Y.0` release; only stabilization fixes are cherry-picked in afterward. | maintainers |
| tags `vX.Y.Z` | Immutable release points on a `release/X.Y` branch. Pushing one triggers the release pipeline. | maintainers |

Feature work → branch off `main` → PR into `main`. Releases → `release/X.Y` → tag `vX.Y.Z`.

## Versioning (SemVer)

`MAJOR.MINOR.PATCH`. Keep these three in lockstep:

- `Cargo.toml` → `[workspace.package] version`
- `apps/desktop/package.json` → `version`
- `apps/desktop/src-tauri/tauri.conf.json` → `version`  ← this is what the running
  app reports and what the auto-updater compares against.

Bump on the release branch, commit, then tag.

## Cutting a release

```bash
# 1. From an up-to-date main, cut (or fast-forward) the release line:
git switch -c release/0.2 main          # first time for the 0.2 line
# ...or: git switch release/0.2 && git merge --ff-only main

# 2. Bump the three version fields to X.Y.Z, refresh Cargo.lock, commit:
cargo check -p vantadeck-desktop
git commit -am "release: v0.2.0"

# 3. Create an annotated (ideally signed) tag and push:
git tag -a v0.2.0 -m "Vantadeck 0.2.0"   # add -s to GPG-sign (recommended)
git push origin release/0.2 --tags
```

Pushing the `v*` tag runs [`.github/workflows/release.yml`](.github/workflows/release.yml):
it builds Windows/macOS/Linux bundles, **signs the auto-updater artifacts**,
generates `latest.json`, and publishes a **draft** GitHub Release. Review the
draft (see [docs/release/RELEASE.md](docs/release/RELEASE.md)) and publish it.

## Required GitHub configuration (one-time)

For auto-update and signed release assets to work, add these repository secrets
(Settings → Secrets and variables → Actions):

- **`TAURI_SIGNING_PRIVATE_KEY`** — contents of `apps/desktop/.tauri/updater.key`
  (generated locally; gitignored, never committed). This signs updater artifacts.
- **`TAURI_SIGNING_PRIVATE_KEY_PASSWORD`** — the key's password (empty if none).

The matching **public key is already embedded** in `tauri.conf.json`, and the
updater reads its feed from
`https://github.com/<owner>/Vantadeck/releases/latest/download/latest.json`.
Once a release with `latest.json` is published, installed apps on an older
version will detect and install it (respecting the "Automatically check for
updates" toggle in Settings).

Optional but recommended: enable branch protection on `main` and `release/*`,
and GPG-sign release tags.

## How auto-update works for users

1. The app reports its `tauri.conf.json` version.
2. On launch (if auto-update is on) and via Settings → Updates → "Check for
   updates", it fetches `latest.json` from the latest GitHub Release.
3. If a newer signed version exists, the user sees a banner / Settings prompt and
   can "Install & restart". The download is verified against the embedded public
   key before installing.

## Continuous builds

[`.github/workflows/build.yml`](.github/workflows/build.yml) builds **unsigned**
installers on every push/PR for testing. These are not auto-update sources;
only tagged `release.yml` runs produce signed, updatable releases.
