// Split reply text into speech-sized chunks for pipelined TTS. Chunks break on
// sentence boundaries, merge fragments too short to sound natural on their own,
// and subdivide any over-long sentence on clause/space boundaries. Keeping this
// a pure function makes it easy to unit-test and reuse from the call machine.

/** Merge sentences shorter than this so playback isn't choppy. */
const MIN_CHARS = 60;
/** Hard cap per chunk; longer sentences are subdivided. */
const MAX_CHARS = 240;

const TERMINATORS = ".!?…";
/** Characters that may trail a terminator and still belong to the sentence. */
const TRAILING = ".!?…\"')]";

/** Break text into sentence-ish pieces, keeping trailing punctuation. */
function splitSentences(text: string): string[] {
  const sentences: string[] = [];
  const chars = Array.from(text);
  let buf = "";

  const flush = () => {
    const s = buf.trim();
    if (s) sentences.push(s);
    buf = "";
  };

  for (let i = 0; i < chars.length; i++) {
    const c = chars[i];
    if (c === "\n") {
      // Hard line/paragraph boundary.
      flush();
      continue;
    }
    buf += c;
    if (TERMINATORS.includes(c)) {
      // Absorb any run of trailing terminators / closing quotes ("...", '?"').
      let j = i + 1;
      while (j < chars.length && TRAILING.includes(chars[j])) {
        buf += chars[j];
        j++;
      }
      // Only a real boundary when whitespace or end follows — this keeps
      // "U.S." and "3.14" intact (a letter/digit follows the dot).
      const next = chars[j];
      if (next === undefined || /\s/.test(next)) flush();
      i = j - 1;
    }
  }
  flush();
  return sentences;
}

/** Split a single over-long sentence into pieces no longer than MAX_CHARS. */
function splitLong(sentence: string): string[] {
  if (sentence.length <= MAX_CHARS) return [sentence];
  const parts: string[] = [];
  let rest = sentence;
  while (rest.length > MAX_CHARS) {
    const window = rest.slice(0, MAX_CHARS);
    // Prefer a clause boundary; fall back to the last space; else a hard cut.
    let cut = Math.max(
      window.lastIndexOf(", "),
      window.lastIndexOf("; "),
      window.lastIndexOf(": "),
      window.lastIndexOf(" — "),
    );
    if (cut > MIN_CHARS) {
      cut += 1; // keep the punctuation with the left piece
    } else {
      cut = window.lastIndexOf(" ");
      if (cut <= 0) cut = MAX_CHARS; // no breakable point; hard cut
    }
    parts.push(rest.slice(0, cut).trim());
    rest = rest.slice(cut).trim();
  }
  if (rest) parts.push(rest);
  return parts;
}

/**
 * Split `text` into ordered chunks suitable for back-to-back TTS synthesis.
 * Returns `[]` for empty input and a single-element array for short text (so the
 * caller can keep its non-streaming fast path).
 */
export function splitIntoSpeechChunks(text: string): string[] {
  // Collapse runs of spaces/tabs but preserve newlines as sentence boundaries.
  const normalized = (text ?? "").replace(/[ \t]+/g, " ").trim();
  if (!normalized) return [];

  const chunks: string[] = [];
  let cur = "";
  for (const sentence of splitSentences(normalized)) {
    for (const piece of splitLong(sentence)) {
      if (!cur) {
        cur = piece;
      } else if (cur.length < MIN_CHARS && cur.length + 1 + piece.length <= MAX_CHARS) {
        cur = `${cur} ${piece}`;
      } else {
        chunks.push(cur);
        cur = piece;
      }
    }
  }
  if (cur) chunks.push(cur);
  return chunks;
}
