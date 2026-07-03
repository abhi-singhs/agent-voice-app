//! Real end-to-end smoke test for the local voice engine.
//!
//! Downloads the default TTS + STT bundles into `~/.agent-voice-app/voice-models/`,
//! synthesizes speech with Kokoro, then transcribes that audio back with
//! Parakeet — exercising the whole local path (download → extract → resolve →
//! CoreML/CPU inference → WAV encode/decode) outside Tauri.
//!
//! Run with:  cargo run -p agent-voice-app --example local_smoke

use std::io::Write;

use app_lib::{local_stt, local_tts, models};

fn progress(p: models::ModelProgress) {
    match p.phase.as_str() {
        "download" => {
            print!("\r  downloading… {:.0}%   ", p.pct);
            let _ = std::io::stdout().flush();
        }
        other => println!("\n  [{other}] {}", p.message.unwrap_or_default()),
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = reqwest::Client::new();

    let tts_id = "kokoro-multi-lang-v1_0";
    let stt_id = "parakeet-tdt-0.6b-v2-int8";
    let sentence = "Hello from the local Kokoro model running on this MacBook Air.";

    println!("== TTS model: {tts_id} ==");
    models::download_with(&client, tts_id, progress).await?;

    println!("Synthesizing with voice sid 26 (bm_george)…");
    let t0 = std::time::Instant::now();
    let wav = local_tts::synthesize(tts_id, sentence, 26, 1.0)?;
    println!(
        "  generated {} bytes of WAV in {:?}",
        wav.len(),
        t0.elapsed()
    );
    let out = std::env::temp_dir().join("kokoro_smoke.wav");
    std::fs::write(&out, &wav)?;
    println!("  wrote {}", out.display());

    println!("\n== STT model: {stt_id} ==");
    models::download_with(&client, stt_id, progress).await?;

    println!("Transcribing the generated audio…");
    let t1 = std::time::Instant::now();
    let text = local_stt::transcribe(stt_id, &wav)?;
    println!("  transcribed in {:?}", t1.elapsed());
    println!("\nExpected : {sentence:?}");
    println!("Got      : {text:?}");

    Ok(())
}
