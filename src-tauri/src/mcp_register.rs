//! Self-registration of the bundled MCP server in supported MCP clients.
//!
//! The desktop app can register (or remove) the bundled MCP server binary so an
//! agent client can call the voice tools. Different MCP hosts use different
//! config files and schemas, so this module keeps the client registry and
//! format-specific upsert logic in one place.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use serde::Serialize;
use serde_json::{json, Value};
use toml_edit::{value as toml_value, Array, DocumentMut, Item, Table, Value as TomlValue};

/// Key under the client's server map and the base name of the MCP server binary.
const SERVER_KEY: &str = "agent-voice-mcp";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClientKind {
    CopilotJson,
    ClaudeCodeJson,
    CodexToml,
    OpenCodeJson,
}

#[derive(Debug, Clone, Copy)]
struct McpClient {
    id: &'static str,
    name: &'static str,
    config_rel: &'static str,
    description: &'static str,
    restart_hint: &'static str,
    kind: ClientKind,
}

const CLIENTS: &[McpClient] = &[
    McpClient {
        id: "copilot",
        name: "Copilot CLI",
        config_rel: ".copilot/mcp-config.json",
        description: "Writes a stdio server to mcpServers in Copilot CLI's MCP config.",
        restart_hint: "Restart your Copilot CLI session so it picks up the new server.",
        kind: ClientKind::CopilotJson,
    },
    McpClient {
        id: "claude-code",
        name: "Claude Code",
        config_rel: ".claude.json",
        description: "Writes a user-scoped stdio server to Claude Code's MCP config.",
        restart_hint: "Restart Claude Code, or run /mcp, so it picks up the new server.",
        kind: ClientKind::ClaudeCodeJson,
    },
    McpClient {
        id: "codex",
        name: "Codex CLI",
        config_rel: ".codex/config.toml",
        description: "Writes a stdio server to Codex's mcp_servers TOML table.",
        restart_hint: "Restart your Codex session so it picks up the new server.",
        kind: ClientKind::CodexToml,
    },
    McpClient {
        id: "opencode",
        name: "OpenCode",
        config_rel: ".config/opencode/opencode.json",
        description: "Writes a local server to OpenCode's mcp config map.",
        restart_hint: "Restart OpenCode so it picks up the new server.",
        kind: ClientKind::OpenCodeJson,
    },
];

/// Client option reported to the UI.
#[derive(Debug, Clone, Serialize)]
pub struct McpClientInfo {
    pub id: String,
    pub name: String,
    pub config_path: String,
    pub description: String,
    pub restart_hint: String,
}

/// Registration status reported to the UI.
#[derive(Debug, Clone, Serialize)]
pub struct McpStatus {
    /// Selected client identifier.
    pub client_id: String,
    /// Human-readable selected client name.
    pub client_name: String,
    /// True when a `agent-voice-mcp` entry exists in the selected client config.
    pub registered: bool,
    /// True when the registered entry matches the current binary path and shape.
    pub up_to_date: bool,
    /// Absolute path to the selected client's MCP config file.
    pub config_path: String,
    /// Resolved absolute path to the MCP server binary (what we would register).
    pub server_path: String,
    /// True when the resolved MCP server binary actually exists on disk.
    pub server_exists: bool,
    /// Client-specific restart/reload guidance.
    pub restart_hint: String,
}

#[derive(Debug, Clone, Copy)]
struct RegistrationState {
    registered: bool,
    up_to_date: bool,
}

/// Supported clients with resolved config paths.
pub fn clients() -> Result<Vec<McpClientInfo>> {
    CLIENTS
        .iter()
        .map(|client| {
            Ok(McpClientInfo {
                id: client.id.to_string(),
                name: client.name.to_string(),
                config_path: config_path(client)?.to_string_lossy().to_string(),
                description: client.description.to_string(),
                restart_hint: client.restart_hint.to_string(),
            })
        })
        .collect()
}

