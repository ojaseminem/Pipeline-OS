# Contributing

Pipeline OS welcomes code, application manifests, task presets, documentation,
testing, design, translations, and production-workflow reports from every creative
discipline: game development, CG, VFX, animation and technical art.

## Good first contributions

- **Application manifests** for creative tools we don't detect yet
  (`manifests/apps/`, see [docs/APP_MANIFEST_SPEC.md](docs/APP_MANIFEST_SPEC.md)).
- **Task presets** for engines and DCCs (`crates/studio/src/task_presets.rs`).
- **Bug reports** with the version, OS, steps and what you expected.
- **Docs** that make a workflow clearer.

## Before opening a pull request

1. Discuss behavior or public-contract changes (CLI/JSON output, project files,
   MCP tools) in an issue or [RFC](docs/RFC_PROCESS.md) first.
2. Keep changes focused and include tests for behavior.
3. Run the same checks CI runs:

   ```bash
   cargo fmt --all --check
   cargo clippy --workspace --all-targets -- -D warnings
   cargo test --workspace
   npm run typecheck && npm run lint && npm test && npm run build
   npm run validate:contracts
   ```

4. Sign every commit with `git commit -s` to certify the [DCO](DCO.txt).
5. In the pull request, explain user impact, privacy/network impact, test evidence,
   and documentation changes (the template asks for each).

Application manifests and pipeline tasks contain structured argument lists only.
Shell fragments, downloaded scripts, and remote installers are rejected.

Work lands on the `dev` branch first and ships to testers as **PipelineOS Dev**;
`main` is promoted from `dev` for releases. See [RELEASING.md](RELEASING.md).

Be respectful and follow [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md). Security reports
must use the private process in [SECURITY.md](SECURITY.md), not public issues.
