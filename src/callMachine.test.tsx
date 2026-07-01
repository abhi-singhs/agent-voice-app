// @vitest-environment jsdom
import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { VoiceRequest } from "./ipc";

const respondCall = vi.fn();
const respondListen = vi.fn();
const respondAck = vi.fn();
const notifyHangup = vi.fn();
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
}));

vi.mock("./notify", () => ({ notifyIncomingCall: vi.fn(() => Promise.resolve()) }));

import { useCallMachine } from "./callMachine";

async function mountAndRegister() {
  const hook = renderHook(() => useCallMachine());
  // Let the subscription effect register the request handler.
  await act(async () => {});
  return hook;
}

function emit(req: VoiceRequest) {
  act(() => requestHandler?.(req));
}

beforeEach(() => {
  vi.clearAllMocks();
  requestHandler = null;
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
    const { result } = await mountAndRegister();
    emit({ kind: "incoming_call", id: 1, reason: null, timeout_sec: null });
    act(() => result.current.answer());
    respondCall.mockClear();
    emit({ kind: "incoming_call", id: 2, reason: null, timeout_sec: null });
    expect(respondCall).toHaveBeenCalledWith(2, "declined");
    // Still in the first call.
    expect(result.current.state.phase).toBe("connected");
  });

  it("say_and_listen enters speaking with a listen pending", async () => {
    const { result } = await mountAndRegister();
    emit({ kind: "incoming_call", id: 1, reason: null, timeout_sec: null });
    act(() => result.current.answer());
    emit({ kind: "say_and_listen", id: 2, text: "How are you?", listen: true, listen_timeout_sec: null });
    expect(result.current.state.phase).toBe("speaking");
    expect(result.current.state.caption).toBe("How are you?");
    expect(respondListen).not.toHaveBeenCalled();
  });

  it("hang up during a listen resolves the pending request with call_ended", async () => {
    const { result } = await mountAndRegister();
    emit({ kind: "incoming_call", id: 1, reason: null, timeout_sec: null });
    act(() => result.current.answer());
    emit({ kind: "say_and_listen", id: 2, text: "Hi", listen: true, listen_timeout_sec: null });

    act(() => result.current.hangUp());
    expect(respondListen).toHaveBeenCalledWith(2, null, "call_ended");
    expect(notifyHangup).toHaveBeenCalled();
    expect(result.current.state.phase).toBe("ended");
  });

  it("agent hangup acks and ends the call", async () => {
    const { result } = await mountAndRegister();
    emit({ kind: "incoming_call", id: 1, reason: null, timeout_sec: null });
    act(() => result.current.answer());
    emit({ kind: "hangup", id: 3, farewell: "Bye!" });
    expect(respondAck).toHaveBeenCalledWith(3, "ok");
    expect(result.current.state.phase).toBe("ended");
    expect(result.current.state.transcript.at(-1)).toMatchObject({ who: "agent", text: "Bye!" });
  });
});
