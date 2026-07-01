//! Loads the shared ElevenLabs configuration written by the Copilot CLI at
//! `~/.copilot/elevenlabs/config.json`. The API key stays in the backend and is
//! never sent to the webview.

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

const CONFIG_REL_PATH: &str = ".copilot/elevenlabs/config.json";

#[derive(Debug, Clone, Deserialize)]
pub struct ElevenLabsConfig {
    #[serde(rename = "apiKey")]
    pub api_key: String,
    #[serde(rename = "voiceId")]
    pub voice_id: String,
    #[serde(rename = "modelId", default = "default_model")]
    pub model_id: String,
    #[serde(rename = "outputFormat", default = "default_format")]
    pub output_format: String,
    #[serde(rename = "voiceName", default)]
    pub voice_name: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(rename = "maxChars", default = "default_max_chars")]
    pub max_chars: usize,
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
    /// Load and parse the config file, returning an error if it's missing.
    pub fn load() -> Result<Self> {
        let home = dirs::home_dir().ok_or_else(|| anyhow!("could not determine home directory"))?;
        let path = home.join(CONFIG_REL_PATH);
        let bytes = std::fs::read(&path)
            .with_context(|| format!("reading {} (is ElevenLabs configured?)", path.display()))?;
        let cfg: ElevenLabsConfig =
            serde_json::from_slice(&bytes).context("parsing elevenlabs/config.json")?;
        if cfg.api_key.trim().is_empty() {
            return Err(anyhow!("ElevenLabs apiKey is empty"));
        }
        Ok(cfg)
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
