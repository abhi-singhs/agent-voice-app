//! Local model catalog + downloader for the on-device STT/TTS engines.
//!
//! Model bundles are the pre-packaged sherpa-onnx `.tar.bz2` archives published
//! on GitHub. They are downloaded on first use into `~/.copilot/voice-models/`
//! (small app download, fully offline afterwards) and extracted in place. The
//! webview is kept informed via `voice://model-progress` events.

use std::fs::File;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use futures_util::StreamExt;
use reqwest::Client;
use serde::Serialize;
use tauri::{AppHandle, Emitter};

/// Event name emitted to the webview during model downloads.
pub const PROGRESS_EVENT: &str = "voice://model-progress";

/// Whether a model powers speech-to-text or text-to-speech.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelKind {
    Stt,
    Tts,
}

/// A downloadable model bundle.
#[derive(Debug, Clone)]
pub struct ModelSpec {
    /// Stable id used in settings and commands.
    pub id: &'static str,
    pub kind: ModelKind,
    pub display_name: &'static str,
    /// URL of the `.tar.bz2` bundle.
    pub url: &'static str,
    /// Top-level directory the archive extracts into.
    pub dir_name: &'static str,
    /// Rough download size for the UI.
    pub approx_mb: u32,
}

/// The bundles the app knows how to fetch. Kept small and curated; extend as
/// needed. Defaults (Parakeet v2 int8 + Kokoro int8 multi-lang) are first.
pub const CATALOG: &[ModelSpec] = &[
    ModelSpec {
        id: "parakeet-tdt-0.6b-v2-int8",
        kind: ModelKind::Stt,
        display_name: "Parakeet TDT 0.6b v2 (int8, English)",
        url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-nemo-parakeet-tdt-0.6b-v2-int8.tar.bz2",
        dir_name: "sherpa-onnx-nemo-parakeet-tdt-0.6b-v2-int8",
        approx_mb: 483,
    },
    ModelSpec {
        id: "kokoro-int8-multi-lang-v1_0",
        kind: ModelKind::Tts,
        display_name: "Kokoro-82M v1.0 (int8, multi-lang)",
        url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/tts-models/kokoro-int8-multi-lang-v1_0.tar.bz2",
        dir_name: "kokoro-int8-multi-lang-v1_0",
        approx_mb: 132,
    },
];

/// Look up a model spec by id.
pub fn spec(id: &str) -> Option<&'static ModelSpec> {
    CATALOG.iter().find(|m| m.id == id)
}

/// Base directory for all downloaded models (`~/.copilot/voice-models/`).
pub fn models_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| anyhow!("could not determine home directory"))?;
    Ok(home.join(".copilot").join("voice-models"))
}

/// Directory a given model extracts into.
pub fn model_dir(spec: &ModelSpec) -> Result<PathBuf> {
    Ok(models_dir()?.join(spec.dir_name))
}

/// Resolved paths for a transducer (Parakeet) STT model.
#[derive(Debug, Clone)]
pub struct SttModelPaths {
    pub encoder: PathBuf,
    pub decoder: PathBuf,
    pub joiner: PathBuf,
    pub tokens: PathBuf,
}

/// Resolved paths for a Kokoro TTS model.
#[derive(Debug, Clone)]
pub struct TtsModelPaths {
    pub model: PathBuf,
    pub voices: PathBuf,
    pub tokens: PathBuf,
    pub data_dir: PathBuf,
    pub lexicon: Vec<PathBuf>,
    pub dict_dir: Option<PathBuf>,
}

/// Find the first file in `dir` whose name matches `pred`.
fn find_file<F: Fn(&str) -> bool>(dir: &Path, pred: F) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    let mut hits: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .filter(|p| p.file_name().and_then(|n| n.to_str()).map(&pred).unwrap_or(false))
        .collect();
    hits.sort();
    hits.into_iter().next()
}

fn find_all<F: Fn(&str) -> bool>(dir: &Path, pred: F) -> Vec<PathBuf> {
    let mut hits: Vec<PathBuf> = match std::fs::read_dir(dir) {
        Ok(entries) => entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file())
            .filter(|p| p.file_name().and_then(|n| n.to_str()).map(&pred).unwrap_or(false))
            .collect(),
        Err(_) => Vec::new(),
    };
    hits.sort();
    hits
}

