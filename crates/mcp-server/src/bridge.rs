//! WebSocket client bridge from the MCP server to the desktop app.
//!
//! Maintains a single persistent connection for the lifetime of a call so the
//! app can correlate multiple turns and push spontaneous events (user hangup).
//! Reads `~/.copilot/voice-call/runtime.json` for the port + token; any failure
//! to connect is surfaced to callers so tools can fall back (e.g. `no_device`).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::{mpsc, oneshot, Mutex as AsyncMutex};
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::Message;
use voice_protocol::{
    ClientMessage, RuntimeInfo, ServerMessage, PROTOCOL_VERSION, RUNTIME_REL_PATH,
};

type PendingMap = Arc<StdMutex<HashMap<u64, oneshot::Sender<ServerMessage>>>>;

/// Handles for one live connection.
struct Conn {
    out_tx: mpsc::UnboundedSender<ClientMessage>,
    pending: PendingMap,
    hung_up: Arc<AtomicBool>,
    alive: Arc<AtomicBool>,
}

/// Persistent bridge to the desktop app.
pub struct Bridge {
    conn: AsyncMutex<Option<Conn>>,
    next_id: AtomicU64,
    session: String,
}

impl Bridge {
    pub fn new(session: String) -> Self {
        Self {
            conn: AsyncMutex::new(None),
            next_id: AtomicU64::new(1),
            session,
        }
    }

    fn next_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }

    /// Whether the user has hung up on the current connection.
    pub async fn hung_up(&self) -> bool {
        match &*self.conn.lock().await {
            Some(c) => c.hung_up.load(Ordering::SeqCst),
            None => false,
        }
    }

    /// Ensure a live connection exists, (re)connecting if necessary.
    ///
    /// A first connection is attempted once so a truly-down app yields
    /// `no_device` promptly. A *reconnection* (a previously-live socket that
    /// dropped, e.g. the app restarting mid-call) is retried with a short
    /// bounded backoff to ride out the gap.
    async fn ensure(&self) -> Result<()> {
        let mut guard = self.conn.lock().await;
        let reconnecting = match guard.as_ref() {
            Some(c) if c.alive.load(Ordering::SeqCst) => return Ok(()),
            Some(_) => true,
            None => false,
        };

        if !reconnecting {
            let conn = connect(&self.session).await?;
            *guard = Some(conn);
            return Ok(());
        }

        let backoffs = [
            Duration::from_millis(0),
            Duration::from_millis(150),
            Duration::from_millis(350),
        ];
        let mut last_err = anyhow!("could not reconnect to desktop app");
        for wait in backoffs {
            if !wait.is_zero() {
                tokio::time::sleep(wait).await;
            }
            match connect(&self.session).await {
                Ok(conn) => {
                    *guard = Some(conn);
                    return Ok(());
                }
                Err(e) => last_err = e,
            }
        }
        Err(last_err)
    }

    /// Send a request (built with the allocated id) and await the correlated reply.
    pub async fn request<F>(&self, make: F, wait: Duration) -> Result<ServerMessage>
    where
        F: FnOnce(u64) -> ClientMessage,
    {
        self.ensure().await?;
        let id = self.next_id();
        let msg = make(id);

        let (out_tx, pending) = {
            let guard = self.conn.lock().await;
            let c = guard.as_ref().ok_or_else(|| anyhow!("not connected"))?;
            (c.out_tx.clone(), c.pending.clone())
        };

        let (tx, rx) = oneshot::channel();
        pending.lock().unwrap().insert(id, tx);
        out_tx
            .send(msg)
            .map_err(|_| anyhow!("connection closed while sending"))?;

        match timeout(wait, rx).await {
            Ok(Ok(reply)) => Ok(reply),
            Ok(Err(_)) => Err(anyhow!("connection dropped while awaiting reply")),
            Err(_) => {
                pending.lock().unwrap().remove(&id);
                Err(anyhow!("timed out awaiting reply"))
            }
        }
    }

    /// Drop the current connection (closes the socket).
    pub async fn disconnect(&self) {
        *self.conn.lock().await = None;
    }
}

