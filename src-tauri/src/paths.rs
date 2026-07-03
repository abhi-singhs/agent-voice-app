//! Central location for the app's own on-disk data.
//!
//! Everything the app owns (voice settings, ElevenLabs config, downloaded
//! models, call history, and the runtime-discovery file) lives under a single
//! dedicated directory in the user's home — [`APP_DIR_NAME`] — rather than the
//! shared `~/.copilot` directory. Keeping our data self-contained avoids
//! colliding with the Copilot CLI's own files.
//!
//! The runtime-discovery file path is defined by `voice_protocol::RUNTIME_REL_PATH`
//! because it must be shared with the separate MCP server crate; it uses the same
//! [`APP_DIR_NAME`] root.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};

/// Name of the app's dedicated data directory, relative to the user's home dir.
pub const APP_DIR_NAME: &str = ".agent-voice-app";

/// Absolute path to the app's data root (`~/.agent-voice-app`).
pub fn app_data_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| anyhow!("could not determine home directory"))?;
    Ok(home.join(APP_DIR_NAME))
}

/// App-owned subpaths that older builds wrote under `~/.copilot`. Only these are
/// removed by [`cleanup_legacy_copilot_data`]; shared files such as
/// `~/.copilot/mcp-config.json` are deliberately left alone.
const LEGACY_COPILOT_SUBPATHS: &[&str] =
    &["voice", "elevenlabs", "voice-models", "voice-call"];

/// Best-effort removal of the app's data from the legacy `~/.copilot` location.
///
/// Older builds stored config, models, and history under `~/.copilot`; now that
/// the app uses its own directory we delete that stale data (no migration).
/// Never errors — cleanup failures must not stop the app from starting.
pub fn cleanup_legacy_copilot_data() {
    let Some(home) = dirs::home_dir() else {
        return;
    };
    let copilot = home.join(".copilot");
    if !copilot.is_dir() {
        return;
    }
    for sub in LEGACY_COPILOT_SUBPATHS {
        remove_path_best_effort(&copilot.join(sub));
    }
}

/// Remove a file or directory if it exists, logging the outcome. Best-effort.
fn remove_path_best_effort(path: &Path) {
    let result = if path.is_dir() {
        std::fs::remove_dir_all(path)
    } else if path.exists() {
        std::fs::remove_file(path)
    } else {
        return;
    };
    match result {
        Ok(()) => eprintln!("voice-call: removed legacy data at {}", path.display()),
        Err(e) => eprintln!(
            "voice-call: could not remove legacy data at {}: {e}",
            path.display()
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_data_dir_is_under_home_and_named() {
        let dir = app_data_dir().unwrap();
        assert!(dir.ends_with(APP_DIR_NAME));
        // Uses our dedicated folder, not the shared Copilot directory.
        assert!(!dir.to_string_lossy().contains(".copilot"));
    }

    #[test]
    fn cleanup_removes_only_app_owned_subpaths() {
        let base = std::env::temp_dir().join(format!(
            "voice-cleanup-{}-{}",
            std::process::id(),
            line!()
        ));
        let copilot = base.join(".copilot");
        // App-owned data plus a shared file that must survive cleanup.
        std::fs::create_dir_all(copilot.join("voice-models")).unwrap();
        std::fs::create_dir_all(copilot.join("voice-call")).unwrap();
        std::fs::create_dir_all(copilot.join("elevenlabs")).unwrap();
        std::fs::write(copilot.join("mcp-config.json"), b"{}").unwrap();

        // Exercise the same removal logic against this temp tree.
        for sub in LEGACY_COPILOT_SUBPATHS {
            remove_path_best_effort(&copilot.join(sub));
        }

        assert!(!copilot.join("voice-models").exists());
        assert!(!copilot.join("voice-call").exists());
        assert!(!copilot.join("elevenlabs").exists());
        assert!(copilot.join("mcp-config.json").exists());

        let _ = std::fs::remove_dir_all(&base);
    }
}
