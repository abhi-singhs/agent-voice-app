//! Voice engine settings: which provider powers speech-to-text and
//! text-to-speech, plus the local-model preferences.
//!
//! Persisted at `~/.copilot/voice/config.json`. Local models are the default so
//! a fresh install works offline and free; ElevenLabs is opt-in (its API key
//! still lives in `~/.copilot/elevenlabs/config.json`, see [`crate::config`]).
//!
//! There are no secrets in this file, so the whole struct is safe to expose to
//! the webview.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

const CONFIG_REL_PATH: &str = ".copilot/voice/config.json";

/// Which backend powers a given direction (STT or TTS).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    /// On-device models (sherpa-onnx: Parakeet + Kokoro). The default.
    Local,
    /// ElevenLabs cloud API.
    Elevenlabs,
}

impl Default for Provider {
    fn default() -> Self {
        Provider::Local
    }
}

/// Preferences for the on-device models.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LocalSettings {
    /// Model id (see [`crate::models`]) used for speech-to-text.
    #[serde(rename = "sttModel", default = "default_stt_model")]
    pub stt_model: String,
    /// Model id used for text-to-speech.
    #[serde(rename = "ttsModel", default = "default_tts_model")]
    pub tts_model: String,
    /// Kokoro speaker id (voice). Defaults to `bm_george` to mirror the classic
    /// ElevenLabs "George" default.
    #[serde(rename = "ttsVoiceSid", default = "default_voice_sid")]
    pub tts_voice_sid: i32,
    /// Speaking rate multiplier (1.0 = natural).
    #[serde(default = "default_speed")]
    pub speed: f32,
}

fn default_stt_model() -> String {
    "parakeet-tdt-0.6b-v2-int8".to_string()
}
fn default_tts_model() -> String {
    "kokoro-int8-multi-lang-v1_0".to_string()
}
fn default_voice_sid() -> i32 {
    26 // bm_george (British male) in kokoro-multi-lang-v1_0
}
fn default_speed() -> f32 {
    1.0
}

impl Default for LocalSettings {
    fn default() -> Self {
        Self {
            stt_model: default_stt_model(),
            tts_model: default_tts_model(),
            tts_voice_sid: default_voice_sid(),
            speed: default_speed(),
        }
    }
}

/// Top-level voice settings. Local-first by default.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct VoiceSettings {
    #[serde(rename = "sttProvider", default)]
    pub stt_provider: Provider,
    #[serde(rename = "ttsProvider", default)]
    pub tts_provider: Provider,
    #[serde(default)]
    pub local: LocalSettings,
}

impl Default for VoiceSettings {
    fn default() -> Self {
        Self {
            stt_provider: Provider::default(),
            tts_provider: Provider::default(),
            local: LocalSettings::default(),
        }
    }
}

impl VoiceSettings {
    /// Absolute path to the settings file (`~/.copilot/voice/config.json`).
    pub fn path() -> Result<PathBuf> {
        let home = dirs::home_dir().ok_or_else(|| anyhow!("could not determine home directory"))?;
        Ok(home.join(CONFIG_REL_PATH))
    }

    /// Load settings, falling back to defaults (local-first) when the file is
    /// missing or unparseable. Never errors, so callers always have a usable
    /// config.
    pub fn load() -> Self {
        match Self::path() {
            Ok(p) => Self::load_from(&p),
            Err(_) => Self::default(),
        }
    }

    fn load_from(path: &Path) -> Self {
        match std::fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    /// Persist the settings to `~/.copilot/voice/config.json`.
    pub fn save(&self) -> Result<()> {
        self.save_to(&Self::path()?)
    }

    fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        let json = serde_json::to_vec_pretty(self).context("serializing voice settings")?;
        std::fs::write(path, json).with_context(|| format!("writing {}", path.display()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    fn temp_path() -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let mut p = std::env::temp_dir();
        p.push(format!("voice-settings-test-{}-{n}", std::process::id()));
        p.push("config.json");
        p
    }

    #[test]
    fn defaults_are_local_first() {
        let s = VoiceSettings::default();
        assert_eq!(s.stt_provider, Provider::Local);
        assert_eq!(s.tts_provider, Provider::Local);
        assert_eq!(s.local.stt_model, "parakeet-tdt-0.6b-v2-int8");
        assert_eq!(s.local.tts_model, "kokoro-int8-multi-lang-v1_0");
        assert_eq!(s.local.tts_voice_sid, 26);
        assert_eq!(s.local.speed, 1.0);
    }

    #[test]
    fn missing_file_yields_local_defaults() {
        let s = VoiceSettings::load_from(Path::new("/nonexistent/voice/config.json"));
        assert_eq!(s.stt_provider, Provider::Local);
        assert_eq!(s.tts_provider, Provider::Local);
    }

    #[test]
    fn save_then_load_round_trips() {
        let path = temp_path();
        let mut s = VoiceSettings::default();
        s.tts_provider = Provider::Elevenlabs;
        s.local.tts_voice_sid = 3;
        s.local.speed = 1.15;
        s.save_to(&path).unwrap();

        let loaded = VoiceSettings::load_from(&path);
        assert_eq!(loaded.stt_provider, Provider::Local);
        assert_eq!(loaded.tts_provider, Provider::Elevenlabs);
        assert_eq!(loaded.local.tts_voice_sid, 3);
        assert_eq!(loaded.local.speed, 1.15);

        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn provider_serializes_lowercase() {
        let json = serde_json::to_string(&Provider::Elevenlabs).unwrap();
        assert_eq!(json, "\"elevenlabs\"");
        let json = serde_json::to_string(&Provider::Local).unwrap();
        assert_eq!(json, "\"local\"");
    }

    #[test]
    fn partial_json_fills_defaults() {
        // Only one field set; the rest should default (local-first).
        let s: VoiceSettings = serde_json::from_str(r#"{"ttsProvider":"elevenlabs"}"#).unwrap();
        assert_eq!(s.stt_provider, Provider::Local);
        assert_eq!(s.tts_provider, Provider::Elevenlabs);
        assert_eq!(s.local.stt_model, "parakeet-tdt-0.6b-v2-int8");
    }
}
