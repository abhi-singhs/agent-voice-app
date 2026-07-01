import { describe, expect, it, vi } from "vitest";

import { Vad } from "./vad";

describe("Vad", () => {
  it("fires onSpeechStart when the level crosses the threshold", () => {
    const vad = new Vad({ threshold: 0.02 });
    const onStart = vi.fn();
    vad.onSpeechStart = onStart;

    vad.push(0.001, 0); // silence
    expect(onStart).not.toHaveBeenCalled();
    vad.push(0.05, 100); // speech
    expect(onStart).toHaveBeenCalledTimes(1);
    expect(vad.hasSpoken).toBe(true);
  });

  it("ends the turn after trailing silence", () => {
    const vad = new Vad({ threshold: 0.02, silenceMs: 500, minSpeechMs: 100 });
    const onEnd = vi.fn();
    vad.onSpeechEnd = onEnd;

    vad.push(0.05, 0); // speech start
    vad.push(0.05, 200); // still speaking (past minSpeechMs)
    vad.push(0.0, 300); // silence begins
    expect(onEnd).not.toHaveBeenCalled();
    vad.push(0.0, 850); // 550ms of silence → end
    expect(onEnd).toHaveBeenCalledTimes(1);
  });

  it("does not end before minimum speech duration", () => {
    const vad = new Vad({ threshold: 0.02, silenceMs: 200, minSpeechMs: 500 });
    const onEnd = vi.fn();
    vad.onSpeechEnd = onEnd;

    vad.push(0.05, 0); // brief blip
    vad.push(0.0, 100);
    vad.push(0.0, 400); // 300ms silence but speech was too short
    expect(onEnd).not.toHaveBeenCalled();
  });

  it("ignores input after the turn has ended", () => {
    const vad = new Vad({ threshold: 0.02, silenceMs: 200, minSpeechMs: 100 });
    const onStart = vi.fn();
    const onEnd = vi.fn();
    vad.onSpeechStart = onStart;
    vad.onSpeechEnd = onEnd;

    vad.push(0.05, 0);
    vad.push(0.05, 150);
    vad.push(0.0, 400); // end
    expect(onEnd).toHaveBeenCalledTimes(1);

    vad.push(0.05, 500); // should be ignored
    expect(onStart).toHaveBeenCalledTimes(1);
    expect(onEnd).toHaveBeenCalledTimes(1);
  });

  it("waits for the onset (attack) window before firing onSpeechStart", () => {
    const vad = new Vad({ threshold: 0.02, onsetMs: 300 });
    const onStart = vi.fn();
    vad.onSpeechStart = onStart;

    vad.push(0.05, 0); // first buffer over threshold — starts the onset clock
    expect(onStart).not.toHaveBeenCalled();
    vad.push(0.05, 200); // still within the onset window
    expect(onStart).not.toHaveBeenCalled();
    vad.push(0.05, 350); // sustained past onsetMs → fires
    expect(onStart).toHaveBeenCalledTimes(1);
    expect(vad.hasSpoken).toBe(true);
  });

  it("resets the onset window if the level dips below threshold (echo blips)", () => {
    const vad = new Vad({ threshold: 0.02, onsetMs: 300 });
    const onStart = vi.fn();
    vad.onSpeechStart = onStart;

    vad.push(0.05, 0); // blip starts onset clock
    vad.push(0.0, 100); // drops out → onset candidate reset
    vad.push(0.05, 200); // new onset clock starts here
    expect(onStart).not.toHaveBeenCalled();
    vad.push(0.05, 400); // only 200ms sustained since restart → still quiet
    expect(onStart).not.toHaveBeenCalled();
    vad.push(0.05, 550); // now 350ms since restart → fires
    expect(onStart).toHaveBeenCalledTimes(1);
  });
});
