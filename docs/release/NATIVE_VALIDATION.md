# Native Validation Record

Copy this checklist into the private or public evidence record for each release. Empty
fields are failures, not implied passes.

## Release identity

- Tag:
- Commit:
- Workflow run URL:
- Draft release URL:
- Evidence reviewer and timestamp:

## Artifact evidence

- `SHA256SUMS.txt` verified:
- Sigstore signatures verified against Rekor and the expected workflow identity (`cosign verify-blob`):
- GitHub build provenance verified:
- GitHub SBOM attestation verified:
- Source SBOM reviewed for Rust and Node coverage:
- Artifact SBOM reviewed:

## Platform matrix

| Platform / version / architecture | Artifact and SHA-256 | Platform signature result | Install | Launch | Basic project workflow | Uninstall / cleanup | Tester, time, evidence |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Windows | | | | | | | |
| macOS | | | | | | | |
| Linux AppImage | | N/A or distribution signature evidence | | | | | |
| Linux DEB | | N/A or repository signature evidence | | | | | |
| Linux RPM | | N/A or repository signature evidence | | | | | |

## Decision

- Known limitations:
- Removed or withheld artifacts:
- Rollback/data-compatibility assessment:
- First maintainer approval:
- Second maintainer approval:
- Promotion time or rejection reason:
