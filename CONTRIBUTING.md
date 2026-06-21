# Contributing

Vantadeck welcomes code, application manifests, documentation, testing, design,
translations, and production-workflow reports from all creative disciplines.

## Before opening a pull request

1. Discuss behavior or public-contract changes in an issue or RFC.
2. Keep changes focused and include tests for behavior.
3. Run `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
   `cargo test --workspace`, `npm test`, and `npm run build`.
4. Sign every commit with `git commit -s` to certify the DCO.
5. Explain user impact, privacy/network impact, test evidence, and documentation changes.

Application manifests must contain structured arguments only. Shell fragments,
downloaded scripts, and remote installers are rejected. See
[docs/APP_MANIFEST_SPEC.md](docs/APP_MANIFEST_SPEC.md).

Be respectful and follow [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md). Security reports
must use the private process in [SECURITY.md](SECURITY.md), not public issues.
