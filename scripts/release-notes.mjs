#!/usr/bin/env node
// Prints the release notes for one version: its CHANGELOG.md section, plus a
// short download note. The release workflow publishes this as the release body.
import { readFileSync } from "node:fs";

const [changelogPath, tag] = process.argv.slice(2);
if (!changelogPath || !tag) {
  console.error("usage: release-notes.mjs <CHANGELOG.md> <tag>");
  process.exit(2);
}
const version = tag.replace(/^v/, "");
const lines = readFileSync(changelogPath, "utf8").split(/\r?\n/);
const start = lines.findIndex((line) => line.startsWith(`## ${version} `) || line === `## ${version}`);
if (start === -1) {
  console.error(`CHANGELOG.md has no "## ${version}" section; add one before releasing`);
  process.exit(1);
}
let end = lines.findIndex((line, index) => index > start && line.startsWith("## "));
if (end === -1) end = lines.length;
const body = lines.slice(start + 1, end).join("\n").trim();
if (!body) {
  console.error(`CHANGELOG.md section for ${version} is empty`);
  process.exit(1);
}
process.stdout.write(`${body}

### Download

Pick the installer for your system under **Assets** below: \`.exe\` or \`.msi\` for
Windows, \`aarch64.dmg\` for Apple Silicon Macs, \`x64.dmg\` for Intel Macs, and
\`.AppImage\`, \`.deb\` or \`.rpm\` for Linux. Already installed? Pipeline OS updates
itself.
`);
