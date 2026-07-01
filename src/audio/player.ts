// TTS playback via Web Audio. Decodes mp3 bytes returned by the backend `tts`
// command and plays them, resolving when playback finishes. Keeps a handle to
// the current source so playback can be stopped (used for hang-up / barge-in).

let ctx: AudioContext | null = null;
let current: AudioBufferSourceNode | null = null;
let currentAudioEl: HTMLAudioElement | null = null;

function audioContext(): AudioContext {
  if (!ctx) {
    const Ctor: typeof AudioContext =
      window.AudioContext ?? (window as unknown as { webkitAudioContext: typeof AudioContext }).webkitAudioContext;
    ctx = new Ctor();
  }
  return ctx;
}

/**
 * Play mp3 audio bytes, resolving when playback ends (or immediately if
 * playback cannot start). Any in-flight playback is stopped first.
 */
export async function playTts(data: ArrayBuffer): Promise<void> {
  stopPlayback();
  try {
    const context = audioContext();
    if (context.state === "suspended") await context.resume();
    // decodeAudioData may detach the buffer; hand it a copy.
    const buffer = await context.decodeAudioData(data.slice(0));
    await new Promise<void>((resolve) => {
      const src = context.createBufferSource();
      src.buffer = buffer;
      src.connect(context.destination);
      current = src;
      src.onended = () => {
        if (current === src) current = null;
        resolve();
      };
      src.start();
    });
  } catch (err) {
    // Fall back to an <audio> element (some WebViews decode mp3 more reliably).
    console.warn("Web Audio playback failed, falling back to <audio>:", err);
    await playViaElement(data);
  }
}

async function playViaElement(data: ArrayBuffer): Promise<void> {
  const blob = new Blob([data], { type: "audio/mpeg" });
  const url = URL.createObjectURL(blob);
  const el = new Audio(url);
  currentAudioEl = el;
  try {
    await new Promise<void>((resolve, reject) => {
      // Resolve on natural end *or* on pause, so stopPlayback() (which pauses)
      // lets the awaiting speak() promise settle for barge-in / hang-up.
      const done = () => resolve();
      el.onended = done;
      el.onpause = done;
      el.onerror = () => reject(el.error ?? new Error("audio element error"));
      void el.play().catch(reject);
    });
  } catch (err) {
    console.error("Audio element playback failed:", err);
  } finally {
    if (currentAudioEl === el) currentAudioEl = null;
    URL.revokeObjectURL(url);
  }
}

/** Stop any in-flight TTS playback immediately. */
export function stopPlayback(): void {
  if (current) {
    try {
      current.stop();
    } catch {
      // already stopped
    }
    current = null;
  }
  if (currentAudioEl) {
    try {
      currentAudioEl.pause();
    } catch {
      // ignore
    }
    currentAudioEl = null;
  }
}
