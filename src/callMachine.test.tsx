// @vitest-environment jsdom
import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { VoiceRequest } from "./ipc";
import type { ListenHandle, ListenResult } from "./audio/listen";

const respondCall = vi.fn();
const respondListen = vi.fn();
const respondAck = vi.fn();
const notifyHangup = vi.fn();
const saveCall = vi.fn((_record: unknown) => Promise.resolve());
const tts = vi.fn(() => Promise.resolve(new ArrayBuffer(8)));
const stt = vi.fn(() => Promise.resolve("hello there"));
let requestHandler: ((req: VoiceRequest) => void) | null = null;

vi.mock("./ipc", () => ({
  onVoiceRequest: (h: (req: VoiceRequest) => void) => {
    requestHandler = h;
    return Promise.resolve(() => {});
  },
  respondCall: (...a: unknown[]) => respondCall(...a),
  respondListen: (...a: unknown[]) => respondListen(...a),
  respondAck: (...a: unknown[]) => respondAck(...a),
  notifyHangup: (...a: unknown[]) => notifyHangup(...a),
  saveCall: (...a: unknown[]) => saveCall(...(a as [unknown])),
  tts: (...a: unknown[]) => tts(...(a as [])),
  stt: (...a: unknown[]) => stt(...(a as [])),
}));

vi.mock("./notify", () => ({ notifyIncomingCall: vi.fn(() => Promise.resolve()) }));

// Audio side-effects: playback resolves (or stays pending); listen is scriptable.
const playTts = vi.fn(() => Promise.resolve());
vi.mock("./audio/player", () => ({
  playTts: (...a: unknown[]) => playTts(...(a as [])),
  stopPlayback: vi.fn(),
}));

const listenFinish = vi.fn();
const listenCancel = vi.fn();
let nextListenResult: Promise<ListenResult> = new Promise<ListenResult>(() => {});
const startListening = vi.fn(
  (): ListenHandle => ({ result: nextListenResult, finish: listenFinish, cancel: listenCancel }),
);
vi.mock("./audio/listen", () => ({
  startListening: (...a: unknown[]) => startListening(...(a as [])),
}));

import { useCallMachine } from "./callMachine";

async function mountAndRegister() {
  const hook = renderHook(() => useCallMachine());
  await act(async () => {});
  return hook;
}

function emit(req: VoiceRequest) {
  act(() => requestHandler?.(req));
}

async function flush() {
  for (let i = 0; i < 12; i++) {
    // eslint-disable-next-line no-await-in-loop
    await act(async () => {
      await Promise.resolve();
    });
  }
}

async function connect(id = 1) {
  const hook = await mountAndRegister();
  emit({ kind: "incoming_call", id, reason: null, timeout_sec: null });
  act(() => hook.result.current.answer());
  return hook;
}

beforeEach(() => {
  vi.clearAllMocks();
  requestHandler = null;
  playTts.mockReturnValue(Promise.resolve());
  tts.mockReturnValue(Promise.resolve(new ArrayBuffer(8)));
  stt.mockReturnValue(Promise.resolve("hello there"));
  nextListenResult = new Promise<ListenResult>(() => {});
});

