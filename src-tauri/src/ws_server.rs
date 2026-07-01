//! Always-on localhost WebSocket server embedded in the Tauri app.
//!
//! Binds to `127.0.0.1` on an ephemeral port, authenticates clients with a
//! shared token, answers health checks directly, and bridges user-facing
//! requests to the webview via [`SessionManager`].

use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::mpsc;
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::Message;
use voice_protocol::{
    AckStatus, CallStatus, ClientMessage, ListenStatus, ServerMessage, PROTOCOL_VERSION,
};

use crate::session::{FrontendRequest, FrontendResponse, SessionManager, REQUEST_EVENT};

/// Default ring timeout if the client doesn't specify one.
const DEFAULT_CALL_TIMEOUT_SEC: u64 = 30;
/// Default listen timeout if the client doesn't specify one.
const DEFAULT_LISTEN_TIMEOUT_SEC: u64 = 20;
/// Safety-net timeout for one-way actions (say / hangup) awaiting the webview.
const DEFAULT_ACTION_TIMEOUT_SEC: u64 = 300;

/// Bind the WS server and spawn its accept loop. Returns the chosen port.
pub fn start(app: AppHandle, state: Arc<SessionManager>) -> anyhow::Result<u16> {
    // Bind synchronously so we can report the port before returning.
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0))?;
    listener.set_nonblocking(true)?;
    let port = listener.local_addr()?.port();
    let token = state.token().to_string();

    tauri::async_runtime::spawn(async move {
        let listener = match tokio::net::TcpListener::from_std(listener) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("voice-call: failed to adopt WS listener: {e}");
                return;
            }
        };
        loop {
            match listener.accept().await {
                Ok((stream, peer)) => {
                    // Defense in depth: only accept loopback peers.
                    if !peer.ip().is_loopback() {
                        eprintln!("voice-call: rejected non-loopback peer {peer}");
                        continue;
                    }
                    let app = app.clone();
                    let state = state.clone();
                    let token = token.clone();
                    tauri::async_runtime::spawn(async move {
                        if let Err(e) = handle_connection(stream, token, state.clone(), app).await {
                            eprintln!("voice-call: connection ended: {e}");
                        }
                        state.set_outbound(None);
                        state.clear_pending();
                    });
                }
                Err(e) => {
                    eprintln!("voice-call: accept error: {e}");
                    break;
                }
            }
        }
    });

    Ok(port)
}

async fn handle_connection(
    stream: tokio::net::TcpStream,
    token: String,
    state: Arc<SessionManager>,
    app: AppHandle,
) -> anyhow::Result<()> {
    let ws = tokio_tungstenite::accept_async(stream).await?;
    let (mut write, mut read) = ws.split();

    // Drive all outbound frames from a single channel so both the request
    // handlers and spontaneous events (e.g. user hangup) can send safely.
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<ServerMessage>();
    let writer = tauri::async_runtime::spawn(async move {
        while let Some(msg) = out_rx.recv().await {
            let text = serde_json::to_string(&msg).unwrap_or_default();
            if write.send(Message::Text(text.into())).await.is_err() {
                break;
            }
        }
        let _ = write.close().await;
    });

    state.set_outbound(Some(out_tx.clone()));

    let mut authed = false;
    while let Some(msg) = read.next().await {
        let msg = msg?;
        if msg.is_close() {
            break;
        }
        if !msg.is_text() {
            continue;
        }
        let text = msg.to_text()?;
        let cm: ClientMessage = match serde_json::from_str(text) {
            Ok(v) => v,
            Err(e) => {
                let _ = out_tx.send(ServerMessage::Error {
                    id: None,
                    message: format!("malformed message: {e}"),
                });
                continue;
            }
        };

        match cm {
            ClientMessage::Hello { token: t, .. } => {
                if t == token {
                    authed = true;
                    let _ = out_tx.send(ServerMessage::Welcome {
                        protocol_version: PROTOCOL_VERSION,
                    });
                } else {
                    let _ = out_tx.send(ServerMessage::Error {
                        id: None,
                        message: "invalid token".into(),
                    });
                    break;
                }
            }
            _ if !authed => {
                let _ = out_tx.send(ServerMessage::Error {
                    id: None,
                    message: "unauthenticated: send Hello first".into(),
                });
                break;
            }
            ClientMessage::Ping { id } => {
                let _ = out_tx.send(ServerMessage::Pong { id });
            }
            other => {
                // Handle user-facing requests concurrently so the read loop
                // stays responsive; replies are correlated by id.
                let app = app.clone();
                let state = state.clone();
                let out = out_tx.clone();
                tauri::async_runtime::spawn(handle_request(other, state, app, out));
            }
        }
    }

    state.set_outbound(None);
    state.clear_pending();
    drop(out_tx);
    let _ = writer.await;
    Ok(())
}

