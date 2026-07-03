# Agent Voice App

A cross-platform desktop **phone for your coding agent**. The agent *calls* you
through an MCP server; you answer in the app and have a natural, spoken
back-and-forth. By default the voice runs **fully on-device** — free, private,
and offline — using [sherpa-onnx](https://github.com/k2-fsa/sherpa-onnx)
(NeMo **Parakeet** for speech-to-text, **Kokoro** for text-to-speech). You can
switch to [ElevenLabs](https://elevenlabs.io) for the voice at any time.

> Built with **Tauri v2** (Rust backend + React/TypeScript webview) and a **Rust
> MCP server** (`rmcp`). With the default local engine your audio never leaves
> your machine at all; if you switch to ElevenLabs, your API key stays in the
> Rust backend and audio is sent only to ElevenLabs.

---

## How it works

```
Agent client (MCP host)
  │  stdio  (call_user / say_and_listen / voice_say / end_call)
  ▼
crates/mcp-server  ──WS client (127.0.0.1 + token)──►  src-tauri WS server (always-on)
        │ returns no_device if the app is down                 │  Tauri events / commands
        ▼                                                      ▼
   result → agent client                              React webview (UI + Web Audio)
                                                      • ring / answer / hang-up
                                                      • mic capture + VAD
                                                      • TTS playback
                                                              │ invoke(bytes)
                                                              ▼
                                                   src-tauri: provider dispatch
                                          ┌───────────────────┴───────────────────┐
                                          ▼                                         ▼
                              local_tts.rs / local_stt.rs              src-tauri/elevenlabs.rs
                              sherpa-onnx (Kokoro + Parakeet)          • TTS /v1/text-to-speech
                              • runs on-device (CPU/CoreML)            • STT /v1/speech-to-text
                                          │                                         │ HTTPS
                                          ▼                                         ▼
                              ~/.copilot/voice-models/                        ElevenLabs
```

- The **voice engine** is selectable per direction (STT and TTS). The default is
  **local** (sherpa-onnx); **ElevenLabs** is opt-in. Settings live in
  `~/.copilot/voice/config.json`.
- The **desktop app** is always-on (lives in the tray/menu bar) and owns the
  hardware — microphone and speaker. It runs a loopback WebSocket server.
- The **MCP server** is ephemeral: the agent client spawns it per session. It is
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
- An MCP-capable agent client. In-app registration supports Copilot CLI, Claude
  Code, Codex CLI, and OpenCode.
- **Optional:** an **ElevenLabs** account + API key — only needed if you switch
  the voice engine to ElevenLabs instead of the default local models (free tier
  is enough: TTS, STT via `scribe_v1`, and premade voices all work).

### Voice engine (local vs ElevenLabs)

The app ships **local-first**. On first run, open **Setup & status** (gear icon)
→ **Voice engine** and click **Download** to fetch the default models into
`~/.copilot/voice-models/` (a one-time ~615 MB total):

| Direction | Default model | Bundle | Download |
| --- | --- | --- | --- |
| Speech-to-text | NeMo Parakeet TDT 0.6b v2 (int8, English) | `sherpa-onnx-nemo-parakeet-tdt-0.6b-v2-int8` | ~483 MB |
| Text-to-speech | Kokoro-82M v1.0 (int8, multi-lang, 50+ voices) | `kokoro-int8-multi-lang-v1_0` | ~132 MB |

After downloading, everything runs on-device (no account, no network, no
per-use cost). The models run on CPU and use Apple's CoreML when the underlying
ONNX Runtime build supports it. In the same card you can pick the local **voice**
and **speaking speed**, or flip either direction to **ElevenLabs**. Your choice
is saved to `~/.copilot/voice/config.json`.

### ElevenLabs config (optional)

Only needed if you switch the voice engine to ElevenLabs. The easiest way is
**in the app**: open **Setup & status** (gear icon on the idle screen), click
**Set up** under *ElevenLabs voice*, paste your API key, **Fetch voices**, pick
one, and **Save**. The app writes `~/.copilot/elevenlabs/config.json` for you
(owner-only) and picks it up on the next call — no restart needed. Fetching
voices also validates the key. Later, click **Change** to switch voice (no need
to re-enter the key) or paste a new key.

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
2. Pick an MCP client and **Register** — writes the MCP server into that
   client's config (see below).

Restart your agent client so it picks up the new MCP server. From an agent
session, ask it to call you (it will invoke `call_user`).

### Registering the MCP server

The app can self-register (Setup panel → *MCP server* → pick a client →
*Register*). It preserves any existing servers and writes one of these config
shapes:

```json
{
  "mcpServers": {
    "agent-voice-mcp": {
      "type": "stdio",
      "command": "/absolute/path/to/agent-voice-mcp",
      "args": [],
      "tools": ["*"]
    }
  }
}
```

Supported clients and default config files:

| Client | Config file |
| --- | --- |
| Copilot CLI | `~/.copilot/mcp-config.json` |
| Claude Code | `~/.claude.json` |
| Codex CLI | `~/.codex/config.toml` |
| OpenCode | `~/.config/opencode/opencode.json` |

The command path points at the MCP binary next to the app executable (works in
both dev and a packaged build). You can also register from the CLI:

```bash
node scripts/install-mcp.mjs                         # build release + register with Copilot CLI
node scripts/install-mcp.mjs --client claude-code
node scripts/install-mcp.mjs --client codex
node scripts/install-mcp.mjs --client opencode
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
`agent-voice-mcp-<target-triple>`), and produces a bundle:

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
3. **Interrupt anytime** — you don't have to wait for the agent to finish:
   - **Just start talking.** Hands-free **barge-in** is on by default: the app
     listens while the agent speaks and, the moment you begin, cuts off its voice
     and captures your reply. Toggle it off with the 🗣️ **Barge-in** control if
     speaker echo false-triggers it (then use the button below instead).
   - **Tap ✋ Interrupt** (or press **Esc**) while the agent is speaking to cut it
     off. For `say_and_listen` this hands you the turn; for a one-way announcement
     it just silences the audio.
4. It listens for your reply — hands-free via **voice activity detection**
   (silence ends your turn) or hold the **push-to-talk** button. Your speech is
   transcribed and returned to the agent.
5. **Mute**, **push-to-talk**, **barge-in**, or **hang up** anytime. If the mic is
   unavailable, a note appears and you can type your reply as a fallback.
6. The agent ends with `end_call` (optional spoken farewell).

---

## Project layout

```
Cargo.toml                    # cargo workspace
src/                          # React webview
  App.tsx, callMachine.ts, callReducer.ts, ipc.ts
  audio/{mic,vad,player,listen}.ts
  components/{IdleScreen,RingScreen,CallScreen,SetupPanel,VoiceEngineCard}.tsx
src-tauri/                    # Tauri backend (Rust)
  src/{lib,ws_server,elevenlabs,config,session,commands,mcp_register,runtime}.rs
  src/{voice_settings,models,local_stt,local_tts}.rs   # local (sherpa-onnx) voice engine
  examples/{voice_probe,local_smoke}.rs                # live round-trip smoke tests
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
cargo run -p agent-voice-app --example local_smoke  # local Kokoro TTS→Parakeet STT (downloads models)
cargo run -p agent-voice-app --example voice_probe   # live ElevenLabs TTS→STT round-trip
node scripts/mcp-smoke.mjs voice_say '{"text":"hello"}' # drive an MCP tool directly
```

---

## Troubleshooting

- **Agent says "no device":** the app isn't running, or the MCP server can't reach
  it. Launch the app and confirm the Setup panel shows *Registered* for the
  client you are using. Delete a
  stale `~/.copilot/voice-call/runtime.json` if the app was force-killed.
- **No microphone prompt / can't hear you:** grant mic access in System Settings →
  Privacy & Security → Microphone. On first use macOS prompts automatically.
- **"MCP binary not found" in Setup:** run `cargo build -p voice-mcp-server` (dev)
  or `pnpm sidecar` before registering.
- **Changed the MCP path:** re-open Setup and choose *Update path*, then restart
  your agent client.

---

## Notes & limits

- The default **local** engine is free and runs offline — no per-turn cost. On
  first use you download ~615 MB of models once.
- If you switch to **ElevenLabs**, free-tier credits are limited; each turn then
  spends TTS + STT credits.
- Batch STT per turn adds ~0.5–1.5s latency; acceptable for v1. Streaming STT is
  planned for v2.
- **Barge-in** (interrupting the agent mid-sentence) is supported — hands-free by
  default, plus a manual ✋ Interrupt button / Esc key. It runs entirely in the
  webview; the agent is unaware it was cut off (it just receives your reply, or its
  usual ack for a one-way announcement). Because the mic is open while the agent
  speaks, loud speaker echo can occasionally false-trigger hands-free barge-in
  despite echo cancellation — toggle it off if that happens.
- If two agent sessions call at once, the app currently handles one call at a
  time; multi-session queueing is a future enhancement.
