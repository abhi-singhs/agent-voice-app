#!/usr/bin/env node
// Register (or update) the Copilot Voice Call MCP server in a supported MCP
// client's config so the agent can call the voice tools.
//
// Idempotent: preserves any existing servers (e.g. playwright) and simply
// upserts the "copilot-voice-mcp" entry with an absolute path to the compiled
// binary.
//
// Usage:
//   node scripts/install-mcp.mjs                         # Copilot CLI
//   node scripts/install-mcp.mjs --client claude-code    # Claude Code
//   node scripts/install-mcp.mjs --client codex          # Codex CLI
//   node scripts/install-mcp.mjs --client opencode       # OpenCode
//   node scripts/install-mcp.mjs --profile debug         # register debug binary
//   node scripts/install-mcp.mjs --no-build              # skip cargo build
//   node scripts/install-mcp.mjs --config <path>         # target custom config path
//   node scripts/install-mcp.mjs --dry-run               # print result, don't write
//   node scripts/install-mcp.mjs --uninstall             # remove the entry

import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const SERVER_NAME = "copilot-voice-mcp";
const __dirname = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(__dirname, "..");

const CLIENTS = {
  copilot: {
    name: "Copilot CLI",
    kind: "copilot-json",
    config: join(homedir(), ".copilot", "mcp-config.json"),
  },
  "claude-code": {
    name: "Claude Code",
    kind: "claude-json",
    config: join(homedir(), ".claude.json"),
  },
  codex: {
    name: "Codex CLI",
    kind: "codex-toml",
    config: join(homedir(), ".codex", "config.toml"),
  },
  opencode: {
    name: "OpenCode",
    kind: "opencode-json",
    config: join(homedir(), ".config", "opencode", "opencode.json"),
  },
};

function normalizeClientId(raw) {
  const id = String(raw ?? "copilot").trim().toLowerCase();
  if (!id || id === "copilot-cli") return "copilot";
  if (id === "claude" || id === "claude_code") return "claude-code";
  if (id === "codex-cli") return "codex";
  if (id === "open-code" || id === "open_code") return "opencode";
  return id;
}

function parseArgs(argv) {
  const opts = {
    profile: "release",
    build: true,
    dryRun: false,
    uninstall: false,
    client: "copilot",
    config: null,
  };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === "--client") opts.client = normalizeClientId(argv[++i]);
    else if (a === "--profile") opts.profile = argv[++i];
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
  if (!CLIENTS[opts.client]) {
    console.error(`--client must be one of: ${Object.keys(CLIENTS).join(", ")}`);
    process.exit(2);
  }
  if (opts.profile !== "release" && opts.profile !== "debug") {
    console.error(`--profile must be "release" or "debug"`);
    process.exit(2);
  }
  opts.config ??= CLIENTS[opts.client].config;
  return opts;
}

function readText(path) {
  if (!existsSync(path)) return "";
  return readFileSync(path, "utf8");
}

function readJsonConfig(path) {
  const raw = readText(path).trim();
  if (!raw) return {};
  try {
    const config = JSON.parse(raw);
    if (!config || typeof config !== "object" || Array.isArray(config)) {
      throw new Error("config root is not a JSON object");
    }
    return config;
  } catch (e) {
    console.error(`failed to parse ${path}: ${e.message}`);
    process.exit(1);
  }
}

function writeConfig(path, content) {
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, content, "utf8");
}

function jsonServerKey(client) {
  return client.kind === "opencode-json" ? "mcp" : "mcpServers";
}

function jsonServerEntry(client, binary) {
  if (client.kind === "opencode-json") {
    return {
      type: "local",
      command: [binary],
      enabled: true,
    };
  }
  if (client.kind === "claude-json") {
    return {
      type: "stdio",
      command: binary,
      args: [],
    };
  }
  return {
    type: "stdio",
    command: binary,
    args: [],
    tools: ["*"],
  };
}

function upsertJson(config, client, binary) {
  const key = jsonServerKey(client);
  if (!config[key] || typeof config[key] !== "object" || Array.isArray(config[key])) {
    config[key] = {};
  }
  config[key][SERVER_NAME] = jsonServerEntry(client, binary);
  return config;
}

