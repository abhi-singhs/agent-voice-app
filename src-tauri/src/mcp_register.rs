//! Self-registration of the MCP server in the Copilot CLI's `mcp-config.json`.
//!
//! The desktop app can register (or remove) the bundled MCP server binary so the
//! Copilot agent can call the voice tools. This mirrors `scripts/install-mcp.mjs`
//! but runs in-process (no Node needed) and resolves the binary next to the
//! running app executable — which is correct both in `tauri dev`
//! (`target/debug/copilot-voice-mcp` sits beside `copilot-voice-call`) and in a
//! bundled app (Tauri places the `externalBin` sidecar in the same directory as
//! the main binary, stripping the target-triple suffix).

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use serde::Serialize;
use serde_json::{json, Value};

/// Key under `mcpServers` and the base name of the MCP server binary.
const SERVER_KEY: &str = "copilot-voice-mcp";
/// Location of the Copilot CLI MCP config, relative to the home directory.
const MCP_CONFIG_REL: &str = ".copilot/mcp-config.json";

/// Registration status reported to the UI.
#[derive(Debug, Clone, Serialize)]
pub struct McpStatus {
    /// True when a `copilot-voice-mcp` entry exists in the config.
    pub registered: bool,
    /// True when the registered command matches the current binary path.
    pub up_to_date: bool,
    /// Absolute path to the MCP config file.
    pub config_path: String,
    /// Resolved absolute path to the MCP server binary (what we would register).
    pub server_path: String,
    /// True when the resolved MCP server binary actually exists on disk.
    pub server_exists: bool,
}

/// Absolute path to the Copilot CLI MCP config (`~/.copilot/mcp-config.json`).
fn config_path() -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| anyhow!("could not determine home directory"))?;
    Ok(home.join(MCP_CONFIG_REL))
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
    // (e.g. copilot-voice-mcp-aarch64-apple-darwin). Pick the first match.
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

/// Parse config bytes into a root object, tolerating an empty/missing file.
fn parse_config(raw: &str) -> Result<Value> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(json!({}));
    }
    let value: Value = serde_json::from_str(trimmed).context("parsing mcp-config.json")?;
    if !value.is_object() {
        return Err(anyhow!("mcp-config.json is not a JSON object"));
    }
    Ok(value)
}

/// Read + parse the config file, returning `{}` when it does not exist.
fn read_config(path: &Path) -> Result<Value> {
    match fs::read_to_string(path) {
        Ok(raw) => parse_config(&raw),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(json!({})),
        Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
    }
}

