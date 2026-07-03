//! On-device text-to-speech via sherpa-onnx (Kokoro-82M).
//!
//! The synthesizer is cached per model id and reused across calls. All sherpa
//! calls are blocking native work — callers should invoke [`synthesize`] from
//! `spawn_blocking`. Output is 16-bit PCM WAV bytes so the existing frontend
//! player (`decodeAudioData`) can play it unchanged.

use std::io::Cursor;
use std::sync::{Mutex, OnceLock};

use anyhow::{anyhow, Result};
use serde::Serialize;
use sherpa_onnx::{
    GenerationConfig, OfflineTts, OfflineTtsConfig, OfflineTtsKokoroModelConfig,
    OfflineTtsModelConfig,
};

use crate::models;

struct Cached {
    model_id: String,
    tts: OfflineTts,
}

fn cache() -> &'static Mutex<Option<Cached>> {
    static CACHE: OnceLock<Mutex<Option<Cached>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

fn providers() -> &'static [&'static str] {
    if cfg!(target_os = "macos") {
        &["coreml", "cpu"]
    } else {
        &["cpu"]
    }
}

fn build_tts(model_id: &str) -> Result<OfflineTts> {
    let paths = models::resolve_tts_paths(model_id)?;
    let to_str = |p: &std::path::Path| p.to_string_lossy().to_string();
    let lexicon = if paths.lexicon.is_empty() {
        None
    } else {
        Some(
            paths
                .lexicon
                .iter()
                .map(|p| to_str(p))
                .collect::<Vec<_>>()
                .join(","),
        )
    };

    let mut last_err = anyhow!("no execution provider succeeded");
    for provider in providers() {
        let config = OfflineTtsConfig {
            model: OfflineTtsModelConfig {
                kokoro: OfflineTtsKokoroModelConfig {
                    model: Some(to_str(&paths.model)),
                    voices: Some(to_str(&paths.voices)),
                    tokens: Some(to_str(&paths.tokens)),
                    data_dir: Some(to_str(&paths.data_dir)),
                    dict_dir: paths.dict_dir.as_deref().map(to_str),
                    lexicon: lexicon.clone(),
                    ..Default::default()
                },
                num_threads: 2,
                provider: Some((*provider).to_string()),
                debug: false,
                ..Default::default()
            },
            ..Default::default()
        };
        match OfflineTts::create(&config) {
            Some(tts) => return Ok(tts),
            None => {
                last_err = anyhow!("failed to init TTS with provider '{provider}'");
            }
        }
    }
    Err(last_err)
}

fn with_tts<T>(model_id: &str, f: impl FnOnce(&OfflineTts) -> T) -> Result<T> {
    let mut guard = cache().lock().map_err(|_| anyhow!("tts cache poisoned"))?;
    let needs_build = guard.as_ref().map(|c| c.model_id != model_id).unwrap_or(true);
    if needs_build {
        let tts = build_tts(model_id)?;
        *guard = Some(Cached {
            model_id: model_id.to_string(),
            tts,
        });
    }
    let cached = guard.as_ref().expect("tts just built");
    Ok(f(&cached.tts))
}

/// Encode f32 samples in [-1, 1] as a 16-bit PCM mono WAV buffer.
fn encode_wav(samples: &[f32], sample_rate: i32) -> Result<Vec<u8>> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: sample_rate.max(1) as u32,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut cursor = Cursor::new(Vec::new());
    {
        let mut writer = hound::WavWriter::new(&mut cursor, spec)?;
        for &s in samples {
            let clamped = s.clamp(-1.0, 1.0);
            let v = (clamped * i16::MAX as f32).round() as i16;
            writer.write_sample(v)?;
        }
        writer.finalize()?;
    }
    Ok(cursor.into_inner())
}

/// Synthesize `text` to WAV bytes using the given local model, voice, and speed.
/// Blocking: run inside `tokio::task::spawn_blocking`.
pub fn synthesize(model_id: &str, text: &str, sid: i32, speed: f32) -> Result<Vec<u8>> {
    if text.trim().is_empty() {
        return Err(anyhow!("cannot synthesize empty text"));
    }
    let (samples, sample_rate) = with_tts(model_id, |tts| {
        let cfg = GenerationConfig {
            sid,
            speed: if speed > 0.0 { speed } else { 1.0 },
            ..Default::default()
        };
        let audio = tts.generate_with_config(text, &cfg, None::<fn(&[f32], f32) -> bool>);
        audio.map(|a| (a.samples().to_vec(), a.sample_rate()))
    })?
    .ok_or_else(|| anyhow!("TTS generation failed"))?;

    encode_wav(&samples, sample_rate)
}

