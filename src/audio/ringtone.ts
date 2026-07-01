// A self-contained ringtone using the Web Audio API — no audio asset required.
//
// Plays a classic two-tone telephone ring on a repeating cadence until stopped.
// If the AudioContext can't start (e.g. autoplay policy), it fails silently and
// the visual ring UI still conveys the incoming call.

export class Ringer {
  private ctx: AudioContext | null = null;
  private timer: number | null = null;
  private active = false;

  start(): void {
    if (this.active) return;
    this.active = true;
    try {
      this.ctx = new AudioContext();
    } catch {
      this.ctx = null;
      return;
    }
    const tick = () => {
      if (!this.active) return;
      this.burst();
    };
    tick();
    this.timer = window.setInterval(tick, 3200);
  }

  stop(): void {
    this.active = false;
    if (this.timer !== null) {
      clearInterval(this.timer);
      this.timer = null;
    }
    if (this.ctx) {
      void this.ctx.close().catch(() => {});
      this.ctx = null;
    }
  }

  private burst(): void {
    const ctx = this.ctx;
    if (!ctx) return;
    if (ctx.state === "suspended") void ctx.resume().catch(() => {});

    const now = ctx.currentTime;
    const gain = ctx.createGain();
    gain.gain.value = 0;
    gain.connect(ctx.destination);

    const o1 = ctx.createOscillator();
    const o2 = ctx.createOscillator();
    o1.type = "sine";
    o2.type = "sine";
    o1.frequency.value = 440;
    o2.frequency.value = 480;
    o1.connect(gain);
    o2.connect(gain);

    // Two ~0.4s pulses with a short gap — the familiar "ring ring".
    const env: [number, number][] = [
      [0.0, 0.0],
      [0.02, 0.12],
      [0.4, 0.12],
      [0.42, 0.0],
      [0.6, 0.0],
      [0.62, 0.12],
      [1.0, 0.12],
      [1.02, 0.0],
    ];
    for (const [t, v] of env) gain.gain.setValueAtTime(v, now + t);

    o1.start(now);
    o2.start(now);
    o1.stop(now + 1.1);
    o2.stop(now + 1.1);
  }
}
