// A single "listen" turn: capture the user's spoken reply and return the WAV
// for transcription. Combines the mic Recorder with the VAD and a set of
// timeouts, exposing a handle so the caller can force-finish (push-to-talk
// release) or cancel (hang-up / switch to typing).

import { Recorder } from "./mic";
import { Vad } from "./vad";

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
}

export interface ListenHandle {
  result: Promise<ListenResult>;
  /** Stop now and transcribe whatever was captured. */
  finish: () => void;
  /** Abort without transcribing (resolves `aborted`). */
  cancel: () => void;
}

const DEFAULT_START_TIMEOUT_MS = 20000;
const DEFAULT_MAX_MS = 20000;
const LEVEL_SCALE = 8; // maps RMS (~0..0.15) to a 0..1 UI meter

export function startListening(opts: ListenOptions = {}): ListenHandle {
  const startTimeoutMs = opts.startTimeoutMs ?? DEFAULT_START_TIMEOUT_MS;
  const maxMs = opts.maxMs ?? DEFAULT_MAX_MS;
  const recorder = new Recorder();
  const vad = new Vad();

  let done = false;
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

  vad.onSpeechStart = () => {
    if (startTimer !== undefined) {
      clearTimeout(startTimer);
      startTimer = undefined;
    }
  };
  vad.onSpeechEnd = () => stopAndTranscribe();

  recorder
    .start((rms) => {
      opts.onLevel?.(Math.min(1, rms * LEVEL_SCALE));
      if (!opts.manual) vad.push(rms, performance.now());
    })
    .then(() => {
      if (done) {
        // Cancelled before the mic finished opening.
        recorder.stop();
        return;
      }
      if (!opts.manual) {
        startTimer = window.setTimeout(() => {
          if (!vad.hasSpoken) {
            recorder.stop();
            settle({ status: "no_speech" });
          }
        }, startTimeoutMs);
        maxTimer = window.setTimeout(() => stopAndTranscribe(), maxMs);
      }
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
  };
}
