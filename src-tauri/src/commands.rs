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
use crate::history::{self, CallRecord};
use crate::mcp_register::{self, McpClientInfo, McpStatus};
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

/// Resolve which API key to use: the one passed from the UI, else the stored key.
fn resolve_key(api_key: Option<String>) -> Result<String, String> {
    if let Some(k) = api_key {
        let k = k.trim().to_string();
        if !k.is_empty() {
            return Ok(k);
        }
    }
    let stored = ElevenLabsConfig::load_or_default().api_key.trim().to_string();
    if stored.is_empty() {
        return Err("No API key provided and none saved yet.".to_string());
    }
    Ok(stored)
}

/// List the voices available for an API key (the passed one, or the stored key
/// when omitted). Also validates the key — an invalid key returns an error.
#[tauri::command]
pub async fn list_voices(
    client: State<'_, Client>,
    api_key: Option<String>,
) -> Result<Vec<elevenlabs::VoiceSummary>, String> {
    let key = resolve_key(api_key)?;
    elevenlabs::list_voices(&client, &key)
        .await
        .map_err(|e| e.to_string())
}

/// Save the ElevenLabs API key and selected voice, merging with any existing
/// config so unrelated fields (model, format, etc.) are preserved.
#[tauri::command]
pub fn save_voice_config(
    api_key: Option<String>,
    voice_id: String,
    voice_name: Option<String>,
) -> Result<VoiceConfigInfo, String> {
    let voice_id = voice_id.trim().to_string();
    if voice_id.is_empty() {
        return Err("A voice must be selected.".to_string());
    }

    let mut cfg = ElevenLabsConfig::load_or_default();

    if let Some(k) = api_key {
        let k = k.trim().to_string();
        if !k.is_empty() {
            cfg.api_key = k;
        }
    }
    if cfg.api_key.trim().is_empty() {
        return Err("An API key is required.".to_string());
    }

    cfg.voice_id = voice_id;
    let new_name = voice_name
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    if new_name.is_some() {
        cfg.voice_name = new_name;
    }

    cfg.save().map_err(|e| e.to_string())?;
    Ok(cfg.info())
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

/// List clients that can receive the bundled MCP server config.
#[tauri::command]
pub fn mcp_clients() -> Result<Vec<McpClientInfo>, String> {
    mcp_register::clients().map_err(|e| e.to_string())
}

/// Report whether the MCP server is registered with the selected client.
#[tauri::command]
pub fn mcp_status(client_id: Option<String>) -> Result<McpStatus, String> {
    mcp_register::status(client_id).map_err(|e| e.to_string())
}

/// Register (or update) the MCP server in the selected client's config.
#[tauri::command]
pub fn register_mcp(client_id: Option<String>) -> Result<McpStatus, String> {
    mcp_register::register(client_id).map_err(|e| e.to_string())
}

/// Remove the MCP server entry from the selected client's config.
#[tauri::command]
pub fn unregister_mcp(client_id: Option<String>) -> Result<McpStatus, String> {
    mcp_register::unregister(client_id).map_err(|e| e.to_string())
}

/// Enable or disable do-not-disturb (auto-decline incoming calls).
#[tauri::command]
pub fn set_dnd(state: State<'_, Arc<SessionManager>>, on: bool) {
    state.set_dnd(on);
}

/// Whether do-not-disturb is currently enabled.
#[tauri::command]
pub fn get_dnd(state: State<'_, Arc<SessionManager>>) -> bool {
    state.dnd()
}

/// Persist a completed call to history.
#[tauri::command]
pub fn save_call(record: CallRecord) -> Result<(), String> {
    history::save(record).map_err(|e| e.to_string())
}

/// List recent calls, newest first (optionally limited).
#[tauri::command]
pub fn list_calls(limit: Option<usize>) -> Result<Vec<CallRecord>, String> {
    history::list(limit).map_err(|e| e.to_string())
}

/// Delete all saved call history.
#[tauri::command]
pub fn clear_calls() -> Result<(), String> {
    history::clear().map_err(|e| e.to_string())
}