/// Write the config as pretty JSON, creating the parent directory as needed.
fn write_config(path: &Path, root: &Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    let mut json = serde_json::to_string_pretty(root)?;
    json.push('\n');
    fs::write(path, json).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

/// The server entry we register (`{ type, command, args, tools }`).
fn server_entry(command: &str) -> Value {
    json!({
        "type": "stdio",
        "command": command,
        "args": [],
        "tools": ["*"],
    })
}

/// Upsert our server entry into `root.mcpServers`, preserving other servers.
/// Returns true when the resulting entry differs from what was already there.
fn apply_register(root: &mut Value, command: &str) -> bool {
    let obj = root.as_object_mut().expect("config root is an object");
    let servers = obj
        .entry("mcpServers")
        .or_insert_with(|| json!({}));
    if !servers.is_object() {
        *servers = json!({});
    }
    let servers = servers.as_object_mut().unwrap();
    let desired = server_entry(command);
    let changed = servers.get(SERVER_KEY) != Some(&desired);
    servers.insert(SERVER_KEY.to_string(), desired);
    changed
}

/// Remove our server entry from `root.mcpServers`. Returns true if it existed.
fn apply_unregister(root: &mut Value) -> bool {
    let Some(obj) = root.as_object_mut() else {
        return false;
    };
    let Some(servers) = obj.get_mut("mcpServers").and_then(Value::as_object_mut) else {
        return false;
    };
    servers.remove(SERVER_KEY).is_some()
}

/// The currently-registered command for our server, if any.
fn registered_command(root: &Value) -> Option<&str> {
    root.get("mcpServers")?
        .get(SERVER_KEY)?
        .get("command")?
        .as_str()
}

/// Build a status snapshot from a parsed config + resolved binary path.
fn status_from(root: &Value, config_path: &Path, server_bin: &Path) -> McpStatus {
    let server_path = server_bin.to_string_lossy().to_string();
    let registered_cmd = registered_command(root);
    McpStatus {
        registered: registered_cmd.is_some(),
        up_to_date: registered_cmd == Some(server_path.as_str()),
        config_path: config_path.to_string_lossy().to_string(),
        server_path,
        server_exists: server_bin.exists(),
    }
}

/// Current registration status (does not modify anything).
pub fn status() -> Result<McpStatus> {
    let cfg_path = config_path()?;
    let server_bin = resolve_server_bin()?;
    let root = read_config(&cfg_path)?;
    Ok(status_from(&root, &cfg_path, &server_bin))
}

/// Register (or update) the MCP server, then return the new status.
pub fn register() -> Result<McpStatus> {
    let cfg_path = config_path()?;
    let server_bin = resolve_server_bin()?;
    if !server_bin.exists() {
        return Err(anyhow!(
            "MCP server binary not found at {}",
            server_bin.display()
        ));
    }
    let mut root = read_config(&cfg_path)?;
    apply_register(&mut root, &server_bin.to_string_lossy());
    write_config(&cfg_path, &root)?;
    Ok(status_from(&root, &cfg_path, &server_bin))
}

/// Remove the MCP server entry, then return the new status.
pub fn unregister() -> Result<McpStatus> {
    let cfg_path = config_path()?;
    let server_bin = resolve_server_bin()?;
    let mut root = read_config(&cfg_path)?;
    if apply_unregister(&mut root) {
        write_config(&cfg_path, &root)?;
    }
    Ok(status_from(&root, &cfg_path, &server_bin))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_empty_yields_object() {
        assert_eq!(parse_config("").unwrap(), json!({}));
        assert_eq!(parse_config("   \n").unwrap(), json!({}));
    }

    #[test]
    fn parse_rejects_non_object() {
        assert!(parse_config("[1,2,3]").is_err());
        assert!(parse_config("\"nope\"").is_err());
    }

    #[test]
    fn register_creates_entry_and_preserves_others() {
        let mut root = json!({
            "mcpServers": {
                "playwright": { "type": "stdio", "command": "pw", "args": [], "tools": ["*"] }
            }
        });
        let changed = apply_register(&mut root, "/path/to/copilot-voice-mcp");
        assert!(changed);
        // Existing server preserved.
        assert_eq!(root["mcpServers"]["playwright"]["command"], json!("pw"));
        // New server added with the expected shape.
        let entry = &root["mcpServers"][SERVER_KEY];
        assert_eq!(entry["type"], json!("stdio"));
        assert_eq!(entry["command"], json!("/path/to/copilot-voice-mcp"));
        assert_eq!(entry["args"], json!([]));
        assert_eq!(entry["tools"], json!(["*"]));
    }

    #[test]
    fn register_from_empty_creates_mcp_servers() {
        let mut root = json!({});
        assert!(apply_register(&mut root, "/bin/x"));
        assert!(root["mcpServers"][SERVER_KEY].is_object());
    }

    #[test]
    fn register_is_idempotent() {
        let mut root = json!({});
        assert!(apply_register(&mut root, "/bin/x"));
        // Same command again → no change reported.
        assert!(!apply_register(&mut root, "/bin/x"));
        // Different command → change reported.
        assert!(apply_register(&mut root, "/bin/y"));
    }

    #[test]
    fn unregister_removes_only_our_entry() {
        let mut root = json!({
            "mcpServers": {
                "playwright": { "command": "pw" },
                SERVER_KEY: { "command": "x" }
            }
        });
        assert!(apply_unregister(&mut root));
        assert!(root["mcpServers"].get(SERVER_KEY).is_none());
        assert!(root["mcpServers"].get("playwright").is_some());
        // Removing again is a no-op.
        assert!(!apply_unregister(&mut root));
    }

    #[test]
    fn registered_command_reads_back() {
        let mut root = json!({});
        apply_register(&mut root, "/bin/z");
        assert_eq!(registered_command(&root), Some("/bin/z"));
    }

    #[test]
    fn status_from_reports_up_to_date() {
        let mut root = json!({});
        apply_register(&mut root, "/tmp/copilot-voice-mcp");
        let st = status_from(
            &root,
            Path::new("/home/u/.copilot/mcp-config.json"),
            Path::new("/tmp/copilot-voice-mcp"),
        );
        assert!(st.registered);
        assert!(st.up_to_date);

        // A stale command path is registered-but-not-up-to-date.
        let st2 = status_from(&root, Path::new("/c"), Path::new("/tmp/other"));
        assert!(st2.registered);
        assert!(!st2.up_to_date);
    }

    #[test]
    fn read_missing_config_is_empty_object() {
        let dir = std::env::temp_dir().join(format!("mcpreg-test-{}", std::process::id()));
        let path = dir.join("does-not-exist.json");
        assert_eq!(read_config(&path).unwrap(), json!({}));
    }

    #[test]
    fn write_then_read_roundtrip() {
        let dir = std::env::temp_dir().join(format!("mcpreg-rt-{}", std::process::id()));
        let path = dir.join("nested/mcp-config.json");
        let mut root = json!({ "mcpServers": { "keep": { "command": "k" } } });
        apply_register(&mut root, "/bin/app");
        write_config(&path, &root).unwrap();
        let back = read_config(&path).unwrap();
        assert_eq!(registered_command(&back), Some("/bin/app"));
        assert_eq!(back["mcpServers"]["keep"]["command"], json!("k"));
        let _ = fs::remove_dir_all(&dir);
    }
}
