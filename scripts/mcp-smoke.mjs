#!/usr/bin/env node
// MCP smoke test for copilot-voice-mcp.
//
// Speaks newline-delimited JSON-RPC (the MCP stdio framing) to the compiled
// server binary: initialize -> tools/list -> optionally call a tool.
//
// Usage:
//   node scripts/mcp-smoke.mjs                      # initialize + tools/list
//   node scripts/mcp-smoke.mjs call_user            # + invoke call_user (short ring)
//   node scripts/mcp-smoke.mjs <tool> '<jsonArgs>'  # invoke any tool with args
//
// With no app running, call_user should return {"status":"no_device"}.
// With the app running (no UI answering), a 2s ring should return
// {"status":"timeout"} — proving the MCP -> WS -> app round-trip.

import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";

const __dirname = dirname(fileURLToPath(import.meta.url));
const bin = resolve(__dirname, "..", "target", "debug", "copilot-voice-mcp");

const toolName = process.argv[2] || null;
const toolArgs = process.argv[3]
  ? JSON.parse(process.argv[3])
  : toolName === "call_user"
    ? { reason: "smoke test", timeoutSec: 2 }
    : {};

const child = spawn(bin, [], { stdio: ["pipe", "pipe", "inherit"] });

let buf = "";
const pending = new Map();
let nextId = 1;

function send(method, params) {
  const id = nextId++;
  const msg = { jsonrpc: "2.0", id, method, params };
  child.stdin.write(JSON.stringify(msg) + "\n");
  return new Promise((res) => pending.set(id, res));
}

function notify(method, params) {
  child.stdin.write(JSON.stringify({ jsonrpc: "2.0", method, params }) + "\n");
}

child.stdout.on("data", (chunk) => {
  buf += chunk.toString();
  let nl;
  while ((nl = buf.indexOf("\n")) >= 0) {
    const line = buf.slice(0, nl).trim();
    buf = buf.slice(nl + 1);
    if (!line) continue;
    let msg;
    try {
      msg = JSON.parse(line);
    } catch {
      console.error("non-JSON line:", line);
      continue;
    }
    if (msg.id != null && pending.has(msg.id)) {
      pending.get(msg.id)(msg);
      pending.delete(msg.id);
    }
  }
});

const fail = (m) => {
  console.error("FAIL:", m);
  child.kill("SIGKILL");
  process.exit(1);
};
child.on("error", (e) => fail(`spawn error: ${e.message}`));

const t = setTimeout(() => fail("timed out"), 60000);

try {
  const init = await send("initialize", {
    protocolVersion: "2025-06-18",
    capabilities: {},
    clientInfo: { name: "mcp-smoke", version: "0.0.0" },
  });
  console.log("initialize ok:", JSON.stringify(init.result?.serverInfo ?? init.result));
  notify("notifications/initialized", {});

  const list = await send("tools/list", {});
  const tools = list.result?.tools ?? [];
  console.log(`tools/list -> ${tools.length} tools:`);
  for (const tool of tools) console.log(`  - ${tool.name}`);

  const expected = ["call_user", "say_and_listen", "voice_say", "end_call"];
  const names = tools.map((x) => x.name).sort();
  if (JSON.stringify(names) !== JSON.stringify([...expected].sort())) {
    fail(`unexpected tool set: ${names.join(", ")}`);
  }
  console.log("PASS: all 4 tools present");

  if (toolName) {
    console.log(`\ncalling ${toolName}(${JSON.stringify(toolArgs)}) ...`);
    const res = await send("tools/call", { name: toolName, arguments: toolArgs });
    const text = res.result?.content?.map((c) => c.text).join("") ?? JSON.stringify(res);
    console.log(`${toolName} ->`, text);
  }

  clearTimeout(t);
  child.kill("SIGKILL");
  process.exit(0);
} catch (e) {
  fail(e.message);
}
