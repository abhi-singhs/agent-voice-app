import { describe, expect, it } from "vitest";

import { splitIntoSpeechChunks } from "./chunk";

describe("splitIntoSpeechChunks", () => {
  it("returns [] for empty or whitespace input", () => {
    expect(splitIntoSpeechChunks("")).toEqual([]);
    expect(splitIntoSpeechChunks("   \n  \t")).toEqual([]);
  });

  it("keeps a single short sentence as one chunk (preserves fast path)", () => {
    expect(splitIntoSpeechChunks("How are you?")).toEqual(["How are you?"]);
    expect(splitIntoSpeechChunks("Hello there")).toEqual(["Hello there"]);
  });

  it("treats ellipsis-only text as a single chunk", () => {
    expect(splitIntoSpeechChunks("Long answer…")).toEqual(["Long answer…"]);
    expect(splitIntoSpeechChunks("Wait...")).toEqual(["Wait..."]);
  });

  it("splits multiple long sentences into separate chunks", () => {
    const a = "This is the first fairly long sentence that stands on its own.";
    const b = "And here is a second sentence that is also long enough to be a chunk!";
    const chunks = splitIntoSpeechChunks(`${a} ${b}`);
    expect(chunks).toEqual([a, b]);
  });

  it("merges short sentences so chunks aren't choppy", () => {
    // Each fragment is well under MIN_CHARS, so they coalesce into one chunk.
    expect(splitIntoSpeechChunks("Hi. Yes. No.")).toEqual(["Hi. Yes. No."]);
  });

  it("keeps every chunk within the max length", () => {
    const long = `${"word ".repeat(120).trim()}.`; // ~600 chars, no sentence breaks
    const chunks = splitIntoSpeechChunks(long);
    expect(chunks.length).toBeGreaterThan(1);
    for (const c of chunks) expect(c.length).toBeLessThanOrEqual(240);
    // Round-trips to the same words (ignoring the split whitespace/period).
    expect(chunks.join(" ").replace(/\s+/g, " ")).toContain("word word");
  });

  it("does not split decimals or abbreviations", () => {
    expect(splitIntoSpeechChunks("Pi is 3.14 today")).toEqual(["Pi is 3.14 today"]);
    expect(splitIntoSpeechChunks("The U.S. economy")).toEqual(["The U.S. economy"]);
  });

  it("breaks on newlines", () => {
    expect(splitIntoSpeechChunks("First line\nSecond line")).toEqual([
      "First line Second line",
    ]);
  });
});
