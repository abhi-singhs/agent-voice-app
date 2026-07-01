// Energy-based voice activity detection.
//
// Pure logic: callers feed it per-buffer RMS levels with timestamps; it fires
// callbacks when speech starts and when a trailing silence ends the turn. No
// audio APIs here, so it is trivially unit-testable.

export interface VadOptions {
  /** RMS level above which a buffer counts as speech. */
  threshold?: number;
  /** Trailing silence (ms) after speech that ends the turn. */
  silenceMs?: number;
  /** Minimum speech duration (ms) before a silence can end the turn. */
  minSpeechMs?: number;
}

const DEFAULTS = {
  threshold: 0.014,
  silenceMs: 900,
  minSpeechMs: 250,
};

export class Vad {
  private readonly threshold: number;
  private readonly silenceMs: number;
  private readonly minSpeechMs: number;

  private speaking = false;
  private everSpoke = false;
  private lastVoiceTs = 0;
  private speechStartTs = 0;
  private ended = false;

  onSpeechStart?: () => void;
  onSpeechEnd?: () => void;

  constructor(opts: VadOptions = {}) {
    this.threshold = opts.threshold ?? DEFAULTS.threshold;
    this.silenceMs = opts.silenceMs ?? DEFAULTS.silenceMs;
    this.minSpeechMs = opts.minSpeechMs ?? DEFAULTS.minSpeechMs;
  }

  /** True once any speech has been detected. */
  get hasSpoken(): boolean {
    return this.everSpoke;
  }

  /** Feed one RMS reading at time `now` (ms, monotonic). */
  push(rms: number, now: number): void {
    if (this.ended) return;
    if (rms >= this.threshold) {
      this.lastVoiceTs = now;
      if (!this.speaking) {
        this.speaking = true;
        if (!this.everSpoke) {
          this.everSpoke = true;
          this.speechStartTs = now;
          this.onSpeechStart?.();
        }
      }
    } else if (this.speaking) {
      const longEnough = now - this.speechStartTs >= this.minSpeechMs;
      if (longEnough && now - this.lastVoiceTs >= this.silenceMs) {
        this.speaking = false;
        this.ended = true;
        this.onSpeechEnd?.();
      }
    }
  }
}
