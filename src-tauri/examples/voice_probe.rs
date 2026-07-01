//! Throwaway probe: real ElevenLabs TTS→STT round-trip using the user's config.
//! Run with: cargo run -p copilot-voice-call --example voice_probe
//! Uses minimal text to conserve free-tier credits.

use app_lib::{config::ElevenLabsConfig, elevenlabs};
use reqwest::Client;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cfg = ElevenLabsConfig::load()?;
    let client = Client::new();

    let phrase = "Hello from the voice probe.";
    eprintln!("TTS: synthesizing {phrase:?} (voice={})", cfg.voice_id);
    let audio = elevenlabs::synthesize(&client, &cfg, phrase).await?;
    eprintln!("TTS: got {} bytes ({})", audio.len(), cfg.output_format);
    assert!(audio.len() > 1000, "audio unexpectedly small");

    eprintln!("STT: transcribing the synthesized audio...");
    let text = elevenlabs::transcribe(&client, &cfg, audio, "audio/mpeg", "probe.mp3").await?;
    eprintln!("STT: transcript = {text:?}");
    assert!(!text.is_empty(), "transcript empty");

    eprintln!("OK: round-trip succeeded.");
    Ok(())
}