function removeJson(config, client) {
  const key = jsonServerKey(client);
  if (config[key] && typeof config[key] === "object" && !Array.isArray(config[key])) {
    delete config[key][SERVER_NAME];
  }
  return config;
}

function tomlString(value) {
  return JSON.stringify(value);
}

function codexTableHeader(line) {
  return /^\s*\[mcp_servers\.(?:"copilot-voice-mcp"|copilot-voice-mcp)\]\s*(?:#.*)?$/.test(
    line,
  );
}

function tomlTableHeader(line) {
  return /^\s*\[[^\]]+\]\s*(?:#.*)?$/.test(line);
}

function removeCodexBlock(raw) {
  const lines = raw.split(/\r?\n/);
  const out = [];
  for (let i = 0; i < lines.length; i++) {
    if (!codexTableHeader(lines[i])) {
      out.push(lines[i]);
      continue;
    }
    i++;
    while (i < lines.length && !tomlTableHeader(lines[i])) i++;
    if (i < lines.length) out.push(lines[i]);
  }
  const cleaned = out.join("\n").replace(/\n{3,}/g, "\n\n").trimEnd();
  return cleaned ? `${cleaned}\n` : "";
}

function upsertCodexToml(raw, binary) {
  const cleaned = removeCodexBlock(raw);
  const prefix = cleaned && !cleaned.endsWith("\n\n") ? `${cleaned}\n` : cleaned;
  return `${prefix}[mcp_servers.${SERVER_NAME}]\ncommand = ${tomlString(binary)}\nargs = []\n`;
}

const opts = parseArgs(process.argv.slice(2));
const client = CLIENTS[opts.client];

if (opts.uninstall) {
  if (client.kind === "codex-toml") {
    const before = readText(opts.config);
    const after = removeCodexBlock(before);
    if (before === after) {
      console.log(`"${SERVER_NAME}" not present in ${opts.config}; nothing to do`);
    } else if (opts.dryRun) {
      console.log(`[dry-run] would remove "${SERVER_NAME}" from ${client.name} config ${opts.config}`);
    } else {
      writeConfig(opts.config, after);
      console.log(`Removed "${SERVER_NAME}" from ${client.name} config ${opts.config}`);
    }
  } else {
    const config = readJsonConfig(opts.config);
    const existed = Boolean(config[jsonServerKey(client)]?.[SERVER_NAME]);
    removeJson(config, client);
    if (!existed) {
      console.log(`"${SERVER_NAME}" not present in ${opts.config}; nothing to do`);
    } else if (opts.dryRun) {
      console.log(`[dry-run] would remove "${SERVER_NAME}" from ${client.name} config ${opts.config}`);
    } else {
      writeConfig(opts.config, JSON.stringify(config, null, 2) + "\n");
      console.log(`Removed "${SERVER_NAME}" from ${client.name} config ${opts.config}`);
    }
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

if (client.kind === "codex-toml") {
  const updated = upsertCodexToml(readText(opts.config), binary);
  if (opts.dryRun) {
    console.log(`[dry-run] would write ${client.name} config ${opts.config}:`);
    console.log(updated);
  } else {
    writeConfig(opts.config, updated);
    console.log(`Registered "${SERVER_NAME}" -> ${binary}`);
    console.log(`Updated ${client.name} config ${opts.config}`);
  }
} else {
  const config = upsertJson(readJsonConfig(opts.config), client, binary);
  if (opts.dryRun) {
    console.log(`[dry-run] would write ${client.name} config ${opts.config}:`);
    console.log(JSON.stringify(config, null, 2));
  } else {
    writeConfig(opts.config, JSON.stringify(config, null, 2) + "\n");
    console.log(`Registered "${SERVER_NAME}" -> ${binary}`);
    console.log(`Updated ${client.name} config ${opts.config}`);
    console.log(`Existing servers preserved: ${Object.keys(config[jsonServerKey(client)]).join(", ")}`);
  }
}
