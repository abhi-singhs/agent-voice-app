//! Agent Voice App MCP server.
//!
//! Spawned by an MCP-capable agent client over stdio. Exposes voice tools and bridges them
//! to the always-on desktop app over a localhost WebSocket. If no app is
//! reachable, tools return a `no_device` / `call_ended` status so the agent can
//! gracefully fall back to text.

mod bridge;

use std::sync::Arc;
use std::time::Duration;

use bridge::Bridge;
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{Implementation, ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router,
    transport::stdio,
    ServerHandler, ServiceExt,
};
use serde::Deserialize;
use serde_json::json;
use voice_protocol::{ClientMessage, ServerMessage};

fn default_true() -> bool {
    true
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
struct CallUserParams {
    /// Why you're calling; spoken/shown to the user when the phone rings.
    #[serde(default)]
    reason: Option<String>,
    /// How long to ring before giving up, in seconds (default 30).
    #[serde(default)]
    timeout_sec: Option<u64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
struct SayAndListenParams {
    /// What to say to the user (spoken via text-to-speech).
    text: String,
    /// Whether to listen for a spoken reply after speaking (default true).
    #[serde(default = "default_true")]
    listen: bool,
    /// Max seconds to wait for the user's spoken reply (default 20).
    #[serde(default)]
    listen_timeout_sec: Option<u64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
struct VoiceSayParams {
    /// The announcement to speak to the user (no reply is captured).
    text: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
struct EndCallParams {
    /// Optional farewell to speak before hanging up.
    #[serde(default)]
    farewell: Option<String>,
}

#[derive(Clone)]
struct VoiceServer {
    bridge: Arc<Bridge>,
    tool_router: ToolRouter<Self>,
}

impl VoiceServer {
    fn new() -> Self {
        let session = format!("pid-{}", std::process::id());
        Self {
            bridge: Arc::new(Bridge::new(session)),
            tool_router: Self::tool_router(),
        }
    }
}

#[tool_router]
impl VoiceServer {
    #[tool(
        name = "call_user",
        description = "Ring the user's Agent Voice App desktop app to start a voice call. \
Returns { status } where status is \"answered\", \"declined\", \"timeout\", or \
\"no_device\" (no app reachable — fall back to text)."
    )]
    async fn call_user(&self, Parameters(p): Parameters<CallUserParams>) -> String {
        let wait = Duration::from_secs(p.timeout_sec.unwrap_or(30) + 10);
        match self
            .bridge
            .request(
                move |id| ClientMessage::IncomingCall {
                    id,
                    reason: p.reason,
                    timeout_sec: p.timeout_sec,
                },
                wait,
            )
            .await
        {
            Ok(ServerMessage::CallResult { status, .. }) => json!({ "status": status }).to_string(),
            _ => json!({ "status": "no_device" }).to_string(),
        }
    }

    #[tool(
        name = "say_and_listen",
        description = "Speak text to the user and (by default) capture their spoken reply. \
Returns { status, heard } where status is \"ok\", \"no_speech\", or \"call_ended\" \
and heard is the transcript (or null)."
    )]
    async fn say_and_listen(&self, Parameters(p): Parameters<SayAndListenParams>) -> String {
        if self.bridge.hung_up().await {
            return json!({ "status": "call_ended", "heard": null }).to_string();
        }
        // Allow room for TTS playback + the listen window + STT.
        let wait = Duration::from_secs(p.listen_timeout_sec.unwrap_or(20) + 60);
        match self
            .bridge
            .request(
                move |id| ClientMessage::SayAndListen {
                    id,
                    text: p.text,
                    listen: p.listen,
                    listen_timeout_sec: p.listen_timeout_sec,
                },
                wait,
            )
            .await
        {
            Ok(ServerMessage::ListenResult { heard, status, .. }) => {
                json!({ "status": status, "heard": heard }).to_string()
            }
            _ => json!({ "status": "call_ended", "heard": null }).to_string(),
        }
    }

    #[tool(
        name = "voice_say",
        description = "Speak a one-way announcement to the user (no reply captured). \
Returns { status } where status is \"ok\" or \"call_ended\"."
    )]
    async fn voice_say(&self, Parameters(p): Parameters<VoiceSayParams>) -> String {
        if self.bridge.hung_up().await {
            return json!({ "status": "call_ended" }).to_string();
        }
        match self
            .bridge
            .request(
                move |id| ClientMessage::Say { id, text: p.text },
                Duration::from_secs(120),
            )
            .await
        {
            Ok(ServerMessage::Ack { status, .. }) => json!({ "status": status }).to_string(),
            _ => json!({ "status": "call_ended" }).to_string(),
        }
    }

    #[tool(
        name = "end_call",
        description = "End the current voice call, optionally speaking a farewell first. \
Returns { status: \"ok\" }."
    )]
    async fn end_call(&self, Parameters(p): Parameters<EndCallParams>) -> String {
        let result = self
            .bridge
            .request(
                move |id| ClientMessage::Hangup {
                    id,
                    farewell: p.farewell,
                },
                Duration::from_secs(60),
            )
            .await;
        self.bridge.disconnect().await;
        match result {
            Ok(ServerMessage::Ack { status, .. }) => json!({ "status": status }).to_string(),
            _ => json!({ "status": "ok" }).to_string(),
        }
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for VoiceServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(
                Implementation::new(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"))
                    .with_title("Agent Voice App"),
            )
            .with_instructions(
                "Voice calling for the user via the Agent Voice App desktop app. \
Call `call_user` to start a call; if it returns \"answered\", use `say_and_listen` for \
back-and-forth conversation, `voice_say` for one-way announcements, and `end_call` to hang up. \
If `call_user` returns \"no_device\", the app isn't running — continue in text.",
            )
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // stdout is reserved for the MCP protocol; diagnostics go to stderr.
    let service = VoiceServer::new().serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
