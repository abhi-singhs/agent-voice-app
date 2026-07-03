#!/usr/bin/env node
// Build the MCP server binary and stage it as a Tauri "externalBin" sidecar.
//
// Tauri bundles external binaries named `<name>-<target-triple>` and, at runtime,
// places them next to the main app binary under their plain `<name>`. This script
// builds `voice-mcp-server` and copies the result to
// `src-tauri/binaries/agent-voice-mcp-<triple>` so `tauri build` can bundle it.
//
// Usage:
//   node scripts/prepare-sidecar.mjs            # release build (for bundling)
//   node scripts/prepare-sidecar.mjs --debug    # debug build (faster, for dev)
//   node scripts/prepare-sidecar.mjs --no-build # copy an already-built binary

import { execFileSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const BIN_NAME = "agent-voice-mcp";
const __dirname = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(__dirname, "..");

const argv = process.argv.slice(2);
const profile = argv.includes("--debug") ? "debug" : "release";
const build = !argv.includes("--no-build");

/** Determine the Rust host target triple (e.g. aarch64-apple-darwin). */
function hostTriple() {
  const out = execFileSync("rustc", ["-vV"], { encoding: "utf8" });
  const match = out.match(/^host:\s*(\S+)$/m);
  if (!match) {
    console.error("could not determine host triple from `rustc -vV`");
    process.exit(1);
  }
  return match[1];
}

if (build) {
  const args = ["build", "-p", "voice-mcp-server"];
  if (profile === "release") args.push("--release");
  console.log(`Building MCP server (${profile}) ...`);
  execFileSync("cargo", args, { cwd: repoRoot, stdio: "inherit" });
}

const triple = hostTriple();
const exeSuffix = process.platform === "win32" ? ".exe" : "";
const src = join(repoRoot, "target", profile, BIN_NAME + exeSuffix);
if (!existsSync(src)) {
  console.error(
    `binary not found: ${src}\n` +
      `Build it first: cargo build ${profile === "release" ? "--release " : ""}-p voice-mcp-server`,
  );
  process.exit(1);
}

const outDir = join(repoRoot, "src-tauri", "binaries");
mkdirSync(outDir, { recursive: true });
const dest = join(outDir, `${BIN_NAME}-${triple}${exeSuffix}`);
copyFileSync(src, dest);
console.log(`Staged sidecar: ${dest}`);