/// Resolve the encoder/decoder/joiner/tokens files for a transducer model.
/// Robust to int8 vs fp32 filenames (e.g. `encoder.int8.onnx`).
pub fn resolve_stt_paths(id: &str) -> Result<SttModelPaths> {
    let spec = spec(id).ok_or_else(|| anyhow!("unknown STT model: {id}"))?;
    let dir = model_dir(spec)?;
    if !dir.is_dir() {
        return Err(anyhow!("model '{id}' is not downloaded"));
    }
    resolve_stt_paths_in(&dir)
}

/// Resolve transducer files within a specific directory (see [`resolve_stt_paths`]).
fn resolve_stt_paths_in(dir: &Path) -> Result<SttModelPaths> {
    let onnx = |prefix: &'static str| {
        move |n: &str| n.starts_with(prefix) && n.ends_with(".onnx")
    };
    Ok(SttModelPaths {
        encoder: find_file(dir, onnx("encoder"))
            .ok_or_else(|| anyhow!("encoder .onnx not found in {}", dir.display()))?,
        decoder: find_file(dir, onnx("decoder"))
            .ok_or_else(|| anyhow!("decoder .onnx not found in {}", dir.display()))?,
        joiner: find_file(dir, onnx("joiner"))
            .ok_or_else(|| anyhow!("joiner .onnx not found in {}", dir.display()))?,
        tokens: {
            let p = dir.join("tokens.txt");
            if !p.is_file() {
                return Err(anyhow!("tokens.txt not found in {}", dir.display()));
            }
            p
        },
    })
}

/// Resolve the Kokoro model files. Prefers the int8 model when present.
pub fn resolve_tts_paths(id: &str) -> Result<TtsModelPaths> {
    let spec = spec(id).ok_or_else(|| anyhow!("unknown TTS model: {id}"))?;
    let dir = model_dir(spec)?;
    if !dir.is_dir() {
        return Err(anyhow!("model '{id}' is not downloaded"));
    }
    resolve_tts_paths_in(&dir)
}

/// Resolve Kokoro files within a specific directory (see [`resolve_tts_paths`]).
fn resolve_tts_paths_in(dir: &Path) -> Result<TtsModelPaths> {
    let model = find_file(dir, |n| n == "model.int8.onnx")
        .or_else(|| find_file(dir, |n| n.starts_with("model") && n.ends_with(".onnx")))
        .ok_or_else(|| anyhow!("model .onnx not found in {}", dir.display()))?;
    let voices = dir.join("voices.bin");
    let tokens = dir.join("tokens.txt");
    let data_dir = dir.join("espeak-ng-data");
    for (p, what) in [(&voices, "voices.bin"), (&tokens, "tokens.txt")] {
        if !p.is_file() {
            return Err(anyhow!("{what} not found in {}", dir.display()));
        }
    }
    if !data_dir.is_dir() {
        return Err(anyhow!("espeak-ng-data not found in {}", dir.display()));
    }
    let lexicon = find_all(dir, |n| n.starts_with("lexicon") && n.ends_with(".txt"));
    let dict_candidate = dir.join("dict");
    let dict_dir = dict_candidate.is_dir().then_some(dict_candidate);
    Ok(TtsModelPaths {
        model,
        voices,
        tokens,
        data_dir,
        lexicon,
        dict_dir,
    })
}

/// Whether all required files for a model are present on disk.
pub fn is_installed(spec: &ModelSpec) -> bool {
    match spec.kind {
        ModelKind::Stt => resolve_stt_paths(spec.id).is_ok(),
        ModelKind::Tts => resolve_tts_paths(spec.id).is_ok(),
    }
}

/// Non-secret status of a model, for the UI.
#[derive(Debug, Clone, Serialize)]
pub struct ModelStatus {
    pub id: String,
    pub kind: ModelKind,
    pub display_name: String,
    pub installed: bool,
    pub approx_mb: u32,
}

/// Status of every catalog model.
pub fn all_status() -> Vec<ModelStatus> {
    CATALOG
        .iter()
        .map(|m| ModelStatus {
            id: m.id.to_string(),
            kind: m.kind,
            display_name: m.display_name.to_string(),
            installed: is_installed(m),
            approx_mb: m.approx_mb,
        })
        .collect()
}