fn client_by_id(client_id: Option<&str>) -> Result<&'static McpClient> {
    let Some(raw) = client_id else {
        return Ok(&CLIENTS[0]);
    };
    let normalized = raw.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return Ok(&CLIENTS[0]);
    }
    let id = match normalized.as_str() {
        "copilot" | "copilot-cli" => "copilot",
        "claude" | "claude-code" | "claude_code" => "claude-code",
        "codex" | "codex-cli" => "codex",
        "opencode" | "open-code" | "open_code" => "opencode",
        other => other,
    };
    CLIENTS
        .iter()
        .find(|client| client.id == id)
        .ok_or_else(|| anyhow!("unsupported MCP client: {raw}"))
}

/// Absolute path to a client's MCP config, relative to the home directory.
fn config_path(client: &McpClient) -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| anyhow!("could not determine home directory"))?;
    Ok(home.join(client.config_rel))
}

/// Resolve the MCP server binary path: a sibling of the running app executable.
fn resolve_server_bin() -> Result<PathBuf> {
    let exe = std::env::current_exe().context("resolving current executable")?;
    let dir = exe
        .parent()
        .ok_or_else(|| anyhow!("executable has no parent directory"))?;

    // Preferred: the plain sibling name (dev build + bundled sidecar).
    let plain = dir.join(SERVER_KEY);
    if plain.exists() {
        return Ok(plain);
    }

    // Fallback: a target-triple-suffixed sidecar that wasn't renamed
    // (e.g. agent-voice-mcp-aarch64-apple-darwin). Pick the first match.
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let base = name.strip_suffix(".exe").unwrap_or(&name);
            if base == SERVER_KEY || base.starts_with(&format!("{SERVER_KEY}-")) {
                return Ok(entry.path());
            }
        }
    }

    // Nothing found; return the expected path so callers can report it as missing.
    Ok(plain)
}

fn read_text(path: &Path) -> Result<String> {
    match fs::read_to_string(path) {
        Ok(raw) => Ok(raw),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
    }
}

/// Parse JSON config bytes into a root object, tolerating an empty/missing file.
fn parse_json_config(raw: &str, path: &Path) -> Result<Value> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(json!({}));
    }
    let value: Value =
        serde_json::from_str(trimmed).with_context(|| format!("parsing {}", path.display()))?;
    if !value.is_object() {
        return Err(anyhow!("{} is not a JSON object", path.display()));
    }
    Ok(value)
}

fn read_json_config(path: &Path) -> Result<Value> {
    let raw = read_text(path)?;
    parse_json_config(&raw, path)
}

fn parse_toml_config(raw: &str, path: &Path) -> Result<DocumentMut> {
    raw.parse::<DocumentMut>()
        .with_context(|| format!("parsing {}", path.display()))
}

fn read_toml_config(path: &Path) -> Result<DocumentMut> {
    let raw = read_text(path)?;
    parse_toml_config(&raw, path)
}

/// Write JSON config as pretty JSON, creating the parent directory as needed.
fn write_json_config(path: &Path, root: &Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    let mut json = serde_json::to_string_pretty(root)?;
    json.push('\n');
    fs::write(path, json).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

/// Write TOML config, creating the parent directory as needed.
fn write_toml_config(path: &Path, doc: &DocumentMut) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    fs::write(path, doc.to_string()).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

fn json_server_key(client: &McpClient) -> &'static str {
    match client.kind {
        ClientKind::OpenCodeJson => "mcp",
        ClientKind::CopilotJson | ClientKind::ClaudeCodeJson => "mcpServers",
        ClientKind::CodexToml => unreachable!("codex uses TOML"),
    }
}

/// The server entry shape expected by a JSON-based client.
fn json_server_entry(client: &McpClient, command: &str) -> Value {
    match client.kind {
        ClientKind::CopilotJson => json!({
            "type": "stdio",
            "command": command,
            "args": [],
            "tools": ["*"],
        }),
        ClientKind::ClaudeCodeJson => json!({
            "type": "stdio",
            "command": command,
            "args": [],
        }),
        ClientKind::OpenCodeJson => json!({
            "type": "local",
            "command": [command],
            "enabled": true,
        }),
        ClientKind::CodexToml => unreachable!("codex uses TOML"),
    }
}

