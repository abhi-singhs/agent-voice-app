//! Tauri commands the webview calls to reply to voice requests and to notify
//! the app of user-initiated events.

use std::sync::Arc;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use reqwest::Client;
use tauri::ipc::Response;
use tauri::State;
use voice_protocol::{AckStatus, CallStatus, ListenStatus, ServerMessage};

use crate::config::{ElevenLabsConfig, VoiceConfigInfo};
use crate::elevenlabs;
use crate::mcp_register::{self, McpStatus};
use crate::session::{FrontendResponse, SessionManager};

/// Reply to an `IncomingCall` request (answered / declined / timeout).
#[tauri::command]
pub fn respond_call(state: State<'_, Arc<SessionManager>>, id: u64, status: CallStatus) {
    state.complete_pending(id, FrontendResponse::Call(status));
}

/// Reply to a `SayAndListen` request with the transcript (if any) and status.
#[tauri::command]
pub fn respond_listen(
    state: State<'_, Arc<SessionManager>>,
    id: u64,
    heard: Option<String>,
    status: ListenStatus,
) {
    state.complete_pending(id, FrontendResponse::Listen { heard, status });
}

/// Reply to a one-way request (`Say` / `Hangup`).
#[tauri::command]
pub fn respond_ack(state: State<'_, Arc<SessionManager>>, id: u64, status: AckStatus) {
    state.complete_pending(id, FrontendResponse::Ack(status));
}

/// Notify the connected client that the user hung up from the app.
#[tauri::command]
pub fn notify_hangup(state: State<'_, Arc<SessionManager>>) {
    state.send_to_client(ServerMessage::UserHungUp);
}

/// Return the non-secret ElevenLabs config for the UI (or an error if unset).
#[tauri::command]
pub fn voice_config() -> Result<VoiceConfigInfo, String> {
    ElevenLabsConfig::load()
        .map(|c| c.info())
        .map_err(|e| e.to_string())
}

/// Synthesize speech for `text`; returns raw audio bytes (mp3) to the webview.
#[tauri::command]
pub async fn tts(client: State<'_, Client>, text: String) -> Result<Response, String> {
    let cfg = ElevenLabsConfig::load().map_err(|e| e.to_string())?;
    let bytes = elevenlabs::synthesize(&client, &cfg, &text)
        .await
        .map_err(|e| e.to_string())?;
    eprintln!("voice-call: tts {} chars -> {} bytes", text.len(), bytes.len());
    Ok(Response::new(bytes))
}

/// Transcribe base64-encoded audio; returns the recognized text.
#[tauri::command]
pub async fn stt(
    client: State<'_, Client>,
    audio: String,
    mime: String,
    filename: String,
) -> Result<String, String> {
    let bytes = STANDARD
        .decode(audio.as_bytes())
        .map_err(|e| format!("invalid base64 audio: {e}"))?;
    let cfg = ElevenLabsConfig::load().map_err(|e| e.to_string())?;
    let text = elevenlabs::transcribe(&client, &cfg, bytes, &mime, &filename)
        .await
        .map_err(|e| e.to_string())?;
    eprintln!("voice-call: stt {} bytes -> {:?}", audio.len(), text);
    Ok(text)
}

/// Report whether the MCP server is registered with the Copilot CLI.
#[tauri::command]
pub fn mcp_status() -> Result<McpStatus, String> {
    mcp_register::status().map_err(|e| e.to_string())
}

/// Register (or update) the MCP server in `~/.copilot/mcp-config.json`.
#[tauri::command]
pub fn register_mcp() -> Result<McpStatus, String> {
    mcp_register::register().map_err(|e| e.to_string())
}

/// Remove the MCP server entry from `~/.copilot/mcp-config.json`.
#[tauri::command]
pub fn unregister_mcp() -> Result<McpStatus, String> {
    mcp_register::unregister().map_err(|e| e.to_string())
}
