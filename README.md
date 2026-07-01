# Copilot Voice Call

A cross-platform desktop **phone for your Copilot agent**. The agent *calls* you
through an MCP server; you answer in the app and have a natural, spoken
back-and-forth. [ElevenLabs](https://elevenlabs.io) provides the agent's voice
(text-to-speech) and transcribes your replies (speech-to-text).

> Built with **Tauri v2** (Rust backend + React/TypeScript webview) and a **Rust
> MCP server** (`rmcp`). Your ElevenLabs API key stays in the Rust backend and
> your audio never leaves your machine except to ElevenLabs.

---

## How it works

```
Copilot Agent (MCP host)
  │  stdio  (call_user / say_and_listen / voice_say / end_call)
  ▼
crates/mcp-server  ──WS client (127.0.0.1 + token)──►  src-tauri WS server (always-on)
        │ returns no_device if the app is down                 │  Tauri events / commands
        ▼                                                      ▼
   result → Copilot                                   React webview (UI + Web Audio)
                                                      • ring / answer / hang-up
                                                      • mic capture + VAD
                                                      • TTS playback
                                                              │ invoke(bytes)
                                                              ▼
                                                     src-tauri/elevenlabs.rs (reqwest)
                                                     • TTS /v1/text-to-speech/{voiceId}
                                                     • STT /v1/speech-to-text (scribe_v1)
                                                              │ HTTPS
                                                              ▼
                                                          ElevenLabs
```

- The **desktop app** is always-on (lives in the tray/menu bar) and owns the
  hardware — microphone and speaker. It runs a loopback WebSocket server.
- The **MCP server** is ephemeral: the Copilot CLI spawns it per session. It is
  the WebSocket *client*. If the app isn't running it returns `no_device` so the
  agent can gracefully fall back to text.
- **Discovery/auth:** on startup the app writes `~/.copilot/voice-call/runtime.json`
  (`{ port, token }`, owner-readable). The MCP server reads it to connect.

---

## MCP tools exposed to the agent

| Tool | Parameters | Returns |
| --- | --- | --- |
| `call_user` | `reason?`, `timeoutSec?` (default 30) | `{ status: answered \| declined \| timeout \| no_device }` |
| `say_and_listen` | `text`, `listen?` (default true), `listenTimeoutSec?` (default 20) | `{ heard, status: ok \| no_speech \| call_ended }` |
| `voice_say` | `text` | `{ status: ok \| call_ended \| error }` (one-way announcement) |
| `end_call` | `farewell?` | `{ status }` (optional goodbye, then hang up) |

Typical flow: `call_user` → (user answers) → one or more `say_and_listen` turns →
`end_call`.

---

## Prerequisites

- **macOS** (primary target; Windows via WebView2 should work, Linux/WebKitGTK is
  best-effort for microphone support).
- **Rust** (stable) + **Cargo**, **Node** 20+, and **pnpm**.
- The **Copilot CLI** with MCP support (`~/.copilot/mcp-config.json`).
- An **ElevenLabs** account + API key (free tier is enough: TTS, STT via
  `scribe_v1`, and premade voices all work).

### ElevenLabs config

The easiest way is **in the app**: open **Setup & status** (gear icon on the
idle screen), click **Set up** under *ElevenLabs voice*, paste your API key,
**Fetch voices**, pick one, and **Save**. The app writes
`~/.copilot/elevenlabs/config.json` for you (owner-only) and picks it up on the
next call — no restart needed. Fetching voices also validates the key. Later,
click **Change** to switch voice (no need to re-enter the key) or paste a new
key.

Prefer to do it by hand? Create `~/.copilot/elevenlabs/config.json`:

```json
{
  "apiKey": "sk_...",
  "voiceId": "JBFqnCBsd6RMkjVDRZzb",
  "voiceName": "George",
  "modelId": "eleven_turbo_v2_5",
  "outputFormat": "mp3_44100_128",
  "maxChars": 800,
  "enabled": true
}
```

Only `apiKey` and `voiceId` are required; the rest have sensible defaults.
`JBFqnCBsd6RMkjVDRZzb` is the premade **George** voice (used as the default when
you set up in-app without choosing another).

---

## Run in development

```bash
pnpm install
cargo build -p voice-mcp-server   # build the MCP server binary (once)
pnpm tauri dev                    # launch the app (Rust backend + webview)
```

Then open **Setup & status** in the app (gear icon on the idle screen) to:

1. Add your ElevenLabs API key and pick a voice (or confirm the one you already
   configured).
2. **Register with Copilot** — writes the MCP server into
   `~/.copilot/mcp-config.json` (see below).

Restart your Copilot CLI session so it picks up the new MCP server. From a
Copilot session, ask the agent to call you (it will invoke `call_user`).

### Registering the MCP server

The app can self-register (Setup panel → *Register with Copilot*). It writes an
entry to `~/.copilot/mcp-config.json`, preserving any existing servers:

```json
{
  "mcpServers": {
    "copilot-voice-mcp": {
      "type": "stdio",
      "command": "/absolute/path/to/copilot-voice-mcp",
      "args": [],
      "tools": ["*"]
    }
  }
}
```

The command path points at the MCP binary next to the app executable (works in
both dev and a packaged build). You can also register from the CLI:

```bash
node scripts/install-mcp.mjs            # build release + register
node scripts/install-mcp.mjs --uninstall
node scripts/install-mcp.mjs --dry-run
```

---

## Build a distributable

```bash
pnpm tauri build
```

This runs the frontend build, stages the MCP server as a Tauri **sidecar**
(`scripts/prepare-sidecar.mjs` builds it and names it
`copilot-voice-mcp-<target-triple>`), and produces a bundle:

- **macOS:** `.app` and `.dmg` under `src-tauri/target/release/bundle/`
- **Windows:** `.msi` / `.exe`
- **Linux:** `.AppImage` / `.deb`

Local builds are unsigned. For distribution you'll need platform code-signing
(Apple Developer ID + notarization on macOS).

To stage the sidecar without a full bundle:

```bash
pnpm sidecar              # release
pnpm sidecar --debug      # debug (for dev)
```

---

## Using it in a call

1. The agent invokes `call_user` → the app rings (window raises, OS notification,
   ringtone) → **Answer** or **Decline**.
2. The agent speaks (`say_and_listen`): you hear the voice and see captions.
3. It listens for your reply — hands-free via **voice activity detection**
   (silence ends your turn) or hold the **push-to-talk** button. Your speech is
   transcribed and returned to the agent.
4. **Mute**, **push-to-talk**, or **hang up** anytime. If the mic is unavailable,
   a note appears and you can type your reply as a fallback.
5. The agent ends with `end_call` (optional spoken farewell).

---

## Project layout

```
Cargo.toml                    # cargo workspace
src/                          # React webview
  App.tsx, callMachine.ts, callReducer.ts, ipc.ts
  audio/{mic,vad,player,listen}.ts
  components/{IdleScreen,RingScreen,CallScreen,SetupPanel}.tsx
src-tauri/                    # Tauri backend (Rust)
  src/{lib,ws_server,elevenlabs,config,session,commands,mcp_register,runtime}.rs
  tauri.conf.json, Info.plist, capabilities/
crates/
  protocol/                   # shared WS wire protocol (serde)
  mcp-server/                 # rmcp stdio server + WS-client bridge
scripts/
  install-mcp.mjs             # register the MCP server (Node)
  prepare-sidecar.mjs         # stage the MCP binary for bundling
  mcp-smoke.mjs               # exercise the MCP server without Copilot
```

---

## Testing

```bash
pnpm test                                  # frontend (Vitest)
cargo test                                 # Rust unit tests
cargo run -p copilot-voice-call --example voice_probe   # live ElevenLabs TTS→STT round-trip
node scripts/mcp-smoke.mjs voice_say '{"text":"hello"}' # drive an MCP tool directly
```

---

## Troubleshooting

- **Agent says "no device":** the app isn't running, or the MCP server can't reach
  it. Launch the app and confirm the Setup panel shows *Registered*. Delete a
  stale `~/.copilot/voice-call/runtime.json` if the app was force-killed.
- **No microphone prompt / can't hear you:** grant mic access in System Settings →
  Privacy & Security → Microphone. On first use macOS prompts automatically.
- **"MCP binary not found" in Setup:** run `cargo build -p voice-mcp-server` (dev)
  or `pnpm sidecar` before registering.
- **Changed the MCP path:** re-open Setup and choose *Update path*, then restart
  your Copilot session.

---

## Notes & limits

- Free-tier ElevenLabs credits are limited; each turn spends TTS + STT credits.
- Batch STT per turn adds ~0.5–1.5s latency; acceptable for v1. Streaming and
  barge-in (interrupting the agent) are planned for v2.
- If two Copilot sessions call at once, the app currently handles one call at a
  time; multi-session queueing is a future enhancement.
