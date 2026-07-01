//! Copilot Voice Call MCP server.
//!
//! This binary is spawned by the Copilot CLI over stdio. It exposes voice tools
//! (`call_user`, `say_and_listen`, `voice_say`, `end_call`) and bridges them to
//! the always-on desktop app via a localhost WebSocket connection.
//!
//! The full implementation lands in phase P2; this stub keeps the workspace
//! compiling from P0 onward.

fn main() {
    eprintln!(
        "copilot-voice-mcp {} (protocol v{}): not yet implemented",
        env!("CARGO_PKG_VERSION"),
        voice_protocol::PROTOCOL_VERSION
    );
}
