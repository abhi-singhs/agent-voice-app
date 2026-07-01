//! In-process state that bridges the localhost WebSocket server to the webview.
//!
//! The app is the always-on WS *server*. When a request that needs the user
//! (ring, speak, listen, hang up) arrives from the MCP client, the WS task
//! registers a pending [`oneshot`] keyed by request id, emits a Tauri event to
//! the webview, and awaits the webview's reply (delivered via a Tauri command).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use serde::Serialize;
use tokio::sync::{mpsc, oneshot};
use voice_protocol::{AckStatus, CallStatus, ListenStatus, ServerMessage};

/// Tauri event name the webview subscribes to for incoming voice requests.
pub const REQUEST_EVENT: &str = "voice://request";

/// A request forwarded from the WS client to the webview.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FrontendRequest {
    IncomingCall {
        id: u64,
        reason: Option<String>,
        timeout_sec: Option<u64>,
    },
    SayAndListen {
        id: u64,
        text: String,
        listen: bool,
        listen_timeout_sec: Option<u64>,
    },
    Say {
        id: u64,
        text: String,
    },
    Hangup {
        id: u64,
        farewell: Option<String>,
    },
}

/// The reply the webview provides for a pending [`FrontendRequest`].
#[derive(Debug)]
pub enum FrontendResponse {
    Call(CallStatus),
    Listen {
        heard: Option<String>,
        status: ListenStatus,
    },
    Ack(AckStatus),
}

#[derive(Default)]
struct Inner {
    /// Outbound channel to the currently connected client, if any.
    outbound: Option<mpsc::UnboundedSender<ServerMessage>>,
    /// Pending webview replies keyed by request id.
    pending: HashMap<u64, oneshot::Sender<FrontendResponse>>,
}

/// Shared, thread-safe session state managed by Tauri.
pub struct SessionManager {
    token: String,
    /// When true, incoming calls are auto-declined without ringing.
    dnd: AtomicBool,
    inner: Mutex<Inner>,
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            token: generate_token(),
            dnd: AtomicBool::new(false),
            inner: Mutex::new(Inner::default()),
        }
    }

    /// The shared secret a client must present in `Hello`.
    pub fn token(&self) -> &str {
        &self.token
    }

    /// Enable or disable do-not-disturb (auto-decline incoming calls).
    pub fn set_dnd(&self, on: bool) {
        self.dnd.store(on, Ordering::Relaxed);
    }

    /// Whether do-not-disturb is currently enabled.
    pub fn dnd(&self) -> bool {
        self.dnd.load(Ordering::Relaxed)
    }

    /// Set (or clear) the outbound channel for the active connection.
    pub fn set_outbound(&self, tx: Option<mpsc::UnboundedSender<ServerMessage>>) {
        self.inner.lock().unwrap().outbound = tx;
    }

    /// Push a message to the connected client. Returns `false` if none connected.
    pub fn send_to_client(&self, msg: ServerMessage) -> bool {
        match &self.inner.lock().unwrap().outbound {
            Some(tx) => tx.send(msg).is_ok(),
            None => false,
        }
    }

    /// Register a pending request and return the receiver the WS task awaits.
    pub fn register_pending(&self, id: u64) -> oneshot::Receiver<FrontendResponse> {
        let (tx, rx) = oneshot::channel();
        self.inner.lock().unwrap().pending.insert(id, tx);
        rx
    }

    /// Remove a pending request (e.g. on timeout) without completing it.
    pub fn take_pending(&self, id: u64) -> Option<oneshot::Sender<FrontendResponse>> {
        self.inner.lock().unwrap().pending.remove(&id)
    }

    /// Complete a pending request with the webview's reply.
    /// Returns `false` if no such request is pending.
    pub fn complete_pending(&self, id: u64, resp: FrontendResponse) -> bool {
        match self.take_pending(id) {
            Some(tx) => tx.send(resp).is_ok(),
            None => false,
        }
    }

    /// Drop all pending requests (their receivers resolve as cancelled).
    pub fn clear_pending(&self) {
        self.inner.lock().unwrap().pending.clear();
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Generate a 48-character hex token from 24 cryptographically-random bytes.
fn generate_token() -> String {
    let mut bytes = [0u8; 24];
    getrandom::fill(&mut bytes).expect("failed to read from the system RNG");
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}
