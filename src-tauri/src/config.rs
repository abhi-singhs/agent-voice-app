//! Loads and saves the ElevenLabs configuration at
//! `~/.agent-voice-app/elevenlabs/config.json`. The API key stays in the backend and is
//! never sent to the webview. The Setup panel writes this file via [`ElevenLabsConfig::save`].

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

/// Premade "George" voice — a sensible default so a fresh setup can call right away.
const DEFAULT_VOICE_ID: &str = "JBFqnCBsd6RMkjVDRZzb";

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ElevenLabsConfig {
    #[serde(rename = "apiKey", default)]
    pub api_key: String,
    #[serde(rename = "voiceId", default = "default_voice")]
    pub voice_id: String,
    #[serde(rename = "modelId", default = "default_model")]
    pub model_id: String,
    #[serde(rename = "outputFormat", default = "default_format")]
    pub output_format: String,
    #[serde(
        rename = "voiceName",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub voice_name: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(rename = "maxChars", default = "default_max_chars")]
    pub max_chars: usize,
}

fn default_voice() -> String {
    DEFAULT_VOICE_ID.to_string()
}
fn default_model() -> String {
    "eleven_turbo_v2_5".to_string()
}
fn default_format() -> String {
    "mp3_44100_128".to_string()
}
fn default_true() -> bool {
    true
}
fn default_max_chars() -> usize {
    800
}

impl Default for ElevenLabsConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            voice_id: default_voice(),
            model_id: default_model(),
            output_format: default_format(),
            voice_name: None,
            enabled: true,
            max_chars: default_max_chars(),
        }
    }
}

/// Non-secret subset of the config, safe to expose to the webview.
#[derive(Debug, Clone, Serialize)]
pub struct VoiceConfigInfo {
    pub voice_id: String,
    pub voice_name: Option<String>,
    pub model_id: String,
    pub enabled: bool,
    pub configured: bool,
}

impl ElevenLabsConfig {
    /// Absolute path to the config file (`~/.agent-voice-app/elevenlabs/config.json`).
    pub fn path() -> Result<PathBuf> {
        Ok(crate::paths::app_data_dir()?
            .join("elevenlabs")
            .join("config.json"))
    }

    /// Strict load used by TTS/STT: errors if the file is missing or the key /
    /// voice is empty.
    pub fn load() -> Result<Self> {
        Self::load_from(&Self::path()?)
    }

    fn load_from(path: &Path) -> Result<Self> {
        let bytes = std::fs::read(path)
            .with_context(|| format!("reading {} (is ElevenLabs configured?)", path.display()))?;
        let cfg: ElevenLabsConfig =
            serde_json::from_slice(&bytes).context("parsing elevenlabs/config.json")?;
        if cfg.api_key.trim().is_empty() {
            return Err(anyhow!("ElevenLabs apiKey is empty"));
        }
        if cfg.voice_id.trim().is_empty() {
            return Err(anyhow!("ElevenLabs voiceId is empty"));
        }
        Ok(cfg)
    }

    /// Lenient load used when editing/merging in the Setup panel: returns
    /// defaults when the file is missing or unparseable, and never errors on an
    /// empty key. Preserves any existing fields so saving merges rather than
    /// clobbers.
    pub fn load_or_default() -> Self {
        match Self::path() {
            Ok(p) => Self::load_or_default_from(&p),
            Err(_) => Self::default(),
        }
    }

    fn load_or_default_from(path: &Path) -> Self {
        match std::fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    /// Persist the config to `~/.agent-voice-app/elevenlabs/config.json` (owner-only).
    pub fn save(&self) -> Result<()> {
        self.save_to(&Self::path()?)
    }

    fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        let json = serde_json::to_vec_pretty(self).context("serializing elevenlabs config")?;
        std::fs::write(path, json).with_context(|| format!("writing {}", path.display()))?;
        restrict_permissions(path);
        Ok(())
    }

    /// A view of the config with no secrets, for the UI.
    pub fn info(&self) -> VoiceConfigInfo {
        VoiceConfigInfo {
            voice_id: self.voice_id.clone(),
            voice_name: self.voice_name.clone(),
            model_id: self.model_id.clone(),
            enabled: self.enabled,
            configured: !self.api_key.trim().is_empty(),
        }
    }
}

/// The file holds a secret API key, so tighten it to owner-only on unix.
#[cfg(unix)]
fn restrict_permissions(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
}

#[cfg(not(unix))]
fn restrict_permissions(_path: &Path) {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    /// A unique, disposable config path under the OS temp dir.
    fn temp_path() -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let mut p = std::env::temp_dir();
        p.push(format!("voice-cfg-test-{}-{n}", std::process::id()));
        p.push("config.json");
        p
    }

    #[test]
    fn save_then_load_round_trips() {
        let path = temp_path();
        let mut cfg = ElevenLabsConfig::default();
        cfg.api_key = "sk_test_123".to_string();
        cfg.voice_id = "voice_abc".to_string();
        cfg.voice_name = Some("Rachel".to_string());
        cfg.save_to(&path).unwrap();

        let loaded = ElevenLabsConfig::load_from(&path).unwrap();
        assert_eq!(loaded.api_key, "sk_test_123");
        assert_eq!(loaded.voice_id, "voice_abc");
        assert_eq!(loaded.voice_name.as_deref(), Some("Rachel"));
        assert_eq!(loaded.model_id, default_model());

        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn serializes_camel_case_keys() {
        let mut cfg = ElevenLabsConfig::default();
        cfg.api_key = "sk_x".to_string();
        let json = serde_json::to_string(&cfg).unwrap();
        assert!(json.contains("\"apiKey\""), "got {json}");
        assert!(json.contains("\"voiceId\""), "got {json}");
        assert!(json.contains("\"maxChars\""), "got {json}");
    }

    #[test]
    fn missing_file_yields_default_with_george_voice() {
        let cfg = ElevenLabsConfig::load_or_default_from(Path::new("/nonexistent/path/x.json"));
        assert!(cfg.api_key.is_empty());
        assert_eq!(cfg.voice_id, DEFAULT_VOICE_ID);
        assert!(cfg.enabled);
    }

    #[test]
    fn save_preserves_existing_non_key_fields() {
        let path = temp_path();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        // A hand-written config with customised, non-default fields.
        std::fs::write(
            &path,
            r#"{
                "apiKey": "sk_old",
                "voiceId": "old_voice",
                "modelId": "eleven_multilingual_v2",
                "outputFormat": "mp3_22050_32",
                "maxChars": 1200,
                "enabled": false
            }"#,
        )
        .unwrap();

        // Simulate the save_voice_config merge: load leniently, change only the
        // voice, keep the existing key, then persist.
        let mut cfg = ElevenLabsConfig::load_or_default_from(&path);
        cfg.voice_id = "new_voice".to_string();
        cfg.voice_name = Some("George".to_string());
        cfg.save_to(&path).unwrap();

        let reloaded = ElevenLabsConfig::load_from(&path).unwrap();
        assert_eq!(reloaded.api_key, "sk_old");
        assert_eq!(reloaded.voice_id, "new_voice");
        assert_eq!(reloaded.voice_name.as_deref(), Some("George"));
        assert_eq!(reloaded.model_id, "eleven_multilingual_v2");
        assert_eq!(reloaded.output_format, "mp3_22050_32");
        assert_eq!(reloaded.max_chars, 1200);
        assert!(!reloaded.enabled);

        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn load_rejects_empty_key() {
        let path = temp_path();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, r#"{"apiKey": "", "voiceId": "v"}"#).unwrap();
        assert!(ElevenLabsConfig::load_from(&path).is_err());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}
