import { useCallback, useEffect, useMemo, useState } from "react";

import {
  deleteModel,
  downloadModel,
  listLocalVoices,
  modelStatus,
  onModelProgress,
  setLocalVoice,
  setSttProvider,
  setTtsProvider,
  voiceSettings,
  type LocalVoice,
  type ModelProgress,
  type ModelStatus,
  type VoiceProvider,
  type VoiceSettings,
} from "../ipc";

/**
 * "Voice engine" card: choose between on-device models (default, free, private)
 * and ElevenLabs for STT and TTS, download/manage the local models, and pick the
 * local voice + speaking rate.
 */
export function VoiceEngineCard() {
  const [settings, setSettings] = useState<VoiceSettings | null>(null);
  const [models, setModels] = useState<ModelStatus[]>([]);
  const [voices, setVoices] = useState<LocalVoice[]>([]);
  const [progress, setProgress] = useState<Record<string, ModelProgress>>({});
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [speed, setSpeed] = useState(1);

  const refreshModels = useCallback(async () => {
    try {
      setModels(await modelStatus());
    } catch (e) {
      setErr(errMessage(e));
    }
  }, []);

  useEffect(() => {
    void (async () => {
      try {
        const s = await voiceSettings();
        setSettings(s);
        setSpeed(s.local.speed);
      } catch (e) {
        setErr(errMessage(e));
      }
      void refreshModels();
      try {
        setVoices(await listLocalVoices());
      } catch {
        /* voice list is best-effort */
      }
    })();
  }, [refreshModels]);

  // Subscribe to download progress; refresh statuses when a download finishes.
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void onModelProgress((p) => {
      setProgress((prev) => ({ ...prev, [p.id]: p }));
      if (p.phase === "done") {
        void refreshModels();
        setProgress((prev) => {
          const next = { ...prev };
          delete next[p.id];
          return next;
        });
      }
    }).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, [refreshModels]);

  const sttModel = useMemo(() => models.find((m) => m.kind === "stt"), [models]);
  const ttsModel = useMemo(() => models.find((m) => m.kind === "tts"), [models]);

  const setStt = async (provider: VoiceProvider) => {
    setBusy(true);
    try {
      setSettings(await setSttProvider(provider));
      setErr(null);
    } catch (e) {
      setErr(errMessage(e));
    } finally {
      setBusy(false);
    }
  };

  const setTts = async (provider: VoiceProvider) => {
    setBusy(true);
    try {
      setSettings(await setTtsProvider(provider));
      setErr(null);
    } catch (e) {
      setErr(errMessage(e));
    } finally {
      setBusy(false);
    }
  };

  const doDownload = async (id: string) => {
    setProgress((prev) => ({
      ...prev,
      [id]: { id, phase: "download", received: 0, total: 0, pct: 0 },
    }));
    try {
      await downloadModel(id);
    } catch (e) {
      setProgress((prev) => ({
        ...prev,
        [id]: { id, phase: "error", received: 0, total: 0, pct: 0, message: errMessage(e) },
      }));
    }
  };

  const doDelete = async (id: string) => {
    setBusy(true);
    try {
      setModels(await deleteModel(id));
      setErr(null);
    } catch (e) {
      setErr(errMessage(e));
    } finally {
      setBusy(false);
    }
  };

  const chosenVoiceSid = settings?.local.ttsVoiceSid ?? 26;
  const changeVoice = async (sid: number) => {
    try {
      setSettings(await setLocalVoice({ sid }));
    } catch (e) {
      setErr(errMessage(e));
    }
  };

  const commitSpeed = async (value: number) => {
    try {
      setSettings(await setLocalVoice({ speed: value }));
    } catch (e) {
      setErr(errMessage(e));
    }
  };

  const sttLocal = settings?.sttProvider === "local";
  const ttsLocal = settings?.ttsProvider === "local";
  const sttReady = !sttLocal || !!sttModel?.installed;
  const ttsReady = !ttsLocal || !!ttsModel?.installed;
  const ready = !!settings && sttReady && ttsReady;

  return (
    <div className="setup-card">
      <div className="setup-card__head">
        <span className={`setup-dot ${ready ? "setup-dot--ok" : "setup-dot--warn"}`} />
        <h2 className="setup-card__title">Voice engine</h2>
      </div>
      <p className="setup-card__body setup-muted">
        On-device models are the default — free, private, and offline. Switch to
        ElevenLabs any time.
      </p>

      {/* Speech-to-text */}
      <div className="setup-subsection">
        <div className="setup-subhead">
          <span className="setup-label">Speech-to-text</span>
          <Segmented
            value={settings?.sttProvider}
            disabled={busy}
            onChange={(p) => void setStt(p)}
          />
        </div>
        {sttLocal && sttModel && (
          <ModelRow
            model={sttModel}
            progress={progress[sttModel.id]}
            busy={busy}
            onDownload={() => void doDownload(sttModel.id)}
            onDelete={() => void doDelete(sttModel.id)}
          />
        )}
      </div>

      {/* Text-to-speech */}
      <div className="setup-subsection">
        <div className="setup-subhead">
          <span className="setup-label">Text-to-speech</span>
          <Segmented
            value={settings?.ttsProvider}
            disabled={busy}
            onChange={(p) => void setTts(p)}
          />
        </div>
        {ttsLocal && ttsModel && (
          <ModelRow
            model={ttsModel}
            progress={progress[ttsModel.id]}
            busy={busy}
            onDownload={() => void doDownload(ttsModel.id)}
            onDelete={() => void doDelete(ttsModel.id)}
          />
        )}
        {ttsLocal && voices.length > 0 && (
          <>
            <label className="setup-field">
              <span className="setup-label">Voice</span>
              <select
                className="setup-select"
                value={chosenVoiceSid}
                onChange={(e) => void changeVoice(Number(e.target.value))}
              >
                {voices.map((v) => (
                  <option key={v.sid} value={v.sid}>
                    {v.name} — {v.locale}, {v.gender}
                  </option>
                ))}
              </select>
            </label>
            <label className="setup-field">
              <span className="setup-label">Speed — {speed.toFixed(2)}×</span>
              <input
                className="setup-range"
                type="range"
                min={0.5}
                max={2}
                step={0.05}
                value={speed}
                onChange={(e) => setSpeed(Number(e.target.value))}
                onPointerUp={() => void commitSpeed(speed)}
                onKeyUp={() => void commitSpeed(speed)}
                onBlur={() => void commitSpeed(speed)}
              />
            </label>
          </>
        )}
      </div>

      {err && <p className="setup-err">{err}</p>}
      <p className="setup-muted setup-hint">
        Models download to <code>~/.copilot/voice-models/</code>. ElevenLabs (when
        selected) uses the key in the card below.
      </p>
    </div>
  );
}