/// Forward a user-facing request to the webview and relay its reply.
async fn handle_request(
    cm: ClientMessage,
    state: Arc<SessionManager>,
    app: AppHandle,
    out: mpsc::UnboundedSender<ServerMessage>,
) {
    match cm {
        ClientMessage::IncomingCall {
            id,
            reason,
            timeout_sec,
        } => {
            // Do-not-disturb: auto-decline without ringing or revealing the app.
            if state.dnd() {
                let _ = out.send(ServerMessage::CallResult {
                    id,
                    status: CallStatus::Declined,
                });
                return;
            }
            let rx = state.register_pending(id);
            // Raise the window so the user sees the ring even if it was hidden.
            reveal_main(&app);
            emit(&app, FrontendRequest::IncomingCall {
                id,
                reason,
                timeout_sec,
            });
            let dur = Duration::from_secs(timeout_sec.unwrap_or(DEFAULT_CALL_TIMEOUT_SEC));
            let status = match timeout(dur, rx).await {
                Ok(Ok(FrontendResponse::Call(s))) => s,
                Ok(_) => CallStatus::Declined,
                Err(_) => {
                    state.take_pending(id);
                    CallStatus::Timeout
                }
            };
            let _ = out.send(ServerMessage::CallResult { id, status });
        }
        ClientMessage::SayAndListen {
            id,
            text,
            listen,
            listen_timeout_sec,
        } => {
            let rx = state.register_pending(id);
            emit(&app, FrontendRequest::SayAndListen {
                id,
                text,
                listen,
                listen_timeout_sec,
            });
            let dur = Duration::from_secs(
                listen_timeout_sec.unwrap_or(DEFAULT_LISTEN_TIMEOUT_SEC) + DEFAULT_LISTEN_TIMEOUT_SEC,
            );
            let (heard, status) = match timeout(dur, rx).await {
                Ok(Ok(FrontendResponse::Listen { heard, status })) => (heard, status),
                Ok(_) => (None, ListenStatus::CallEnded),
                Err(_) => {
                    state.take_pending(id);
                    (None, ListenStatus::NoSpeech)
                }
            };
            let _ = out.send(ServerMessage::ListenResult { id, heard, status });
        }
        ClientMessage::Say { id, text } => {
            let rx = state.register_pending(id);
            emit(&app, FrontendRequest::Say { id, text });
            let status = await_ack(&state, id, rx).await;
            let _ = out.send(ServerMessage::Ack { id, status });
        }
        ClientMessage::Hangup { id, farewell } => {
            let rx = state.register_pending(id);
            emit(&app, FrontendRequest::Hangup { id, farewell });
            let status = await_ack(&state, id, rx).await;
            let _ = out.send(ServerMessage::Ack { id, status });
        }
        // Hello / Ping are handled inline in the read loop.
        ClientMessage::Hello { .. } | ClientMessage::Ping { .. } => {}
    }
}

async fn await_ack(
    state: &SessionManager,
    id: u64,
    rx: tokio::sync::oneshot::Receiver<FrontendResponse>,
) -> AckStatus {
    match timeout(Duration::from_secs(DEFAULT_ACTION_TIMEOUT_SEC), rx).await {
        Ok(Ok(FrontendResponse::Ack(s))) => s,
        Ok(_) => AckStatus::Error,
        Err(_) => {
            state.take_pending(id);
            AckStatus::Error
        }
    }
}

fn emit(app: &AppHandle, req: FrontendRequest) {
    if let Err(e) = app.emit(REQUEST_EVENT, req) {
        eprintln!("voice-call: failed to emit {REQUEST_EVENT}: {e}");
    }
}

/// Show, unminimize, and focus the main window (raise-on-ring).
fn reveal_main(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.unminimize();
        let _ = win.show();
        let _ = win.set_focus();
    }
}