/// Progress payload emitted during a download.
#[derive(Debug, Clone, Serialize)]
pub struct ModelProgress {
    pub id: String,
    /// "download" | "extract" | "done" | "error"
    pub phase: String,
    pub received: u64,
    pub total: u64,
    pub pct: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

fn emit(app: &AppHandle, p: ModelProgress) {
    let _ = app.emit(PROGRESS_EVENT, p);
}

/// Download and extract a model bundle, emitting `voice://model-progress`
/// events to the webview. Idempotent (returns early if already installed).
pub async fn download(app: &AppHandle, client: &Client, id: &str) -> Result<()> {
    let app = app.clone();
    download_with(client, id, move |p| emit(&app, p)).await
}

/// Download and extract a model bundle, reporting progress via `on_progress`.
/// This is the engine behind [`download`]; it takes a plain callback so it can
/// be driven outside Tauri (tests, examples). Idempotent.
pub async fn download_with<F>(client: &Client, id: &str, mut on_progress: F) -> Result<()>
where
    F: FnMut(ModelProgress),
{
    let spec = spec(id).ok_or_else(|| anyhow!("unknown model: {id}"))?;

    if is_installed(spec) {
        on_progress(ModelProgress {
            id: id.to_string(),
            phase: "done".into(),
            received: 0,
            total: 0,
            pct: 100.0,
            message: Some("already installed".into()),
        });
        return Ok(());
    }

    let dir = models_dir()?;
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let archive = dir.join(format!("{}.tar.bz2", spec.dir_name));
    let part = dir.join(format!("{}.tar.bz2.part", spec.dir_name));

    let result = download_inner(client, spec, &part, &archive, &dir, &mut on_progress).await;

    // Best-effort cleanup of the intermediate files.
    let _ = std::fs::remove_file(&part);
    let _ = std::fs::remove_file(&archive);

    match &result {
        Ok(()) => on_progress(ModelProgress {
            id: id.to_string(),
            phase: "done".into(),
            received: 0,
            total: 0,
            pct: 100.0,
            message: None,
        }),
        Err(e) => on_progress(ModelProgress {
            id: id.to_string(),
            phase: "error".into(),
            received: 0,
            total: 0,
            pct: 0.0,
            message: Some(e.to_string()),
        }),
    }
    result
}

async fn download_inner<F>(
    client: &Client,
    spec: &ModelSpec,
    part: &Path,
    archive: &Path,
    dir: &Path,
    on_progress: &mut F,
) -> Result<()>
where
    F: FnMut(ModelProgress),
{
    // --- Download phase (streamed to disk with progress) ---
    let resp = client
        .get(spec.url)
        .send()
        .await
        .with_context(|| format!("requesting {}", spec.url))?;
    if !resp.status().is_success() {
        return Err(anyhow!("download HTTP {}", resp.status()));
    }
    let total = resp.content_length().unwrap_or(0);

    let mut file = File::create(part).with_context(|| format!("creating {}", part.display()))?;
    let mut received: u64 = 0;
    let mut last_emit: u64 = 0;
    let mut stream = resp.bytes_stream();
    use std::io::Write;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("reading download chunk")?;
        file.write_all(&chunk).context("writing download chunk")?;
        received += chunk.len() as u64;
        // Throttle events to roughly every 2 MB to avoid flooding the webview.
        if received - last_emit >= 2 * 1024 * 1024 {
            last_emit = received;
            let pct = if total > 0 {
                (received as f32 / total as f32) * 100.0
            } else {
                0.0
            };
            on_progress(ModelProgress {
                id: spec.id.to_string(),
                phase: "download".into(),
                received,
                total,
                pct,
                message: None,
            });
        }
    }
    file.flush().ok();
    drop(file);
    std::fs::rename(part, archive).with_context(|| "finalizing download")?;

    // --- Extract phase (blocking; runs off the async runtime) ---
    on_progress(ModelProgress {
        id: spec.id.to_string(),
        phase: "extract".into(),
        received: total,
        total,
        pct: 100.0,
        message: Some("Extracting…".into()),
    });
    let archive_owned = archive.to_path_buf();
    let dir_owned = dir.to_path_buf();
    tokio::task::spawn_blocking(move || extract_tar_bz2(&archive_owned, &dir_owned))
        .await
        .context("extract task panicked")??;

    // --- Verify ---
    if !is_installed(spec) {
        return Err(anyhow!(
            "extraction finished but expected files are missing for {}",
            spec.id
        ));
    }
    Ok(())
}

