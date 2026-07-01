// Pure state-machine core for the call UI.
//
// This module has no side effects and no runtime dependencies (type-only
// imports), which keeps the transitions easy to reason about and unit-test.
// `useCallMachine` (in callMachine.ts) wraps this with the effectful bits
// (replying to the backend, ringing, notifications).

export type Phase = "idle" | "incoming" | "connected" | "listening" | "speaking" | "ended";

export type Speaker = "agent" | "you";

export interface TranscriptLine {
  key: number;
  who: Speaker;
  text: string;
}

/** A request from the agent that is currently awaiting a user reply. */
export type Pending =
  | { kind: "listen"; id: number }
  | { kind: "ack"; id: number }
  | { kind: "call"; id: number }
  | null;

export interface CallState {
  phase: Phase;
  reason: string | null;
  caption: string | null;
  transcript: TranscriptLine[];
  pending: Pending;
  muted: boolean;
  pushToTalk: boolean;
  /** Whether hands-free voice barge-in (interrupting the agent) is enabled. */
  bargeIn: boolean;
  endedReason: string | null;
  /** Transient local status note (e.g. mic denied, TTS failed). */
  note: string | null;
}

export const initialState: CallState = {
  phase: "idle",
  reason: null,
  caption: null,
  transcript: [],
  pending: null,
  muted: false,
  pushToTalk: false,
  bargeIn: true,
  endedReason: null,
  note: null,
};

let lineSeq = 0;
export const nextKey = (): number => ++lineSeq;

/** Reset the transcript key counter (test helper). */
export const resetKeys = (): void => {
  lineSeq = 0;
};

export type Action =
  | { type: "ring"; id: number; reason: string | null }
  | { type: "answered" }
  | { type: "declined" }
  | { type: "agent_say"; text: string; pending: Pending }
  | { type: "speaking_done" }
  | { type: "user_reply"; text: string }
  | { type: "clear_pending" }
  | { type: "return_to_connected" }
  | { type: "note"; text: string | null }
  | { type: "ended"; reason: string; farewell: string | null }
  | { type: "toggle_mute" }
  | { type: "set_ptt"; value: boolean }
  | { type: "set_barge_in"; value: boolean }
  | { type: "reset" };

export function reducer(state: CallState, action: Action): CallState {
  switch (action.type) {
    case "ring":
      return {
        ...initialState,
        phase: "incoming",
        reason: action.reason,
        pending: { kind: "call", id: action.id },
      };

    case "answered":
      return { ...state, phase: "connected", pending: null, caption: null };

    case "declined":
      return { ...initialState, phase: "idle" };

    case "agent_say": {
      const transcript = [
        ...state.transcript,
        { key: nextKey(), who: "agent" as const, text: action.text },
      ];
      return {
        ...state,
        phase: "speaking",
        caption: action.text,
        transcript,
        pending: action.pending,
        note: null,
      };
    }

    case "speaking_done":
      // After the agent finishes speaking, either listen or return to connected.
      return {
        ...state,
        phase: state.pending?.kind === "listen" ? "listening" : "connected",
        caption: null,
      };

    case "user_reply": {
      const transcript = [
        ...state.transcript,
        { key: nextKey(), who: "you" as const, text: action.text },
      ];
      return { ...state, phase: "connected", transcript, pending: null };
    }

    case "clear_pending":
      return { ...state, pending: null };

    case "note":
      return { ...state, note: action.text };

    case "return_to_connected":
      // Listen finished with nothing to add (no speech / error); go back to
      // the connected state without appending a transcript line.
      return { ...state, phase: "connected", pending: null, caption: null };

    case "ended": {
      const transcript = action.farewell
        ? [...state.transcript, { key: nextKey(), who: "agent" as const, text: action.farewell }]
        : state.transcript;
      return {
        ...state,
        phase: "ended",
        caption: action.farewell,
        transcript,
        pending: null,
        endedReason: action.reason,
      };
    }

    case "toggle_mute":
      return { ...state, muted: !state.muted };

    case "set_ptt":
      return { ...state, pushToTalk: action.value };

    case "set_barge_in":
      return { ...state, bargeIn: action.value };

    case "reset":
      return { ...initialState };

    default:
      return state;
  }
}
