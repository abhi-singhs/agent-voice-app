#!/usr/bin/env node
// Register (or update) the Copilot Voice Call MCP server in the Copilot CLI's
// mcp-config.json so the agent can call the voice tools.
//
// Idempotent: preserves any existing servers (e.g. playwright) and simply
// upserts the "copilot-voice-mcp" entry with an absolute path to the compiled
// binary.
//
// Usage:
//   node scripts/install-mcp.mjs                 # build release, then register
//   node scripts/install-mcp.mjs --profile debug # register the debug binary
//   node scripts/install-mcp.mjs --no-build      # skip cargo build; use existing
//   node scripts/install-mcp.mjs --config <path> # target a custom config file
//   node scripts/install-mcp.mjs --dry-run       # print result, don't write
//   node scripts/install-mcp.mjs --uninstall     # remove the entry

import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const SERVER_NAME = "copilot-voice-mcp";
const __dirname = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(__dirname, "..");

function parseArgs(argv) {
  const opts = { profile: "release", build: true, dryRun: false, uninstall: false };
  opts.config = join(homedir(), ".copilot", "mcp-config.json");
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === "--profile") opts.profile = argv[++i];
    else if (a === "--debug") opts.profile = "debug";
    else if (a === "--no-build") opts.build = false;
    else if (a === "--dry-run") opts.dryRun = true;
    else if (a === "--uninstall") opts.uninstall = true;
    else if (a === "--config") opts.config = resolve(argv[++i]);
    else {
      console.error(`unknown argument: ${a}`);
      process.exit(2);
    }
  }
  if (opts.profile !== "release" && opts.profile !== "debug") {
    console.error(`--profile must be "release" or "debug"`);
    process.exit(2);
  }
  return opts;
}

function readConfig(path) {
  if (!existsSync(path)) return {};
  try {
    const raw = readFileSync(path, "utf8").trim();
    return raw ? JSON.parse(raw) : {};
  } catch (e) {
    console.error(`failed to parse ${path}: ${e.message}`);
    process.exit(1);
  }
}

function writeConfig(path, config) {
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, JSON.stringify(config, null, 2) + "\n", "utf8");
}

const opts = parseArgs(process.argv.slice(2));
const config = readConfig(opts.config);
if (!config.mcpServers || typeof config.mcpServers !== "object") {
  config.mcpServers = {};
}

if (opts.uninstall) {
  if (config.mcpServers[SERVER_NAME]) {
    delete config.mcpServers[SERVER_NAME];
    if (opts.dryRun) {
      console.log(`[dry-run] would remove "${SERVER_NAME}" from ${opts.config}`);
    } else {
      writeConfig(opts.config, config);
      console.log(`Removed "${SERVER_NAME}" from ${opts.config}`);
    }
  } else {
    console.log(`"${SERVER_NAME}" not present in ${opts.config}; nothing to do`);
  }
  process.exit(0);
}

if (opts.build) {
  const args = ["build", "-p", "voice-mcp-server"];
  if (opts.profile === "release") args.push("--release");
  console.log(`Building MCP server (${opts.profile}) ...`);
  execFileSync("cargo", args, { cwd: repoRoot, stdio: "inherit" });
}

const binary = join(repoRoot, "target", opts.profile, SERVER_NAME);
if (!existsSync(binary)) {
  console.error(
    `binary not found: ${binary}\n` +
      `Build it first: cargo build ${opts.profile === "release" ? "--release " : ""}-p voice-mcp-server`,
  );
  process.exit(1);
}

config.mcpServers[SERVER_NAME] = {
  type: "stdio",
  command: binary,
  args: [],
  tools: ["*"],
};

if (opts.dryRun) {
  console.log(`[dry-run] would write ${opts.config}:`);
  console.log(JSON.stringify(config, null, 2));
} else {
  writeConfig(opts.config, config);
  console.log(`Registered "${SERVER_NAME}" -> ${binary}`);
  console.log(`Updated ${opts.config}`);
  console.log(`Existing servers preserved: ${Object.keys(config.mcpServers).join(", ")}`);
}
