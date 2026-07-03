// Smoke test for the Agent Voice App WebSocket server (phase P1).
//
// Starts the desktop app first (`pnpm tauri dev`), then run:
//   node scripts/ws-smoke.mjs
//
// It reads ~/.copilot/voice-call/runtime.json, connects to the loopback WS
// server, performs the Hello handshake, and exchanges a Ping/Pong.

import { readFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

const runtimePath = join(homedir(), ".copilot", "voice-call", "runtime.json");

let runtime;
try {
  runtime = JSON.parse(readFileSync(runtimePath, "utf8"));
} catch (err) {
  console.error(`Could not read ${runtimePath}: ${err.message}`);
  console.error("Is the app running? Start it with `pnpm tauri dev`.");
  process.exit(1);
}

const { port, token, protocol_version } = runtime;
console.log(`runtime.json: port=${port} protocol_version=${protocol_version}`);

const ws = new WebSocket(`ws://127.0.0.1:${port}`);
const deadline = setTimeout(() => {
  console.error("Timed out waiting for server responses.");
  process.exit(1);
}, 5000);

let gotWelcome = false;

ws.addEventListener("open", () => {
  ws.send(JSON.stringify({ type: "hello", token, protocol_version: 1, session: "smoke" }));
});

ws.addEventListener("message", (ev) => {
  const msg = JSON.parse(ev.data);
  console.log("recv:", msg);
  if (msg.type === "welcome") {
    gotWelcome = true;
    ws.send(JSON.stringify({ type: "ping", id: 42 }));
  } else if (msg.type === "pong" && msg.id === 42) {
    if (!gotWelcome) {
      console.error("FAIL: pong before welcome");
      process.exit(1);
    }
    console.log("PASS: handshake + ping/pong round-trip OK");
    clearTimeout(deadline);
    ws.close();
    process.exit(0);
  } else if (msg.type === "error") {
    console.error("FAIL: server error:", msg.message);
    process.exit(1);
  }
});

ws.addEventListener("error", (err) => {
  console.error("WebSocket error:", err.message ?? err);
  process.exit(1);
});
