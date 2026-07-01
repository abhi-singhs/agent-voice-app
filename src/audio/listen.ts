// A single "listen" turn: capture the user's spoken reply and return the WAV
// for transcription. Combines the mic Recorder with the VAD and a set of
// timeouts, exposing a handle so the caller can force-finish (push-to-talk
// release) or cancel (hang-up / switch to typing).
//
// Barge mode: start capturing *while the agent is still speaking* but only watch
// for speech onset with an echo-resistant VAD. When the user starts talking we
// fire `onBarge` (so the caller can stop TTS) and promote to a normal capture
// turn, dropping the buffered TTS echo but keeping a short pre-roll. The caller
// can also promote explicitly via `beginCapture()` when the agent finishes.

import { Recorder } from "./mic";
import { Vad, type VadOptions } from "./vad";

export type ListenResult =
  | { status: "ok"; wavBase64: string; durationMs: number }
  | { status: "no_speech" }
  | { status: "aborted" }
  | { status: "error"; error: string };

export interface ListenOptions {
  /** Max wait (ms) for speech to begin before giving up with no_speech. */
  startTimeoutMs?: number;
  /** Hard cap (ms) on a single utterance. */
  maxMs?: number;
  /** Manual mode (push-to-talk): no VAD/timeout auto-stop; caller calls finish(). */
  manual?: boolean;
  /** Optional 0..1 level callback for a UI meter. */
  onLevel?: (level: number) => void;
  /**
   * Start in "armed" barge mode: capture audio but only watch for speech onset
   * (via an echo-resistant VAD). Promote to a real capture turn with
   * `beginCapture()` or automatically when the user starts speaking.
   */
  barge?: boolean;
  /** Fired once when the user starts speaking during armed barge mode. */
  onBarge?: () => void;
  /** VAD overrides for onset detection while armed (echo-resistant defaults). */
  bargeVad?: VadOptions;
}

export interface ListenHandle {
  result: Promise<ListenResult>;
  /** Stop now and transcribe whatever was captured. */
  finish: () => void;
  /** Abort without transcribing (resolves `aborted`). */
  cancel: () => void;
  /**
   * Promote an armed barge listen into a normal capture turn (idempotent).
   * Drops the buffered TTS echo (keeping a short pre-roll) and arms the
   * no_speech / max-duration timers. No-op once already capturing.
   */
  beginCapture: () => void;
}

const DEFAULT_START_TIMEOUT_MS = 20000;
const DEFAULT_MAX_MS = 20000;
const LEVEL_SCALE = 8; // maps RMS (~0..0.15) to a 0..1 UI meter
const BARGE_PRE_ROLL_MS = 300; // audio kept before the barge point (utterance onset)
/** Echo-resistant onset detection while the agent is still speaking. */
const BARGE_VAD_DEFAULTS: VadOptions = { threshold: 0.03, onsetMs: 300 };

export function startListening(opts: ListenOptions = {}): ListenHandle {
  const startTimeoutMs = opts.startTimeoutMs ?? DEFAULT_START_TIMEOUT_MS;
  const maxMs = opts.maxMs ?? DEFAULT_MAX_MS;
  const recorder = new Recorder();
  const vad = new Vad(); // normal turn-end detection (used once capturing)
  const bargeVad = new Vad({ ...BARGE_VAD_DEFAULTS, ...opts.bargeVad });

  let done = false;
  // Non-barge turns capture immediately; barge turns start armed.
  let capturing = !opts.barge;
  let started = false; // recorder mic actually opened
  let timersArmed = false;

  let resolve!: (r: ListenResult) => void;
  const result = new Promise<ListenResult>((r) => (resolve = r));

  let startTimer: number | undefined;
  let maxTimer: number | undefined;

  const clearTimers = () => {
    if (startTimer !== undefined) clearTimeout(startTimer);
    if (maxTimer !== undefined) clearTimeout(maxTimer);
  };

  const settle = (r: ListenResult) => {
    if (done) return;
    done = true;
    clearTimers();
    resolve(r);
  };

  const stopAndTranscribe = () => {
    if (done) return;
    const rec = recorder.stop();
    if (!rec) {
      settle({ status: "no_speech" });
      return;
    }
    settle({ status: "ok", wavBase64: rec.wavBase64, durationMs: rec.durationMs });
  };

  // Arm the no_speech + max-duration timers, once (and only while capturing).
  const armTimers = () => {
    if (done || timersArmed || opts.manual || !started || !capturing) return;
    timersArmed = true;
    startTimer = window.setTimeout(() => {
      if (!vad.hasSpoken) {
        recorder.stop();
        settle({ status: "no_speech" });
      }
    }, startTimeoutMs);
    maxTimer = window.setTimeout(() => stopAndTranscribe(), maxMs);
  };

  const beginCapture = () => {
    if (done || capturing) return;
    capturing = true;
    // Drop the agent's TTS echo captured while armed, keep the utterance onset.
    recorder.keepLastMs(BARGE_PRE_ROLL_MS);
    armTimers();
  };

  // Normal turn-end detection (active once capturing).
  vad.onSpeechStart = () => {
    if (startTimer !== undefined) {
      clearTimeout(startTimer);
      startTimer = undefined;
    }
  };
  vad.onSpeechEnd = () => stopAndTranscribe();

  // Barge onset detection (active while armed).
  bargeVad.onSpeechStart = () => {
    if (done || capturing) return;
    opts.onBarge?.();
    beginCapture();
  };

  recorder
    .start((rms) => {
      opts.onLevel?.(Math.min(1, rms * LEVEL_SCALE));
      if (opts.manual) return;
      const now = performance.now();
      if (capturing) vad.push(rms, now);
      else bargeVad.push(rms, now);
    })
    .then(() => {
      if (done) {
        // Cancelled before the mic finished opening.
        recorder.stop();
        return;
      }
      started = true;
      // Non-barge turns (or barge turns already promoted) arm now; a still-armed
      // barge turn defers until beginCapture().
      armTimers();
    })
    .catch((err: unknown) => {
      settle({ status: "error", error: err instanceof Error ? err.message : String(err) });
    });

  return {
    result,
    finish: () => stopAndTranscribe(),
    cancel: () => {
      if (done) return;
      try {
        recorder.stop();
      } catch {
        // ignore
      }
      settle({ status: "aborted" });
    },
    beginCapture,
  };
}
