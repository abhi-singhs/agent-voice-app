# Contributing to Agent Voice App

Thanks for your interest in contributing! Agent Voice App is a cross-platform
desktop **phone for your coding agent**, built with **Tauri v2** (Rust backend +
React/TypeScript webview) and a **Rust MCP server** (`rmcp`). This guide covers
how to set up your environment, make changes, and open a pull request.

## Code of Conduct

Be respectful and constructive. We want this to be a welcoming project for
everyone. Please keep discussions focused on the technical merits of a change.

## Prerequisites

- **macOS** (primary target; Windows via WebView2 should work, Linux/WebKitGTK is
  best-effort for microphone support).
- **Rust** (stable) + **Cargo**, **Node** 20+, and **pnpm**.
- An MCP-capable agent client for end-to-end testing (Copilot CLI, Claude Code,
  Codex CLI, or OpenCode).
- **Optional:** an **ElevenLabs** account + API key — only needed if you work on
  the ElevenLabs voice path (the default local engine needs no account).

## Getting started

1. **Fork** the repository and clone your fork.
2. Install dependencies and build the MCP server once:

   ```bash
   pnpm install
   cargo build -p voice-mcp-server
   ```

3. Run the app in development:

   ```bash
   pnpm tauri dev
   ```

See the [README](README.md) for more detail on the voice engine, registering the
MCP server, and how the pieces fit together.

## Project layout

```
Cargo.toml                    # cargo workspace
src/                          # React webview
src-tauri/                    # Tauri backend (Rust)
crates/
  protocol/                   # shared WS wire protocol (serde)
  mcp-server/                 # rmcp stdio server + WS-client bridge
scripts/                      # Node helper scripts
```

## Making changes

- Create a topic branch off `main` for your work.
- Keep changes focused and as small as possible; unrelated changes make review
  harder.
- Match the existing code style. TypeScript lives under `src/`; Rust lives under
  `src-tauri/` and `crates/`.
- Update documentation (including the README) when your change affects setup,
  behavior, or the MCP tools.

## Testing

Please run the relevant checks before opening a pull request:

```bash
pnpm test                                  # frontend (Vitest)
pnpm build                                 # type-check + build the webview
cargo test                                 # Rust unit tests
cargo build --workspace                    # compile the Rust workspace
```

Optional live/round-trip smoke tests (these download models or hit ElevenLabs):

```bash
cargo run -p agent-voice-app --example local_smoke  # local Kokoro TTS→Parakeet STT
cargo run -p agent-voice-app --example voice_probe   # live ElevenLabs TTS→STT round-trip
node scripts/mcp-smoke.mjs voice_say '{"text":"hello"}' # drive an MCP tool directly
```

CI (`.github/workflows/ci.yml`) runs the frontend tests + build and
compiles/tests the Rust workspace on Linux, macOS, and Windows for every push and
pull request to `main`, so make sure those pass locally first.

## Submitting a pull request

1. Ensure your branch is up to date with `main` and that tests pass.
2. Push your branch and open a pull request against `main`.
3. In the description, explain **what** changed and **why**, and link any related
   issues.
4. Keep the PR scoped to a single logical change where possible.
5. Be responsive to review feedback — maintainers may request adjustments before
   merging.

## Reporting bugs and requesting features

Open an issue with:

- A clear title and description.
- Your platform (macOS/Windows/Linux) and versions (Rust, Node, pnpm).
- Steps to reproduce, expected vs. actual behavior, and any relevant logs.

Thanks again for contributing!