describe("useCallMachine", () => {
  it("registers a request handler on mount", async () => {
    await mountAndRegister();
    expect(requestHandler).toBeTypeOf("function");
  });

  it("rings on incoming_call and answers into connected", async () => {
    const { result } = await mountAndRegister();
    emit({ kind: "incoming_call", id: 5, reason: "standup", timeout_sec: 30 });
    expect(result.current.state.phase).toBe("incoming");
    expect(result.current.state.reason).toBe("standup");

    act(() => result.current.answer());
    expect(respondCall).toHaveBeenCalledWith(5, "answered");
    expect(result.current.state.phase).toBe("connected");
  });

  it("declines an incoming call back to idle", async () => {
    const { result } = await mountAndRegister();
    emit({ kind: "incoming_call", id: 9, reason: null, timeout_sec: null });
    act(() => result.current.decline());
    expect(respondCall).toHaveBeenCalledWith(9, "declined");
    expect(result.current.state.phase).toBe("idle");
  });

  it("declines a second concurrent call while in one", async () => {
    const { result } = await connect(1);
    respondCall.mockClear();
    emit({ kind: "incoming_call", id: 2, reason: null, timeout_sec: null });
    expect(respondCall).toHaveBeenCalledWith(2, "declined");
    expect(result.current.state.phase).toBe("connected");
  });

  it("say_and_listen enters speaking while TTS is playing", async () => {
    // Keep playback pending so we can observe the speaking phase.
    playTts.mockReturnValue(new Promise<void>(() => {}));
    const { result } = await connect(1);
    emit({ kind: "say_and_listen", id: 2, text: "How are you?", listen: true, listen_timeout_sec: null });
    expect(result.current.state.phase).toBe("speaking");
    expect(result.current.state.caption).toBe("How are you?");
    expect(respondListen).not.toHaveBeenCalled();
  });

  it("completes a spoken reply end-to-end (TTS → listen → STT)", async () => {
    nextListenResult = Promise.resolve({ status: "ok", wavBase64: "AAAA", durationMs: 800 });
    const { result } = await connect(1);
    emit({ kind: "say_and_listen", id: 2, text: "Hello?", listen: true, listen_timeout_sec: 15 });
    await flush();

    expect(tts).toHaveBeenCalledWith("Hello?");
    expect(startListening).toHaveBeenCalled();
    expect(stt).toHaveBeenCalledWith("AAAA", "audio/wav", "reply.wav");
    expect(respondListen).toHaveBeenCalledWith(2, "hello there", "ok");
    expect(result.current.state.phase).toBe("connected");
    expect(result.current.state.transcript.at(-1)).toMatchObject({ who: "you", text: "hello there" });
  });

  it("reports no_speech when the listen turn hears nothing", async () => {
    nextListenResult = Promise.resolve({ status: "no_speech" });
    const { result } = await connect(1);
    emit({ kind: "say_and_listen", id: 2, text: "Hi?", listen: true, listen_timeout_sec: null });
    await flush();

    expect(stt).not.toHaveBeenCalled();
    expect(respondListen).toHaveBeenCalledWith(2, null, "no_speech");
    expect(result.current.state.phase).toBe("connected");
  });

  it("surfaces a note and returns no_speech when the mic fails", async () => {
    nextListenResult = Promise.resolve({ status: "error", error: "denied" });
    const { result } = await connect(1);
    emit({ kind: "say_and_listen", id: 2, text: "Hi?", listen: true, listen_timeout_sec: null });
    await flush();

    expect(stt).not.toHaveBeenCalled();
    expect(respondListen).toHaveBeenCalledWith(2, null, "no_speech");
    expect(result.current.state.note).toMatch(/microphone/i);
    expect(result.current.state.phase).toBe("connected");
  });

  it("surfaces a note when TTS playback fails", async () => {
    playTts.mockImplementationOnce(() => Promise.reject(new Error("no audio")));
    nextListenResult = Promise.resolve({ status: "no_speech" });
    const { result } = await connect(1);
    emit({ kind: "say_and_listen", id: 2, text: "Hi?", listen: true, listen_timeout_sec: null });
    await flush();

    expect(result.current.state.note).toMatch(/audio/i);
  });

  it("voice_say speaks then acks and returns to connected", async () => {
    const { result } = await connect(1);
    emit({ kind: "say", id: 2, text: "Heads up!" });
    await flush();
    expect(tts).toHaveBeenCalledWith("Heads up!");
    expect(respondAck).toHaveBeenCalledWith(2, "ok");
    expect(result.current.state.phase).toBe("connected");
  });

  it("a typed reply cancels the mic capture and answers the agent", async () => {
    // Listen never resolves on its own; the typed reply should drive it.
    const { result } = await connect(1);
    emit({ kind: "say_and_listen", id: 2, text: "Name?", listen: true, listen_timeout_sec: null });
    await flush();
    expect(startListening).toHaveBeenCalled();

    act(() => result.current.sendReply("Ada"));
    expect(listenCancel).toHaveBeenCalled();
    expect(respondListen).toHaveBeenCalledWith(2, "Ada", "ok");
    expect(result.current.state.transcript.at(-1)).toMatchObject({ who: "you", text: "Ada" });
  });

  it("hang up during a listen resolves the pending request with call_ended", async () => {
    playTts.mockReturnValue(new Promise<void>(() => {}));
    const { result } = await connect(1);
    emit({ kind: "say_and_listen", id: 2, text: "Hi", listen: true, listen_timeout_sec: null });

    act(() => result.current.hangUp());
    expect(respondListen).toHaveBeenCalledWith(2, null, "call_ended");
    expect(notifyHangup).toHaveBeenCalled();
    expect(result.current.state.phase).toBe("ended");
  });

  it("agent hangup acks and ends the call", async () => {
    const { result } = await connect(1);
    emit({ kind: "hangup", id: 3, farewell: "Bye!" });
    expect(respondAck).toHaveBeenCalledWith(3, "ok");
    expect(result.current.state.phase).toBe("ended");
    expect(result.current.state.transcript.at(-1)).toMatchObject({ who: "agent", text: "Bye!" });
  });

  it("saves the call to history when it ends with a transcript", async () => {
    const { result } = await connect(1);
    emit({ kind: "say", id: 2, text: "Hello there" });
    await flush();
    act(() => result.current.hangUp());
    await flush();

    expect(saveCall).toHaveBeenCalledTimes(1);
    const record = saveCall.mock.calls[0][0] as {
      entries: Array<{ who: string; text: string }>;
      outcome: string | null;
    };
    expect(record.entries).toEqual([{ who: "agent", text: "Hello there" }]);
    expect(record.outcome).toMatch(/ended/i);
  });
});