/** Local ⟷ ElevenLabs segmented toggle. */
function Segmented({
  value,
  disabled,
  onChange,
}: {
  value: VoiceProvider | undefined;
  disabled?: boolean;
  onChange: (p: VoiceProvider) => void;
}) {
  return (
    <div className="seg" role="group">
      <button
        type="button"
        className={`seg-btn ${value === "local" ? "seg-btn--active" : ""}`}
        disabled={disabled}
        onClick={() => onChange("local")}
      >
        Local
      </button>
      <button
        type="button"
        className={`seg-btn ${value === "elevenlabs" ? "seg-btn--active" : ""}`}
        disabled={disabled}
        onClick={() => onChange("elevenlabs")}
      >
        ElevenLabs
      </button>
    </div>
  );
}

/** One local model's status: installed (with Delete) or download control. */
function ModelRow({
  model,
  progress,
  busy,
  onDownload,
  onDelete,
}: {
  model: ModelStatus;
  progress: ModelProgress | undefined;
  busy: boolean;
  onDownload: () => void;
  onDelete: () => void;
}) {
  const downloading =
    !!progress && (progress.phase === "download" || progress.phase === "extract");
  const errored = progress?.phase === "error";

  return (
    <div className="setup-model">
      <div className="setup-model__row">
        <span className="setup-model__name">
          {model.display_name}
          <span className="setup-muted"> · ~{model.approx_mb} MB</span>
        </span>
        {model.installed ? (
          <button
            className="setup-btn setup-btn--ghost setup-btn--sm"
            onClick={onDelete}
            disabled={busy || downloading}
          >
            Delete
          </button>
        ) : (
          !downloading && (
            <button
              className="setup-btn setup-btn--sm"
              onClick={onDownload}
              disabled={busy}
            >
              Download
            </button>
          )
        )}
      </div>

      {downloading && (
        <div className="setup-progress" aria-label="download progress">
          <div
            className="setup-progress__bar"
            style={{ width: `${Math.max(2, Math.min(100, progress.pct))}%` }}
          />
          <span className="setup-progress__label">
            {progress.phase === "extract"
              ? "Extracting…"
              : progress.total > 0
                ? `${fmtMb(progress.received)} / ${fmtMb(progress.total)} (${progress.pct.toFixed(0)}%)`
                : `${fmtMb(progress.received)}…`}
          </span>
        </div>
      )}
      {errored && <p className="setup-err">{progress?.message ?? "Download failed."}</p>}
      {model.installed && !downloading && (
        <span className="setup-muted setup-hint">Installed ✓</span>
      )}
    </div>
  );
}

function fmtMb(bytes: number): string {
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function errMessage(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return String(e);
}
