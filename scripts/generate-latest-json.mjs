#!/usr/bin/env node
// Builds the Tauri updater feed (`latest.json`) from the signed updater
// artifacts produced by the release build, and copies those artifacts into the
// release-assets directory so the feed URLs resolve to published assets.
//
// Usage: node scripts/generate-latest-json.mjs <artifactsDir> <releaseAssetsDir> <tag>
//
// The updater plugin downloads the artifact named in `url` and verifies it
// against the `signature` (the contents of the matching `.sig` file) using the
// public key embedded in tauri.conf.json. SHA256SUMS is for human verification;
// updater integrity comes from this signature.

import { readdirSync, statSync, readFileSync, copyFileSync, writeFileSync } from "node:fs";
import { join, basename } from "node:path";

const [, , artifactsDir, releaseAssetsDir, rawTag] = process.argv;
if (!artifactsDir || !releaseAssetsDir || !rawTag) {
  console.error("usage: generate-latest-json.mjs <artifactsDir> <releaseAssetsDir> <tag>");
  process.exit(1);
}

const version = rawTag.replace(/^v/, "");
const repo = process.env.GITHUB_REPOSITORY ?? "ojaseminem/Vantadeck";
const server = process.env.GITHUB_SERVER_URL ?? "https://github.com";
const downloadBase = `${server}/${repo}/releases/download/${rawTag}`;

function walk(dir) {
  const out = [];
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) out.push(...walk(full));
    else out.push(full);
  }
  return out;
}

// Map an updater artifact filename to a Tauri updater platform key.
function platformFor(name) {
  if (name.endsWith("-setup.exe")) return "windows-x86_64";
  if (name.endsWith(".nsis.zip")) return "windows-x86_64";
  if (name.endsWith(".msi.zip")) return "windows-x86_64";
  if (name.endsWith(".AppImage")) return "linux-x86_64";
  if (name.endsWith(".app.tar.gz")) return name.includes("aarch64") ? "darwin-aarch64" : "darwin-x86_64";
  return null;
}

const files = walk(artifactsDir);
const signatures = files.filter((f) => f.endsWith(".sig"));
const platforms = {};

for (const sigPath of signatures) {
  const artifactPath = sigPath.slice(0, -4); // drop ".sig"
  const name = basename(artifactPath);
  const platform = platformFor(name);
  if (!platform) continue;
  // Copy the updater artifact next to the release assets so the URL resolves.
  copyFileSync(artifactPath, join(releaseAssetsDir, name));
  copyFileSync(sigPath, join(releaseAssetsDir, basename(sigPath)));
  platforms[platform] = {
    signature: readFileSync(sigPath, "utf8").trim(),
    url: `${downloadBase}/${name}`,
  };
}

if (Object.keys(platforms).length === 0) {
  console.error("no updater artifacts (.sig) found; updater feed not generated");
  process.exit(1);
}

const feed = {
  version,
  notes: `Vantadeck ${rawTag}. See the release notes for details.`,
  pub_date: new Date().toISOString(),
  platforms,
};

writeFileSync(join(releaseAssetsDir, "latest.json"), `${JSON.stringify(feed, null, 2)}\n`);
console.log(`latest.json generated for ${Object.keys(platforms).join(", ")}`);
