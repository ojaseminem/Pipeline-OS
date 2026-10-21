#!/usr/bin/env node
// Builds the `vantadeck` CLI and stages it as a Tauri sidecar so installers
// ship it next to the desktop app (used for terminals, scripting, and the MCP
// server AI agents connect to). Pair with
// `--config src-tauri/tauri.sidecar.conf.json` on `tauri build`.
//
// Usage: node scripts/prepare-sidecar.mjs [--debug]

import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const debug = process.argv.includes("--debug");
const host = execFileSync("rustc", ["-vV"], { encoding: "utf8" })
  .split("\n")
  .find((line) => line.startsWith("host:"))
  ?.slice(5)
  .trim();
const triple = process.env.TAURI_ENV_TARGET_TRIPLE || host;
if (!triple) throw new Error("could not determine the Rust target triple");

const args = ["build", "-p", "vantadeck", "--locked"];
if (!debug) args.push("--release");
if (triple !== host) args.push("--target", triple);
execFileSync("cargo", args, { cwd: root, stdio: "inherit" });

const ext = triple.includes("windows") ? ".exe" : "";
const profile = debug ? "debug" : "release";
const built = join(root, "target", ...(triple !== host ? [triple] : []), profile, `vantadeck${ext}`);
const dest = join(root, "apps/desktop/src-tauri/binaries", `vantadeck-${triple}${ext}`);
mkdirSync(dirname(dest), { recursive: true });
copyFileSync(built, dest);
console.log(`sidecar staged: ${dest}`);
