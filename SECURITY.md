# Security Policy

## Reporting

Use GitHub private vulnerability reporting for this repository. Do not open a public
issue or include exploit details in public logs. Include affected version, impact,
reproduction, and suggested mitigation when possible.

Maintainers target acknowledgement within three business days, triage within seven
days, and coordinated disclosure after a fix is available. Critical issues may
require an accelerated release and temporary feature disablement.

Do not attach production credentials, Perforce tickets, private repository URLs,
customer assets, or unsanitized logs. If GitHub private vulnerability reporting is
unavailable, use the private security contact listed on the repository owner profile
and include only enough information to establish a secure follow-up channel.

## Supported versions

Before 1.0, only the newest published release (currently the 0.5.x line) receives
security fixes; the in-app updater moves installs to it. After 1.0, the current minor
line and the latest patch of the previous minor line will receive security fixes.

Pipeline OS never accepts shell commands from manifests or pipeline tasks, never
automatically executes downloaded tools, and stores no credentials in SQLite or
project files (tokens go to the OS keychain). Pipeline tasks that arrive through a
repository, or that an AI agent writes over MCP, run only after the user reviews and
trusts the exact commands on that machine; any change to the tasks file revokes trust.

## Supply-chain controls

Pull requests and the default branch run secret, dependency, advisory, source, and
license checks. Release workflows build each desktop target on its native GitHub-hosted
runner and produce SHA-256 checksums, source and artifact SBOMs, and GitHub build
provenance/SBOM attestations. Every published file is also keyless-signed with Sigstore
and logged to the public Rekor transparency log; the release page itself only carries
the installers, checksums, and SBOMs (not a redundant `.sigstore.json` bundle per file)
— verify a download with `cosign verify-blob` against Rekor or `gh attestation verify`
against the GitHub attestation store. Actions are pinned to immutable revisions and
workflows use least-privilege job permissions.

Auto-update packages are signed with the project's updater key and verified by the app
before installing. These controls do not replace Apple notarization, Windows
Authenticode signing, or Linux package-repository signing, which are still pending
(see `docs/release/BLOCKERS.md`), so installers show an "unknown publisher" or
Gatekeeper prompt on first launch.

## Verification and response

Treat a failed security workflow, unverifiable checksum/signature/attestation, or
unexpected SBOM component as a release blocker. Preserve the workflow URL, commit,
logs, artifacts, and investigation notes. Follow `docs/release/RECOVERY.md` for release
recovery or rollback; never overwrite an existing tag or published artifact in place.
