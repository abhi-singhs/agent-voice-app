//! Shared protocol types for Copilot Voice Call.
//!
//! These types are the single source of truth for the localhost WebSocket
//! messages exchanged between the Tauri app (WS server) and the MCP server
//! (WS client), plus the runtime-discovery file the app writes so the MCP
//! server can find and authenticate to it.

use serde::{Deserialize, Serialize};

/// The protocol version negotiated between the app and the MCP server.
pub const PROTOCOL_VERSION: u32 = 1;

/// Relative path (under the user's `.copilot` dir) of the runtime-discovery file.
pub const RUNTIME_REL_PATH: &str = ".copilot/voice-call/runtime.json";

fn default_true() -> bool {
    true
}

/// Contents of the runtime-discovery file written by the app on startup.
///
/// The app is the always-on WebSocket *server*; the MCP server is an ephemeral
/// *client* spawned by Copilot. The client reads this file to learn which port
/// to connect to and which token to authenticate with.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeInfo {
    /// Loopback TCP port the app's WebSocket server is listening on.
    pub port: u16,
    /// Shared secret the client must present in [`ClientMessage::Hello`].
    pub token: String,
    /// PID of the app process (used to detect a stale file).
    pub pid: u32,
    /// Protocol version the app speaks.
    pub protocol_version: u32,
}

/// Messages sent from the MCP server (WS client) to the app (WS server).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    /// First message after connecting; authenticates with the shared token.
    Hello {
        token: String,
        protocol_version: u32,
        /// Optional identifier of the Copilot session placing the call.
        #[serde(default)]
        session: Option<String>,
    },
    /// Health check; the app replies with [`ServerMessage::Pong`].
    Ping { id: u64 },
    /// Ring the user. Maps to the `call_user` MCP tool.
    IncomingCall {
        id: u64,
        #[serde(default)]
        reason: Option<String>,
        #[serde(default)]
        timeout_sec: Option<u64>,
    },
    /// Speak `text` (TTS) and optionally capture a spoken reply (VAD + STT).
    /// Maps to the `say_and_listen` MCP tool.
    SayAndListen {
        id: u64,
        text: String,
        #[serde(default = "default_true")]
        listen: bool,
        #[serde(default)]
        listen_timeout_sec: Option<u64>,
    },
    /// One-way announcement (no listening). Maps to the `voice_say` MCP tool.
    Say { id: u64, text: String },
    /// End the call, optionally speaking a farewell first. Maps to `end_call`.
    Hangup {
        id: u64,
        #[serde(default)]
        farewell: Option<String>,
    },
}

/// Messages sent from the app (WS server) to the MCP server (WS client).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    /// Response to [`ClientMessage::Hello`] when the token is valid.
    Welcome { protocol_version: u32 },
    /// Response to [`ClientMessage::Ping`].
    Pong { id: u64 },
    /// Response to [`ClientMessage::IncomingCall`].
    CallResult { id: u64, status: CallStatus },
    /// Response to [`ClientMessage::SayAndListen`].
    ListenResult {
        id: u64,
        #[serde(default)]
        heard: Option<String>,
        status: ListenStatus,
    },
    /// Response to [`ClientMessage::Say`] and [`ClientMessage::Hangup`].
    Ack { id: u64, status: AckStatus },
    /// Unsolicited: the user hung up from the app side.
    UserHungUp,
    /// An error handling a specific request (`id` set) or a protocol error.
    Error {
        #[serde(default)]
        id: Option<u64>,
        message: String,
    },
}

/// Outcome of an [`ClientMessage::IncomingCall`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CallStatus {
    /// The user answered.
    Answered,
    /// The user actively declined.
    Declined,
    /// The ring timed out with no answer.
    Timeout,
    /// No app was reachable (client determines this locally).
    NoDevice,
}

/// Outcome of a listen turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ListenStatus {
    /// Speech was captured and transcribed.
    Ok,
    /// The user was silent for the whole listen window.
    NoSpeech,
    /// The call ended before/while listening.
    CallEnded,
}

/// Outcome of a one-way action ([`ClientMessage::Say`] / [`ClientMessage::Hangup`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AckStatus {
    /// The action completed.
    Ok,
    /// The call had already ended.
    CallEnded,
    /// The action failed.
    Error,
}
