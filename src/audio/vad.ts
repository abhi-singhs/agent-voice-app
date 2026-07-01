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
  /**
   * Attack time (ms): speech must stay above `threshold` for this long before
   * `onSpeechStart` fires. Defaults to 0 (fire on the first buffer over
   * threshold). A non-zero value makes onset detection resistant to short
   * echo blips — used for barge-in while the agent is speaking.
   */
  onsetMs?: number;
}

const DEFAULTS = {
  threshold: 0.014,
  silenceMs: 900,
  minSpeechMs: 250,
  onsetMs: 0,
};

export class Vad {
  private readonly threshold: number;
  private readonly silenceMs: number;
  private readonly minSpeechMs: number;
  private readonly onsetMs: number;

  private speaking = false;
  private everSpoke = false;
  private lastVoiceTs = 0;
  private speechStartTs = 0;
  /** When the current above-threshold run began (for the onset/attack gate). */
  private candidateTs: number | null = null;
  private ended = false;

  onSpeechStart?: () => void;
  onSpeechEnd?: () => void;

  constructor(opts: VadOptions = {}) {
    this.threshold = opts.threshold ?? DEFAULTS.threshold;
    this.silenceMs = opts.silenceMs ?? DEFAULTS.silenceMs;
    this.minSpeechMs = opts.minSpeechMs ?? DEFAULTS.minSpeechMs;
    this.onsetMs = opts.onsetMs ?? DEFAULTS.onsetMs;
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
        // Require the level to stay above threshold for `onsetMs` before we
        // declare speech, so brief spikes (e.g. speaker echo) don't trigger.
        if (this.candidateTs === null) this.candidateTs = now;
        if (now - this.candidateTs >= this.onsetMs) {
          this.speaking = true;
          if (!this.everSpoke) {
            this.everSpoke = true;
            this.speechStartTs = now;
            this.onSpeechStart?.();
          }
        }
      }
    } else {
      // Dropped below threshold — reset the onset candidate…
      this.candidateTs = null;
      if (this.speaking) {
        const longEnough = now - this.speechStartTs >= this.minSpeechMs;
        if (longEnough && now - this.lastVoiceTs >= this.silenceMs) {
          this.speaking = false;
          this.ended = true;
          this.onSpeechEnd?.();
        }
      }
    }
  }
}
