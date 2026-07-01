//! ElevenLabs HTTP client: text-to-speech and speech-to-text.
//!
//! Lives in the backend so the API key never enters the webview. Audio bytes
//! flow app ⇄ ElevenLabs over HTTPS and app ⇄ webview over Tauri IPC.

use anyhow::{anyhow, Context, Result};
use reqwest::multipart::{Form, Part};
use reqwest::Client;
use serde::Serialize;

use crate::config::ElevenLabsConfig;

const API_BASE: &str = "https://api.elevenlabs.io";
const STT_MODEL: &str = "scribe_v1";

/// A voice available on the account, sent to the webview for the picker.
#[derive(Debug, Clone, Serialize)]
pub struct VoiceSummary {
    pub voice_id: String,
    pub name: String,
    pub category: Option<String>,
}

/// List the voices available to `api_key` via `GET /v1/voices`. Doubles as a
/// validation of the key: an invalid key yields an HTTP error.
pub async fn list_voices(client: &Client, api_key: &str) -> Result<Vec<VoiceSummary>> {
    let url = format!("{API_BASE}/v1/voices");
    let resp = client
        .get(&url)
        .header("xi-api-key", api_key)
        .send()
        .await
        .context("voices request failed")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let detail = resp.text().await.unwrap_or_default();
        return Err(anyhow!("voices HTTP {status}: {}", detail.trim()));
    }

    let json: serde_json::Value = resp.json().await.context("parsing voices response")?;
    let voices = json
        .get("voices")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow!("unexpected voices response shape"))?
        .iter()
        .filter_map(|v| {
            let voice_id = v.get("voice_id")?.as_str()?.to_string();
            let name = v
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or(&voice_id)
                .to_string();
            let category = v
                .get("category")
                .and_then(|c| c.as_str())
                .map(|s| s.to_string());
            Some(VoiceSummary {
                voice_id,
                name,
                category,
            })
        })
        .collect();
    Ok(voices)
}

/// Synthesize `text` to speech, returning encoded audio bytes (per `outputFormat`).
pub async fn synthesize(client: &Client, cfg: &ElevenLabsConfig, text: &str) -> Result<Vec<u8>> {
    let text = truncate(text, cfg.max_chars);
    let url = format!(
        "{API_BASE}/v1/text-to-speech/{}?output_format={}",
        cfg.voice_id, cfg.output_format
    );
    let body = serde_json::json!({
        "text": text,
        "model_id": cfg.model_id,
    });

    let resp = client
        .post(&url)
        .header("xi-api-key", &cfg.api_key)
        .header("accept", "audio/mpeg")
        .json(&body)
        .send()
        .await
        .context("TTS request failed")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let detail = resp.text().await.unwrap_or_default();
        return Err(anyhow!("TTS HTTP {status}: {}", detail.trim()));
    }
    Ok(resp.bytes().await?.to_vec())
}

/// Transcribe audio bytes to text using the Scribe model.
pub async fn transcribe(
    client: &Client,
    cfg: &ElevenLabsConfig,
    audio: Vec<u8>,
    mime: &str,
    filename: &str,
) -> Result<String> {
    let url = format!("{API_BASE}/v1/speech-to-text");
    let part = Part::bytes(audio)
        .file_name(filename.to_string())
        .mime_str(mime)
        .context("invalid audio mime type")?;
    let form = Form::new().text("model_id", STT_MODEL).part("file", part);

    let resp = client
        .post(&url)
        .header("xi-api-key", &cfg.api_key)
        .multipart(form)
        .send()
        .await
        .context("STT request failed")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let detail = resp.text().await.unwrap_or_default();
        return Err(anyhow!("STT HTTP {status}: {}", detail.trim()));
    }

    let json: serde_json::Value = resp.json().await.context("parsing STT response")?;
    let text = json
        .get("text")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .trim()
        .to_string();
    Ok(text)
}

/// Trim to at most `max_chars` characters on a char boundary.
fn truncate(text: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return text.to_string();
    }
    match text.char_indices().nth(max_chars) {
        Some((idx, _)) => text[..idx].to_string(),
        None => text.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::truncate;

    #[test]
    fn truncate_respects_char_boundaries() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello world", 5), "hello");
        assert_eq!(truncate("naïve café", 4), "naïv");
        assert_eq!(truncate("", 5), "");
    }
}
