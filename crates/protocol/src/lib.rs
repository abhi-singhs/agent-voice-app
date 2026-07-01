//! Shared protocol types for Copilot Voice Call.
//!
//! These types are the single source of truth for the localhost WebSocket
//! messages exchanged between the Tauri app (WS server) and the MCP server
//! (WS client). The full message enums are implemented in phase P1.

/// The protocol version negotiated between the app and the MCP server.
pub const PROTOCOL_VERSION: u32 = 1;
