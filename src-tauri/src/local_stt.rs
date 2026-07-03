//! On-device speech-to-text via sherpa-onnx (NeMo Parakeet transducer).
//!
//! The recognizer is expensive to construct (loads the ONNX graphs), so a single
//! instance is cached per model id and reused. All sherpa calls are blocking
//! native work — callers should invoke [`transcribe`] from `spawn_blocking`.

use std::sync::{Mutex, OnceLock};

use anyhow::{anyhow, Context, Result};
use sherpa_onnx::{
    OfflineModelConfig, OfflineRecognizer, OfflineRecognizerConfig, OfflineTransducerModelConfig,
};

use crate::models;

struct Cached {
    model_id: String,
    recognizer: OfflineRecognizer,
}

fn cache() -> &'static Mutex<Option<Cached>> {
    static CACHE: OnceLock<Mutex<Option<Cached>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

/// Candidate execution providers, best first. CoreML accelerates on Apple
/// Silicon; CPU is the universal fallback.
fn providers() -> &'static [&'static str] {
    if cfg!(target_os = "macos") {
        &["coreml", "cpu"]
    } else {
        &["cpu"]
    }
}

fn build_recognizer(model_id: &str) -> Result<OfflineRecognizer> {
    let paths = models::resolve_stt_paths(model_id)?;
    let to_str = |p: &std::path::Path| p.to_string_lossy().to_string();

    let mut last_err = anyhow!("no execution provider succeeded");
    for provider in providers() {
        let config = OfflineRecognizerConfig {
            model_config: OfflineModelConfig {
                transducer: OfflineTransducerModelConfig {
                    encoder: Some(to_str(&paths.encoder)),
                    decoder: Some(to_str(&paths.decoder)),
                    joiner: Some(to_str(&paths.joiner)),
                },
                tokens: Some(to_str(&paths.tokens)),
                num_threads: 2,
                provider: Some((*provider).to_string()),
                debug: false,
                ..Default::default()
            },
            ..Default::default()
        };
        match OfflineRecognizer::create(&config) {
            Some(recognizer) => return Ok(recognizer),
            None => {
                last_err = anyhow!("failed to init recognizer with provider '{provider}'");
            }
        }
    }
    Err(last_err)
}

fn with_recognizer<T>(model_id: &str, f: impl FnOnce(&OfflineRecognizer) -> T) -> Result<T> {
    let mut guard = cache().lock().map_err(|_| anyhow!("stt cache poisoned"))?;
    let needs_build = guard.as_ref().map(|c| c.model_id != model_id).unwrap_or(true);
    if needs_build {
        let recognizer = build_recognizer(model_id)?;
        *guard = Some(Cached {
            model_id: model_id.to_string(),
            recognizer,
        });
    }
    let cached = guard.as_ref().expect("recognizer just built");
    Ok(f(&cached.recognizer))
}

/// Decode a mono/stereo PCM or float WAV buffer into `(sample_rate, mono f32)`.
fn decode_wav(wav: &[u8]) -> Result<(i32, Vec<f32>)> {
    let reader = hound::WavReader::new(std::io::Cursor::new(wav)).context("parsing WAV")?;
    let spec = reader.spec();
    let channels = spec.channels.max(1) as usize;
    let sample_rate = spec.sample_rate as i32;

    let interleaved: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Int => {
            let scale = 1i64 << (spec.bits_per_sample.saturating_sub(1));
            let scale = scale.max(1) as f32;
            reader
                .into_samples::<i32>()
                .filter_map(Result::ok)
                .map(|s| (s as f32 / scale).clamp(-1.0, 1.0))
                .collect()
        }
        hound::SampleFormat::Float => reader
            .into_samples::<f32>()
            .filter_map(Result::ok)
            .collect(),
    };

    let mono = if channels > 1 {
        interleaved
            .chunks(channels)
            .map(|frame| frame.iter().copied().sum::<f32>() / channels as f32)
            .collect()
    } else {
        interleaved
    };
    Ok((sample_rate, mono))
}

/// Transcribe a WAV audio buffer using the given local model.
/// Blocking: run inside `tokio::task::spawn_blocking`.
pub fn transcribe(model_id: &str, wav: &[u8]) -> Result<String> {
    let (sample_rate, samples) = decode_wav(wav)?;
    if samples.is_empty() {
        return Ok(String::new());
    }
    with_recognizer(model_id, |recognizer| {
        let stream = recognizer.create_stream();
        stream.accept_waveform(sample_rate, &samples);
        recognizer.decode(&stream);
        stream
            .get_result()
            .map(|r| r.text.trim().to_string())
            .unwrap_or_default()
    })
}

/// Drop the cached recognizer (e.g. after a model is deleted).
pub fn clear_cache() {
    if let Ok(mut guard) = cache().lock() {
        *guard = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_wav(sample_rate: u32, channels: u16, samples: &[i16]) -> Vec<u8> {
        let spec = hound::WavSpec {
            channels,
            sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut w = hound::WavWriter::new(&mut buf, spec).unwrap();
            for &s in samples {
                w.write_sample(s).unwrap();
            }
            w.finalize().unwrap();
        }
        buf.into_inner()
    }

    #[test]
    fn decode_mono_int16() {
        let wav = make_wav(16000, 1, &[0, i16::MAX, i16::MIN, 0]);
        let (sr, samples) = decode_wav(&wav).unwrap();
        assert_eq!(sr, 16000);
        assert_eq!(samples.len(), 4);
        assert!((samples[1] - 1.0).abs() < 1e-3);
        assert!((samples[2] + 1.0).abs() < 1e-3);
    }

    #[test]
    fn decode_stereo_downmixes_to_mono() {
        // Two frames of stereo -> two mono samples (averaged).
        let wav = make_wav(16000, 2, &[i16::MAX, i16::MAX, i16::MIN, i16::MIN]);
        let (_, samples) = decode_wav(&wav).unwrap();
        assert_eq!(samples.len(), 2);
    }
}
