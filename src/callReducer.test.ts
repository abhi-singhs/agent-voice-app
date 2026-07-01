import { beforeEach, describe, expect, it } from "vitest";

import { initialState, reducer, resetKeys, type CallState } from "./callReducer";

beforeEach(() => resetKeys());

/** Apply a sequence of actions starting from idle. */
function run(...actions: Parameters<typeof reducer>[1][]): CallState {
  return actions.reduce((s, a) => reducer(s, a), initialState);
}

describe("call reducer", () => {
  it("starts idle", () => {
    expect(initialState.phase).toBe("idle");
    expect(initialState.pending).toBeNull();
  });

  it("rings on incoming call and tracks the pending call id", () => {
    const s = run({ type: "ring", id: 7, reason: "standup" });
    expect(s.phase).toBe("incoming");
    expect(s.reason).toBe("standup");
    expect(s.pending).toEqual({ kind: "call", id: 7 });
  });

  it("answers into the connected phase and clears pending", () => {
    const s = run({ type: "ring", id: 1, reason: null }, { type: "answered" });
    expect(s.phase).toBe("connected");
    expect(s.pending).toBeNull();
  });

  it("declines back to idle", () => {
    const s = run({ type: "ring", id: 1, reason: null }, { type: "declined" });
    expect(s.phase).toBe("idle");
    expect(s.pending).toBeNull();
  });

  it("say_and_listen: speaks then listens, then records the reply", () => {
    const afterSay = run(
      { type: "ring", id: 1, reason: null },
      { type: "answered" },
      { type: "agent_say", text: "How are you?", pending: { kind: "listen", id: 2 } },
    );
    expect(afterSay.phase).toBe("speaking");
    expect(afterSay.caption).toBe("How are you?");
    expect(afterSay.transcript).toHaveLength(1);
    expect(afterSay.transcript[0]).toMatchObject({ who: "agent", text: "How are you?" });

    const listening = reducer(afterSay, { type: "speaking_done" });
    expect(listening.phase).toBe("listening");
    expect(listening.caption).toBeNull();
    expect(listening.pending).toEqual({ kind: "listen", id: 2 });

    const replied = reducer(listening, { type: "user_reply", text: "Doing well" });
    expect(replied.phase).toBe("connected");
    expect(replied.pending).toBeNull();
    expect(replied.transcript).toHaveLength(2);
    expect(replied.transcript[1]).toMatchObject({ who: "you", text: "Doing well" });
  });

  it("one-way say returns to connected (no listen)", () => {
    const speaking = run(
      { type: "ring", id: 1, reason: null },
      { type: "answered" },
      { type: "agent_say", text: "FYI: build is green", pending: null },
    );
    expect(speaking.phase).toBe("speaking");
    const back = reducer(speaking, { type: "speaking_done" });
    expect(back.phase).toBe("connected");
    expect(back.pending).toBeNull();
  });

  it("ends with a farewell appended to the transcript", () => {
    const ended = run(
      { type: "ring", id: 1, reason: null },
      { type: "answered" },
      { type: "ended", reason: "The agent ended the call.", farewell: "Bye!" },
    );
    expect(ended.phase).toBe("ended");
    expect(ended.endedReason).toBe("The agent ended the call.");
    expect(ended.transcript.at(-1)).toMatchObject({ who: "agent", text: "Bye!" });
  });

  it("reset returns to the pristine idle state", () => {
    const ended = run(
      { type: "ring", id: 1, reason: null },
      { type: "answered" },
      { type: "ended", reason: "done", farewell: null },
    );
    expect(reducer(ended, { type: "reset" })).toEqual(initialState);
  });

  it("mute and push-to-talk toggle independently", () => {
    let s = reducer(initialState, { type: "toggle_mute" });
    expect(s.muted).toBe(true);
    s = reducer(s, { type: "toggle_mute" });
    expect(s.muted).toBe(false);
    s = reducer(s, { type: "set_ptt", value: true });
    expect(s.pushToTalk).toBe(true);
  });
});