fn ensure_json_server_map<'a>(
    root: &'a mut Value,
    client: &McpClient,
) -> &'a mut serde_json::Map<String, Value> {
    let obj = root.as_object_mut().expect("config root is an object");
    let servers = obj
        .entry(json_server_key(client))
        .or_insert_with(|| json!({}));
    if !servers.is_object() {
        *servers = json!({});
    }
    servers.as_object_mut().unwrap()
}

fn json_entry<'a>(root: &'a Value, client: &McpClient) -> Option<&'a Value> {
    root.get(json_server_key(client))?.get(SERVER_KEY)
}

fn json_entry_matches(root: &Value, client: &McpClient, command: &str) -> bool {
    json_entry(root, client) == Some(&json_server_entry(client, command))
}

/// Upsert our server entry into a JSON client config, preserving other servers.
/// Returns true when the resulting entry differs from what was already there.
fn apply_json_register(root: &mut Value, client: &McpClient, command: &str) -> bool {
    let desired = json_server_entry(client, command);
    let changed = json_entry(root, client) != Some(&desired);
    ensure_json_server_map(root, client).insert(SERVER_KEY.to_string(), desired);
    changed
}

/// Remove our server entry from a JSON client config. Returns true if it existed.
fn apply_json_unregister(root: &mut Value, client: &McpClient) -> bool {
    let Some(obj) = root.as_object_mut() else {
        return false;
    };
    let Some(servers) = obj
        .get_mut(json_server_key(client))
        .and_then(Value::as_object_mut)
    else {
        return false;
    };
    servers.remove(SERVER_KEY).is_some()
}

fn toml_servers_table_mut(doc: &mut DocumentMut) -> &mut Table {
    if !doc.as_table().contains_key("mcp_servers") {
        doc.as_table_mut()
            .insert("mcp_servers", Item::Table(Table::new()));
    }
    if !doc["mcp_servers"].is_table() {
        doc["mcp_servers"] = Item::Table(Table::new());
    }
    doc["mcp_servers"].as_table_mut().unwrap()
}

fn codex_toml_entry(command: &str) -> Table {
    let mut table = Table::new();
    table["command"] = toml_value(command);
    table["args"] = Item::Value(TomlValue::Array(Array::new()));
    table
}

fn toml_server_table<'a>(doc: &'a DocumentMut) -> Option<&'a Table> {
    doc.get("mcp_servers")?
        .as_table()?
        .get(SERVER_KEY)?
        .as_table()
}

fn toml_entry_matches(doc: &DocumentMut, command: &str) -> bool {
    let Some(table) = toml_server_table(doc) else {
        return false;
    };
    let command_matches = table.get("command").and_then(Item::as_str) == Some(command);
    let args_empty = table
        .get("args")
        .and_then(Item::as_array)
        .is_some_and(|args| args.is_empty());
    command_matches && args_empty
}

/// Upsert our server entry into Codex's TOML config, preserving other servers.
/// Returns true when the resulting entry differs from what was already there.
fn apply_toml_register(doc: &mut DocumentMut, command: &str) -> bool {
    let changed = !toml_entry_matches(doc, command);
    toml_servers_table_mut(doc).insert(SERVER_KEY, Item::Table(codex_toml_entry(command)));
    changed
}

/// Remove our server entry from Codex's TOML config. Returns true if it existed.
fn apply_toml_unregister(doc: &mut DocumentMut) -> bool {
    let Some(servers) = doc.get_mut("mcp_servers").and_then(Item::as_table_mut) else {
        return false;
    };
    servers.remove(SERVER_KEY).is_some()
}