/// Extract a `.tar.bz2` archive into `dest`.
fn extract_tar_bz2(archive: &Path, dest: &Path) -> Result<()> {
    let file = File::open(archive).with_context(|| format!("opening {}", archive.display()))?;
    let decompressor = bzip2::read::BzDecoder::new(file);
    let mut tar = tar::Archive::new(decompressor);
    tar.unpack(dest)
        .with_context(|| format!("extracting into {}", dest.display()))?;
    Ok(())
}

/// Delete a downloaded model from disk.
pub fn delete(id: &str) -> Result<()> {
    let spec = spec(id).ok_or_else(|| anyhow!("unknown model: {id}"))?;
    let dir = model_dir(spec)?;
    if dir.is_dir() {
        std::fs::remove_dir_all(&dir).with_context(|| format!("removing {}", dir.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_has_default_models() {
        assert!(spec("parakeet-tdt-0.6b-v2-int8").is_some());
        assert!(spec("kokoro-int8-multi-lang-v1_0").is_some());
        assert!(spec("does-not-exist").is_none());
    }

    #[test]
    fn default_models_have_expected_kinds() {
        assert_eq!(spec("parakeet-tdt-0.6b-v2-int8").unwrap().kind, ModelKind::Stt);
        assert_eq!(spec("kokoro-int8-multi-lang-v1_0").unwrap().kind, ModelKind::Tts);
    }

    #[test]
    fn model_dir_is_under_voice_models() {
        let spec = spec("kokoro-int8-multi-lang-v1_0").unwrap();
        let dir = model_dir(spec).unwrap();
        assert!(dir.ends_with("kokoro-int8-multi-lang-v1_0"));
        assert!(dir.to_string_lossy().contains("voice-models"));
    }

    #[test]
    fn resolve_unknown_model_errors() {
        assert!(resolve_stt_paths("does-not-exist").is_err());
        assert!(resolve_tts_paths("does-not-exist").is_err());
    }

    #[test]
    fn resolve_in_missing_dir_errors() {
        let dir = std::env::temp_dir()
            .join(format!("voice-missing-{}-{}", std::process::id(), line!()));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(resolve_stt_paths_in(&dir).is_err());
        assert!(resolve_tts_paths_in(&dir).is_err());
    }

    #[test]
    fn resolve_in_with_int8_files_ok() {
        let dir = std::env::temp_dir()
            .join(format!("voice-resolve-{}-{}", std::process::id(), line!()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // STT (int8-named) files.
        for f in ["encoder.int8.onnx", "decoder.int8.onnx", "joiner.int8.onnx", "tokens.txt"] {
            std::fs::write(dir.join(f), b"x").unwrap();
        }
        let stt = resolve_stt_paths_in(&dir).unwrap();
        assert!(stt.encoder.ends_with("encoder.int8.onnx"));
        assert!(stt.joiner.ends_with("joiner.int8.onnx"));

        // TTS (Kokoro) files, incl. two lexicons and the espeak data dir.
        std::fs::write(dir.join("model.int8.onnx"), b"x").unwrap();
        std::fs::write(dir.join("voices.bin"), b"x").unwrap();
        std::fs::write(dir.join("lexicon-us-en.txt"), b"x").unwrap();
        std::fs::write(dir.join("lexicon-zh.txt"), b"x").unwrap();
        std::fs::create_dir_all(dir.join("espeak-ng-data")).unwrap();
        let tts = resolve_tts_paths_in(&dir).unwrap();
        assert!(tts.model.ends_with("model.int8.onnx"));
        assert_eq!(tts.lexicon.len(), 2);
        assert!(tts.dict_dir.is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolve_tts_prefers_int8_over_fp32() {
        let dir = std::env::temp_dir()
            .join(format!("voice-prefer-{}-{}", std::process::id(), line!()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("model.onnx"), b"x").unwrap();
        std::fs::write(dir.join("model.int8.onnx"), b"x").unwrap();
        std::fs::write(dir.join("voices.bin"), b"x").unwrap();
        std::fs::write(dir.join("tokens.txt"), b"x").unwrap();
        std::fs::create_dir_all(dir.join("espeak-ng-data")).unwrap();
        let tts = resolve_tts_paths_in(&dir).unwrap();
        assert!(tts.model.ends_with("model.int8.onnx"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn all_status_covers_catalog() {
        let s = all_status();
        assert_eq!(s.len(), CATALOG.len());
    }
}
