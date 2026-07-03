//! Runtime-discovery file the app writes so the ephemeral MCP server can find
//! and authenticate to the always-on WebSocket server.

use std::fs;
use std::path::PathBuf;

use anyhow::{anyhow, Context};
use voice_protocol::{RuntimeInfo, PROTOCOL_VERSION, RUNTIME_REL_PATH};

/// Absolute path to the runtime-discovery file (`~/.agent-voice-app/voice-call/runtime.json`).
pub fn path() -> anyhow::Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| anyhow!("could not determine home directory"))?;
    Ok(home.join(RUNTIME_REL_PATH))
}

/// Write the runtime-discovery file with the given port and token.
pub fn write(port: u16, token: &str) -> anyhow::Result<PathBuf> {
    let path = path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    let info = RuntimeInfo {
        port,
        token: token.to_string(),
        pid: std::process::id(),
        protocol_version: PROTOCOL_VERSION,
    };
    let json = serde_json::to_vec_pretty(&info)?;
    fs::write(&path, json).with_context(|| format!("writing {}", path.display()))?;
    restrict_permissions(&path);
    Ok(path)
}

/// Best-effort removal of the runtime-discovery file (called on app exit).
pub fn remove() {
    if let Ok(p) = path() {
        let _ = fs::remove_file(p);
    }
}

/// The file holds a secret token, so tighten it to owner-only on unix.
#[cfg(unix)]
fn restrict_permissions(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
}

#[cfg(not(unix))]
fn restrict_permissions(_path: &std::path::Path) {}