/// Read the runtime-discovery file written by the app.
fn read_runtime() -> Result<RuntimeInfo> {
    let home = dirs::home_dir().ok_or_else(|| anyhow!("could not determine home directory"))?;
    let path = home.join(RUNTIME_REL_PATH);
    let bytes = std::fs::read(&path)
        .with_context(|| format!("reading {} (is the app running?)", path.display()))?;
    Ok(serde_json::from_slice(&bytes)?)
}

/// Establish a connection: TCP connect, Hello/Welcome handshake, spawn I/O tasks.
async fn connect(session: &str) -> Result<Conn> {
    let info = read_runtime()?;
    let url = format!("ws://127.0.0.1:{}", info.port);
    let (ws, _resp) = tokio_tungstenite::connect_async(&url)
        .await
        .with_context(|| format!("connecting to {url}"))?;
    let (mut write, mut read) = ws.split();

    // Handshake.
    let hello = ClientMessage::Hello {
        token: info.token.clone(),
        protocol_version: PROTOCOL_VERSION,
        session: Some(session.to_string()),
    };
    write
        .send(Message::Text(serde_json::to_string(&hello)?.into()))
        .await?;

    let first = timeout(Duration::from_secs(5), read.next())
        .await
        .map_err(|_| anyhow!("handshake timed out"))?
        .ok_or_else(|| anyhow!("connection closed during handshake"))??;
    match serde_json::from_str::<ServerMessage>(first.to_text()?)? {
        ServerMessage::Welcome { .. } => {}
        ServerMessage::Error { message, .. } => {
            return Err(anyhow!("handshake rejected: {message}"))
        }
        other => return Err(anyhow!("unexpected handshake reply: {other:?}")),
    }

    let pending: PendingMap = Arc::new(StdMutex::new(HashMap::new()));
    let hung_up = Arc::new(AtomicBool::new(false));
    let alive = Arc::new(AtomicBool::new(true));

    // Writer task.
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<ClientMessage>();
    let alive_w = alive.clone();
    tokio::spawn(async move {
        while let Some(msg) = out_rx.recv().await {
            let text = match serde_json::to_string(&msg) {
                Ok(t) => t,
                Err(_) => continue,
            };
            if write.send(Message::Text(text.into())).await.is_err() {
                break;
            }
        }
        let _ = write.close().await;
        alive_w.store(false, Ordering::SeqCst);
    });

    // Reader task.
    let pending_r = pending.clone();
    let hung_up_r = hung_up.clone();
    let alive_r = alive.clone();
    tokio::spawn(async move {
        while let Some(next) = read.next().await {
            let msg = match next {
                Ok(m) => m,
                Err(_) => break,
            };
            if msg.is_close() {
                break;
            }
            if !msg.is_text() {
                continue;
            }
            let text = match msg.to_text() {
                Ok(t) => t,
                Err(_) => continue,
            };
            let sm: ServerMessage = match serde_json::from_str(text) {
                Ok(v) => v,
                Err(_) => continue,
            };
            if let ServerMessage::UserHungUp = sm {
                hung_up_r.store(true, Ordering::SeqCst);
                continue;
            }
            if let Some(id) = reply_id(&sm) {
                if let Some(tx) = pending_r.lock().unwrap().remove(&id) {
                    let _ = tx.send(sm);
                }
            }
        }
        alive_r.store(false, Ordering::SeqCst);
        pending_r.lock().unwrap().clear();
    });

    Ok(Conn {
        out_tx,
        pending,
        hung_up,
        alive,
    })
}

/// Extract the correlation id from a reply message, if it carries one.
fn reply_id(m: &ServerMessage) -> Option<u64> {
    match m {
        ServerMessage::Pong { id } => Some(*id),
        ServerMessage::CallResult { id, .. } => Some(*id),
        ServerMessage::ListenResult { id, .. } => Some(*id),
        ServerMessage::Ack { id, .. } => Some(*id),
        ServerMessage::Error { id, .. } => *id,
        ServerMessage::Welcome { .. } | ServerMessage::UserHungUp => None,
    }
}