fn registration_state(client: &McpClient, config_path: &Path, command: &str) -> Result<RegistrationState> {
    match client.kind {
        ClientKind::CodexToml => {
            let doc = read_toml_config(config_path)?;
            Ok(RegistrationState {
                registered: toml_server_table(&doc).is_some(),
                up_to_date: toml_entry_matches(&doc, command),
            })
        }
        ClientKind::CopilotJson | ClientKind::ClaudeCodeJson | ClientKind::OpenCodeJson => {
            let root = read_json_config(config_path)?;
            Ok(RegistrationState {
                registered: json_entry(&root, client).is_some(),
                up_to_date: json_entry_matches(&root, client, command),
            })
        }
    }
}

/// Build a status snapshot from a parsed config + resolved binary path.
fn status_from(
    client: &McpClient,
    state: RegistrationState,
    config_path: &Path,
    server_bin: &Path,
) -> McpStatus {
    McpStatus {
        client_id: client.id.to_string(),
        client_name: client.name.to_string(),
        registered: state.registered,
        up_to_date: state.up_to_date,
        config_path: config_path.to_string_lossy().to_string(),
        server_path: server_bin.to_string_lossy().to_string(),
        server_exists: server_bin.exists(),
        restart_hint: client.restart_hint.to_string(),
    }
}

/// Current registration status for a client (does not modify anything).
pub fn status(client_id: Option<String>) -> Result<McpStatus> {
    let client = client_by_id(client_id.as_deref())?;
    let cfg_path = config_path(client)?;
    let server_bin = resolve_server_bin()?;
    let server_path = server_bin.to_string_lossy().to_string();
    let state = registration_state(client, &cfg_path, &server_path)?;
    Ok(status_from(client, state, &cfg_path, &server_bin))
}

/// Register (or update) the MCP server for a client, then return the new status.
pub fn register(client_id: Option<String>) -> Result<McpStatus> {
    let client = client_by_id(client_id.as_deref())?;
    let cfg_path = config_path(client)?;
    let server_bin = resolve_server_bin()?;
    if !server_bin.exists() {
        return Err(anyhow!(
            "MCP server binary not found at {}",
            server_bin.display()
        ));
    }
    let server_path = server_bin.to_string_lossy().to_string();

    match client.kind {
        ClientKind::CodexToml => {
            let mut doc = read_toml_config(&cfg_path)?;
            apply_toml_register(&mut doc, &server_path);
            write_toml_config(&cfg_path, &doc)?;
        }
        ClientKind::CopilotJson | ClientKind::ClaudeCodeJson | ClientKind::OpenCodeJson => {
            let mut root = read_json_config(&cfg_path)?;
            apply_json_register(&mut root, client, &server_path);
            write_json_config(&cfg_path, &root)?;
        }
    }

    let state = registration_state(client, &cfg_path, &server_path)?;
    Ok(status_from(client, state, &cfg_path, &server_bin))
}

