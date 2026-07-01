// Microphone capture for speech-to-text.
//
// Captures raw PCM via getUserMedia + AudioContext (ScriptProcessorNode, which
// WKWebView supports reliably), then downsamples to 16 kHz mono and encodes a
// 16-bit PCM WAV. This avoids MediaRecorder codec gaps on macOS WebViews. The
// per-buffer RMS level is surfaced for VAD and UI metering.

const TARGET_RATE = 16000;
const BUFFER_SIZE = 4096;

type ScriptProcessorFactory = (
  bufferSize: number,
  inputChannels: number,
  outputChannels: number,
) => ScriptProcessorNode;

/** Result of a completed recording. */
export interface Recording {
  wavBase64: string;
  durationMs: number;
}

export class Recorder {
  private ctx: AudioContext | null = null;
  private stream: MediaStream | null = null;
  private source: MediaStreamAudioSourceNode | null = null;
  private processor: ScriptProcessorNode | null = null;
  private sink: GainNode | null = null;
  private chunks: Float32Array[] = [];
  private srcRate = TARGET_RATE;
  private onLevel?: (rms: number) => void;
  private _started = false;

  get started(): boolean {
    return this._started;
  }

  /** Request mic access and begin capturing. Rejects if permission denied. */
  async start(onLevel?: (rms: number) => void): Promise<void> {
    this.onLevel = onLevel;
    this.stream = await navigator.mediaDevices.getUserMedia({
      audio: { channelCount: 1, echoCancellation: true, noiseSuppression: true },
    });
    const Ctor: typeof AudioContext =
      window.AudioContext ?? (window as unknown as { webkitAudioContext: typeof AudioContext }).webkitAudioContext;
    this.ctx = new Ctor();
    this.srcRate = this.ctx.sampleRate;
    this.source = this.ctx.createMediaStreamSource(this.stream);
    const createProcessor = this.ctx.createScriptProcessor as unknown as ScriptProcessorFactory;
    this.processor = createProcessor.call(this.ctx, BUFFER_SIZE, 1, 1);
    this.processor.onaudioprocess = (e: AudioProcessingEvent) => {
      const input = e.inputBuffer.getChannelData(0);
      const copy = new Float32Array(input.length);
      copy.set(input);
      this.chunks.push(copy);
      let sum = 0;
      for (let i = 0; i < copy.length; i++) sum += copy[i] * copy[i];
      this.onLevel?.(Math.sqrt(sum / copy.length));
    };
    // Route through a muted gain node so the processor runs without feedback.
    this.sink = this.ctx.createGain();
    this.sink.gain.value = 0;
    this.source.connect(this.processor);
    this.processor.connect(this.sink);
    this.sink.connect(this.ctx.destination);
    this._started = true;
  }

  /** Stop capturing and return the encoded WAV, or null if nothing was captured. */
  stop(): Recording | null {
    if (!this._started) return null;
    this._started = false;
    try {
      this.processor?.disconnect();
      this.source?.disconnect();
      this.sink?.disconnect();
      this.stream?.getTracks().forEach((t) => t.stop());
      void this.ctx?.close();
    } catch {
      // best-effort teardown
    }
    const merged = mergeChunks(this.chunks);
    this.chunks = [];
    if (merged.length === 0) return null;
    const down = downsample(merged, this.srcRate, TARGET_RATE);
    const wav = encodeWav(down, TARGET_RATE);
    return {
      wavBase64: bytesToBase64(wav),
      durationMs: (merged.length / this.srcRate) * 1000,
    };
  }
}

function mergeChunks(chunks: Float32Array[]): Float32Array {
  let total = 0;
  for (const c of chunks) total += c.length;
  const out = new Float32Array(total);
  let offset = 0;
  for (const c of chunks) {
    out.set(c, offset);
    offset += c.length;
  }
  return out;
}

/** Linear-interpolation downsample from srcRate to dstRate. */
function downsample(input: Float32Array, srcRate: number, dstRate: number): Float32Array {
  if (dstRate >= srcRate) return input;
  const ratio = srcRate / dstRate;
  const outLength = Math.floor(input.length / ratio);
  const out = new Float32Array(outLength);
  for (let i = 0; i < outLength; i++) {
    const pos = i * ratio;
    const idx = Math.floor(pos);
    const frac = pos - idx;
    const a = input[idx] ?? 0;
    const b = input[idx + 1] ?? a;
    out[i] = a + (b - a) * frac;
  }
  return out;
}

/** Encode mono Float32 samples as a 16-bit PCM WAV file. */
function encodeWav(samples: Float32Array, rate: number): Uint8Array {
  const bytesPerSample = 2;
  const dataSize = samples.length * bytesPerSample;
  const buffer = new ArrayBuffer(44 + dataSize);
  const view = new DataView(buffer);

  writeAscii(view, 0, "RIFF");
  view.setUint32(4, 36 + dataSize, true);
  writeAscii(view, 8, "WAVE");
  writeAscii(view, 12, "fmt ");
  view.setUint32(16, 16, true); // PCM chunk size
  view.setUint16(20, 1, true); // audio format = PCM
  view.setUint16(22, 1, true); // channels = mono
  view.setUint32(24, rate, true);
  view.setUint32(28, rate * bytesPerSample, true); // byte rate
  view.setUint16(32, bytesPerSample, true); // block align
  view.setUint16(34, 16, true); // bits per sample
  writeAscii(view, 36, "data");
  view.setUint32(40, dataSize, true);

  let offset = 44;
  for (let i = 0; i < samples.length; i++) {
    const s = Math.max(-1, Math.min(1, samples[i]));
    view.setInt16(offset, s < 0 ? s * 0x8000 : s * 0x7fff, true);
    offset += 2;
  }
  return new Uint8Array(buffer);
}

function writeAscii(view: DataView, offset: number, text: string): void {
  for (let i = 0; i < text.length; i++) view.setUint8(offset + i, text.charCodeAt(i));
}

function bytesToBase64(bytes: Uint8Array): string {
  let binary = "";
  const chunk = 0x8000;
  for (let i = 0; i < bytes.length; i += chunk) {
    binary += String.fromCharCode(...bytes.subarray(i, i + chunk));
  }
  return btoa(binary);
}