/// Metadata about a selectable local voice.
#[derive(Debug, Clone, Serialize)]
pub struct VoiceInfo {
    pub sid: i32,
    pub name: String,
    pub locale: String,
    pub gender: String,
}

/// Voice names for the Kokoro multi-lang v1.0 bundle, indexed by speaker id.
const KOKORO_VOICES: &[&str] = &[
    "af_alloy", "af_aoede", "af_bella", "af_heart", "af_jessica", "af_kore", "af_nicole",
    "af_nova", "af_river", "af_sarah", "af_sky", // 0-10
    "am_adam", "am_echo", "am_eric", "am_fenrir", "am_liam", "am_michael", "am_onyx", "am_puck",
    "am_santa", // 11-19
    "bf_alice", "bf_emma", "bf_isabella", "bf_lily", // 20-23
    "bm_daniel", "bm_fable", "bm_george", "bm_lewis", // 24-27
    "ef_dora", "em_alex", // 28-29
    "ff_siwis", // 30
    "hf_alpha", "hf_beta", "hm_omega", "hm_psi", // 31-34
    "if_sara", "im_nicola", // 35-36
    "jf_alpha", "jf_gongitsune", "jf_nezumi", "jf_tebukuro", "jm_kumo", // 37-41
    "pf_dora", "pm_alex", "pm_santa", // 42-44
    "zf_xiaobei", "zf_xiaoni", "zf_xiaoxiao", "zf_xiaoyi", "zm_yunjian", "zm_yunxi", "zm_yunxia",
    "zm_yunyang", // 45-52
];

fn locale_of(name: &str) -> &'static str {
    match name.as_bytes().first() {
        Some(b'a') => "English (US)",
        Some(b'b') => "English (UK)",
        Some(b'e') => "Spanish",
        Some(b'f') => "French",
        Some(b'h') => "Hindi",
        Some(b'i') => "Italian",
        Some(b'j') => "Japanese",
        Some(b'p') => "Portuguese",
        Some(b'z') => "Chinese",
        _ => "Unknown",
    }
}

fn gender_of(name: &str) -> &'static str {
    match name.as_bytes().get(1) {
        Some(b'f') => "Female",
        Some(b'm') => "Male",
        _ => "Unknown",
    }
}

/// List the voices available for a model. Uses the embedded catalog so the UI
/// can render the picker instantly, before or after the model is downloaded.
pub fn list_voices(_model_id: &str) -> Result<Vec<VoiceInfo>> {
    Ok((0..KOKORO_VOICES.len())
        .map(|i| {
            let name = KOKORO_VOICES[i];
            VoiceInfo {
                sid: i as i32,
                name: name.to_string(),
                locale: locale_of(name).to_string(),
                gender: gender_of(name).to_string(),
            }
        })
        .collect())
}

/// Drop the cached synthesizer (e.g. after a model is deleted).
pub fn clear_cache() {
    if let Ok(mut guard) = cache().lock() {
        *guard = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_has_53_voices() {
        assert_eq!(KOKORO_VOICES.len(), 53);
    }

    #[test]
    fn george_is_sid_26() {
        assert_eq!(KOKORO_VOICES[26], "bm_george");
    }

    #[test]
    fn locale_and_gender_derivation() {
        assert_eq!(locale_of("bm_george"), "English (UK)");
        assert_eq!(gender_of("bm_george"), "Male");
        assert_eq!(locale_of("af_heart"), "English (US)");
        assert_eq!(gender_of("af_heart"), "Female");
        assert_eq!(locale_of("zf_xiaoni"), "Chinese");
    }

    #[test]
    fn encode_wav_roundtrips_via_hound() {
        let samples = vec![0.0f32, 0.5, -0.5, 1.0, -1.0];
        let wav = encode_wav(&samples, 24000).unwrap();
        let reader = hound::WavReader::new(Cursor::new(wav)).unwrap();
        assert_eq!(reader.spec().sample_rate, 24000);
        assert_eq!(reader.spec().channels, 1);
        assert_eq!(reader.len(), 5);
    }
}