/// Remove the MCP server entry for a client, then return the new status.
pub fn unregister(client_id: Option<String>) -> Result<McpStatus> {
    let client = client_by_id(client_id.as_deref())?;
    let cfg_path = config_path(client)?;
    let server_bin = resolve_server_bin()?;
    let server_path = server_bin.to_string_lossy().to_string();

    match client.kind {
        ClientKind::CodexToml => {
            let mut doc = read_toml_config(&cfg_path)?;
            if apply_toml_unregister(&mut doc) {
                write_toml_config(&cfg_path, &doc)?;
            }
        }
        ClientKind::CopilotJson | ClientKind::ClaudeCodeJson | ClientKind::OpenCodeJson => {
            let mut root = read_json_config(&cfg_path)?;
            if apply_json_unregister(&mut root, client) {
                write_json_config(&cfg_path, &root)?;
            }
        }
    }

    let state = registration_state(client, &cfg_path, &server_path)?;
    Ok(status_from(client, state, &cfg_path, &server_bin))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client(id: &str) -> &'static McpClient {
        client_by_id(Some(id)).unwrap()
    }

    #[test]
    fn client_lookup_defaults_and_accepts_aliases() {
        assert_eq!(client_by_id(None).unwrap().id, "copilot");
        assert_eq!(client_by_id(Some("")).unwrap().id, "copilot");
        assert_eq!(client_by_id(Some("claude")).unwrap().id, "claude-code");
        assert_eq!(client_by_id(Some("open-code")).unwrap().id, "opencode");
        assert!(client_by_id(Some("unknown")).is_err());
    }

    #[test]
    fn parse_empty_json_yields_object() {
        let path = Path::new("/tmp/mcp-config.json");
        assert_eq!(parse_json_config("", path).unwrap(), json!({}));
        assert_eq!(parse_json_config("   \n", path).unwrap(), json!({}));
    }

    #[test]
    fn parse_json_rejects_non_object() {
        let path = Path::new("/tmp/mcp-config.json");
        assert!(parse_json_config("[1,2,3]", path).is_err());
        assert!(parse_json_config("\"nope\"", path).is_err());
    }

    #[test]
    fn copilot_register_creates_entry_and_preserves_others() {
        let copilot = client("copilot");
        let mut root = json!({
            "mcpServers": {
                "playwright": { "type": "stdio", "command": "pw", "args": [], "tools": ["*"] }
            }
        });
        let changed = apply_json_register(&mut root, copilot, "/path/to/agent-voice-mcp");
        assert!(changed);
        assert_eq!(root["mcpServers"]["playwright"]["command"], json!("pw"));
        let entry = &root["mcpServers"][SERVER_KEY];
        assert_eq!(entry["type"], json!("stdio"));
        assert_eq!(entry["command"], json!("/path/to/agent-voice-mcp"));
        assert_eq!(entry["args"], json!([]));
        assert_eq!(entry["tools"], json!(["*"]));
    }

    #[test]
    fn claude_code_register_uses_mcp_servers_without_tools_field() {
        let claude = client("claude-code");
        let mut root = json!({});
        assert!(apply_json_register(&mut root, claude, "/bin/voice"));
        assert_eq!(
            root["mcpServers"][SERVER_KEY],
            json!({ "type": "stdio", "command": "/bin/voice", "args": [] })
        );
    }

    #[test]
    fn opencode_register_uses_local_mcp_shape() {
        let opencode = client("opencode");
        let mut root = json!({ "mcp": { "keep": { "type": "remote", "url": "https://x" } } });
        assert!(apply_json_register(&mut root, opencode, "/bin/voice"));
        assert_eq!(
            root["mcp"][SERVER_KEY],
            json!({ "type": "local", "command": ["/bin/voice"], "enabled": true })
        );
        assert_eq!(root["mcp"]["keep"]["url"], json!("https://x"));
    }

    #[test]
    fn json_register_from_empty_creates_server_map() {
        let mut root = json!({});
        assert!(apply_json_register(&mut root, client("copilot"), "/bin/x"));
        assert!(root["mcpServers"][SERVER_KEY].is_object());
    }

    #[test]
    fn json_register_is_idempotent() {
        let copilot = client("copilot");
        let mut root = json!({});
        assert!(apply_json_register(&mut root, copilot, "/bin/x"));
        assert!(!apply_json_register(&mut root, copilot, "/bin/x"));
        assert!(apply_json_register(&mut root, copilot, "/bin/y"));
    }

    #[test]
    fn json_unregister_removes_only_our_entry() {
        let copilot = client("copilot");
        let mut root = json!({
            "mcpServers": {
                "playwright": { "command": "pw" },
                SERVER_KEY: { "command": "x" }
            }
        });
        assert!(apply_json_unregister(&mut root, copilot));
        assert!(root["mcpServers"].get(SERVER_KEY).is_none());
        assert!(root["mcpServers"].get("playwright").is_some());
        assert!(!apply_json_unregister(&mut root, copilot));
    }

    #[test]
    fn json_entry_match_requires_expected_shape() {
        let copilot = client("copilot");
        let mut root = json!({});
        apply_json_register(&mut root, copilot, "/bin/z");
        assert!(json_entry_matches(&root, copilot, "/bin/z"));
        assert!(!json_entry_matches(&root, copilot, "/bin/other"));
    }

    #[test]
    fn toml_register_creates_codex_mcp_server() {
        let mut doc = parse_toml_config("model = \"gpt-5.5\"\n", Path::new("/tmp/config.toml")).unwrap();
        assert!(apply_toml_register(&mut doc, "/bin/voice"));
        assert!(toml_entry_matches(&doc, "/bin/voice"));
        let out = doc.to_string();
        assert!(out.contains("model = \"gpt-5.5\""));
        assert!(out.contains("[mcp_servers.agent-voice-mcp]"));
        assert!(out.contains("command = \"/bin/voice\""));
        assert!(out.contains("args = []"));
    }

    #[test]
    fn toml_register_is_idempotent() {
        let mut doc = parse_toml_config("", Path::new("/tmp/config.toml")).unwrap();
        assert!(apply_toml_register(&mut doc, "/bin/x"));
        assert!(!apply_toml_register(&mut doc, "/bin/x"));
        assert!(apply_toml_register(&mut doc, "/bin/y"));
    }

    #[test]
    fn toml_unregister_removes_only_our_entry() {
        let mut doc = parse_toml_config(
            r#"
[mcp_servers.keep]
command = "keep"
args = []

[mcp_servers.agent-voice-mcp]
command = "voice"
args = []
"#,
            Path::new("/tmp/config.toml"),
        )
        .unwrap();
        assert!(apply_toml_unregister(&mut doc));
        assert!(toml_server_table(&doc).is_none());
        assert!(doc["mcp_servers"]["keep"].is_table());
        assert!(!apply_toml_unregister(&mut doc));
    }

    #[test]
    fn status_from_reports_client_details() {
        let copilot = client("copilot");
        let st = status_from(
            copilot,
            RegistrationState {
                registered: true,
                up_to_date: true,
            },
            Path::new("/home/u/.copilot/mcp-config.json"),
            Path::new("/tmp/agent-voice-mcp"),
        );
        assert_eq!(st.client_id, "copilot");
        assert_eq!(st.client_name, "Copilot CLI");
        assert!(st.registered);
        assert!(st.up_to_date);
    }

    #[test]
    fn read_missing_json_config_is_empty_object() {
        let dir = std::env::temp_dir().join(format!("mcpreg-test-{}", std::process::id()));
        let path = dir.join("does-not-exist.json");
        assert_eq!(read_json_config(&path).unwrap(), json!({}));
    }

    #[test]
    fn write_then_read_json_roundtrip() {
        let dir = std::env::temp_dir().join(format!("mcpreg-json-rt-{}", std::process::id()));
        let path = dir.join("nested/mcp-config.json");
        let mut root = json!({ "mcpServers": { "keep": { "command": "k" } } });
        apply_json_register(&mut root, client("copilot"), "/bin/app");
        write_json_config(&path, &root).unwrap();
        let back = read_json_config(&path).unwrap();
        assert!(json_entry_matches(&back, client("copilot"), "/bin/app"));
        assert_eq!(back["mcpServers"]["keep"]["command"], json!("k"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_then_read_toml_roundtrip() {
        let dir = std::env::temp_dir().join(format!("mcpreg-toml-rt-{}", std::process::id()));
        let path = dir.join("nested/config.toml");
        let mut doc = parse_toml_config("model = \"gpt-5.5\"\n", &path).unwrap();
        apply_toml_register(&mut doc, "/bin/app");
        write_toml_config(&path, &doc).unwrap();
        let back = read_toml_config(&path).unwrap();
        assert!(toml_entry_matches(&back, "/bin/app"));
        assert_eq!(back["model"].as_str(), Some("gpt-5.5"));
        let _ = fs::remove_dir_all(&dir);
    }
}
