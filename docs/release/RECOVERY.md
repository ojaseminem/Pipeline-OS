# Release Recovery and Rollback

## Failed draft assembly

1. Leave the failed run and its logs intact.
2. Record the tag, commit, failing job, runner image, and error.
3. Determine whether the failure is transient infrastructure, compromised evidence,
   source defects, or signing/validation failure.
4. Fix source or workflow defects through normal review. Create a new version tag for
   changed source. Do not move the existing tag.
5. A manual rebuild may target the same tag only when source and workflow inputs are
   unchanged and the failure was transient. Compare hashes and explain differences.

Never promote a partial matrix, reuse artifacts from an unrelated run, disable a gate
without review, or replace draft assets without preserving the old evidence.

## Compromised or invalid artifact

1. Unpublish the draft or affected release and stop distribution immediately.
2. Revoke or rotate affected credentials and platform certificates when applicable.
3. Preserve the release assets, hashes, attestations, workflow logs, and access audit
   trail in restricted incident storage.
4. Open the private security response process from `SECURITY.md`.
5. Build a corrected release from a reviewed commit under a new version tag.
6. Publish an advisory that identifies affected versions and hashes without exposing
   exploit details prematurely.

## Published release rollback

Desktop software cannot be remotely rolled back safely. Rollback means stopping new
downloads, restoring a known-good published version as the recommended version, and
shipping a forward-fix release.

1. Mark the affected release unavailable and state why it was withdrawn.
2. Verify the last known-good release's hashes, signatures, attestations, and support
   status before recommending it.
3. Identify configuration or data migrations that make downgrade unsafe. Provide a
   tested recovery path and backups before asking users to downgrade.
4. Create a forward-fix from reviewed source with a new tag and complete every release
   gate again.
5. After containment, document cause, scope, timeline, evidence, and preventive action.

## Recovery exercise

At least once per release cycle, perform a tabletop exercise covering an unavailable
runner, leaked signing identity, malicious dependency, invalid notarization, and a
data-migration regression. Record owners, elapsed recovery time, and unresolved gaps.
