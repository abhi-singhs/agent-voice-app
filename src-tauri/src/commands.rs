//! Tauri commands the webview calls to reply to voice requests and to notify
//! the app of user-initiated events.

use std::sync::Arc;

use tauri::State;
use voice_protocol::{AckStatus, CallStatus, ListenStatus, ServerMessage};

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
