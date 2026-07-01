import { describe, expect, it } from "vitest";

import { trimToLastMs } from "./mic";

/** Build `n` chunks of `size` samples each, filled with the chunk index. */
function chunks(sizes: number[]): Float32Array[] {
  return sizes.map((size, i) => new Float32Array(size).fill(i));
}

describe("trimToLastMs", () => {
  it("returns nothing for a non-positive window", () => {
    expect(trimToLastMs(chunks([10, 10]), 1000, 0)).toEqual([]);
    expect(trimToLastMs(chunks([10, 10]), 1000, -5)).toEqual([]);
  });

  it("keeps whole trailing buffers until the sample budget is met", () => {
    // 1000 Hz, 100ms → 100-sample budget. Buffers of 40 samples each.
    const cs = chunks([40, 40, 40, 40]); // indices 0..3
    const kept = trimToLastMs(cs, 1000, 100);
    // From the end: 40 (<100), 80 (<100), 120 (>=100) → last three buffers.
    expect(kept).toHaveLength(3);
    expect(kept[0][0]).toBe(1);
    expect(kept[2][0]).toBe(3);
  });

  it("keeps everything when the total is shorter than the window", () => {
    const cs = chunks([20, 20]);
    const kept = trimToLastMs(cs, 1000, 1000); // budget 1000 samples > 40 total
    expect(kept).toHaveLength(2);
  });

  it("never splits a buffer (keeps at least the final one)", () => {
    const cs = chunks([500]);
    const kept = trimToLastMs(cs, 1000, 10); // budget 10 < 500
    expect(kept).toHaveLength(1);
    expect(kept[0]).toBe(cs[0]);
  });
});
