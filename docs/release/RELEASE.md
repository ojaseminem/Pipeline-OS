# Release Runbook

Vantadeck releases are assembled by `.github/workflows/release.yml`. A successful
workflow creates a **draft**, not a promoted release. Promotion is a deliberate human
decision after evidence review and native validation.

## Reproducible inputs

1. Start from a reviewed commit on `main` with green CI and Security workflows.
2. Confirm `Cargo.lock`, `package-lock.json`, the Rust toolchain, Node version, and all
   GitHub Actions are pinned in the tagged commit.
3. Create one immutable, signed `v*` tag. Never move or reuse a release tag.
4. Let the tag workflow build Windows, macOS, and Linux bundles on their corresponding
   GitHub-hosted runners. A manual rebuild must select the existing tag, not a branch.

The workflow records the tag commit, run URL, sorted SHA-256 hashes, source SBOM,
packaged-artifact SBOM, and GitHub provenance/SBOM attestations. Every published file is
also keyless-signed and logged to the public Rekor transparency log, but — to keep the
release page to the files a user actually needs — the signature bundle itself isn't
attached as a separate downloadable asset; verify with `cosign verify-blob` (against
Rekor) or `gh attestation verify` (against the GitHub attestation store) instead of a
bundled `.sigstore.json` file. Native packaging tools can embed timestamps or
nondeterministic metadata, so equal source inputs do not currently promise byte-for-byte
identical installers. Differences must be explained before promotion.

## Evidence review

For every draft:

- Confirm the workflow resolved the expected tag and commit.
- Confirm all three build jobs and the assembly job completed without overrides.
- Download release assets into a clean directory and verify `SHA256SUMS.txt`.
- Verify each asset against Rekor and the expected workflow identity/issuer using
  `cosign verify-blob --certificate-identity ... --certificate-oidc-issuer ...` (no
  local `.sigstore.json` bundle is published; `cosign` resolves the signature from
  Rekor by the file's digest).
- Verify GitHub artifact attestations with `gh attestation verify --repo OWNER/REPO`.
- Review both SBOMs for unexpected packages, missing Rust/Node lockfile coverage, and
  components with prohibited licenses or known critical vulnerabilities.
- Preserve the run URL, reviewer, review time, and verification output in the release
  record. The workflow itself is evidence, but is not evidence of manual validation.

## External promotion gates

The following evidence is required before changing a draft to published:

| Platform | Platform trust gate | Native validation gate |
| --- | --- | --- |
| Windows | Authenticode signature and timestamp validate on every installer | Clean Windows VM install, launch, basic project workflow, uninstall |
| macOS | Developer ID signature, hardened runtime, and Apple notarization validate | Clean supported macOS install, Gatekeeper launch, basic project workflow |
| Linux | Package/repository signature where distributed through a repository | Clean supported distro install for each package format, launch, basic project workflow |

Keyless Sigstore signatures protect artifact identity and transport. They do not
substitute for platform signing. Hosted compilation alone does not satisfy native
validation. Record the OS version, architecture, artifact hash, tester, result, and
evidence link for every row.

## Promotion

1. Confirm release notes accurately describe changes, security impact, migration, and
   rollback considerations.
2. Confirm all external gates have recorded evidence or explicitly remove an
   unvalidated artifact from the release.
3. Have a second maintainer compare the draft assets to `SHA256SUMS.txt` and approve.
4. Publish the existing draft without replacing its assets.
5. Re-download the public assets and repeat checksum, Sigstore, attestation, and
   platform-signature verification.

If any check fails, keep the release draft and follow `RECOVERY.md`.
