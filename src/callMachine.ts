import { useCallback, useEffect, useMemo, useReducer, useRef } from "react";

import { Ringer } from "./audio/ringtone";
import {
  initialState,
  reducer,
  type CallState,
  type Pending,
  type Phase,
} from "./callReducer";
import {
  notifyHangup,
  onVoiceRequest,
  respondAck,
  respondCall,
  respondListen,
  type VoiceRequest,
} from "./ipc";
import { notifyIncomingCall } from "./notify";

export type { CallState, Phase, Speaker, TranscriptLine } from "./callReducer";

export interface CallController {
  state: CallState;
  answer: () => void;
  decline: () => void;
  hangUp: () => void;
  sendReply: (text: string) => void;
  toggleMute: () => void;
  setPushToTalk: (value: boolean) => void;
}

export function useCallMachine(): CallController {
  const [state, dispatch] = useReducer(reducer, initialState);
  const ringer = useRef<Ringer>(new Ringer());
  // Mirror of state.pending for use inside stable event callbacks.
  const pendingRef = useRef<Pending>(null);
  const phaseRef = useRef<Phase>("idle");
  const endTimer = useRef<number | null>(null);

  useEffect(() => {
    pendingRef.current = state.pending;
    phaseRef.current = state.phase;
  }, [state.pending, state.phase]);

  // Auto-return to idle a few seconds after a call ends.
  useEffect(() => {
    if (state.phase === "ended") {
      endTimer.current = window.setTimeout(() => dispatch({ type: "reset" }), 3500);
      return () => {
        if (endTimer.current !== null) clearTimeout(endTimer.current);
      };
    }
  }, [state.phase]);

  const handleRequest = useCallback((req: VoiceRequest) => {
    switch (req.kind) {
      case "incoming_call": {
        // Decline a second call while one is already active.
        if (phaseRef.current !== "idle" && phaseRef.current !== "ended") {
          void respondCall(req.id, "declined");
          return;
        }
        ringer.current.start();
        void notifyIncomingCall(req.reason);
        dispatch({ type: "ring", id: req.id, reason: req.reason });
        break;
      }

      case "say": {
        dispatch({ type: "agent_say", text: req.text, pending: null });
        // P3: no TTS yet — acknowledge immediately so the agent can proceed.
        void respondAck(req.id, "ok");
        window.setTimeout(() => dispatch({ type: "speaking_done" }), 1200);
        break;
      }

      case "say_and_listen": {
        const pending: Pending = req.listen ? { kind: "listen", id: req.id } : { kind: "ack", id: req.id };
        dispatch({ type: "agent_say", text: req.text, pending });
        if (req.listen) {
          // Move into the listening phase after a brief "speaking" beat.
          window.setTimeout(() => dispatch({ type: "speaking_done" }), 1200);
        } else {
          void respondListen(req.id, null, "ok");
          window.setTimeout(() => dispatch({ type: "speaking_done" }), 1200);
        }
        break;
      }

      case "hangup": {
        void respondAck(req.id, "ok");
        dispatch({ type: "ended", reason: "The agent ended the call.", farewell: req.farewell });
        break;
      }
    }
  }, []);

  useEffect(() => {
    const unlisten = onVoiceRequest(handleRequest);
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, [handleRequest]);

  const answer = useCallback(() => {
    ringer.current.stop();
    const p = pendingRef.current;
    if (p?.kind === "call") void respondCall(p.id, "answered");
    dispatch({ type: "answered" });
  }, []);

  const decline = useCallback(() => {
    ringer.current.stop();
    const p = pendingRef.current;
    if (p?.kind === "call") void respondCall(p.id, "declined");
    dispatch({ type: "declined" });
  }, []);

  const hangUp = useCallback(() => {
    ringer.current.stop();
    const p = pendingRef.current;
    // Resolve any in-flight request so the agent's tool call returns promptly.
    if (p?.kind === "listen") void respondListen(p.id, null, "call_ended");
    else if (p?.kind === "ack") void respondAck(p.id, "call_ended");
    else if (p?.kind === "call") void respondCall(p.id, "declined");
    void notifyHangup();
    dispatch({ type: "ended", reason: "You ended the call.", farewell: null });
  }, []);

  const sendReply = useCallback((text: string) => {
    const trimmed = text.trim();
    if (!trimmed) return;
    const p = pendingRef.current;
    if (p?.kind === "listen") void respondListen(p.id, trimmed, "ok");
    dispatch({ type: "user_reply", text: trimmed });
  }, []);

  const toggleMute = useCallback(() => dispatch({ type: "toggle_mute" }), []);
  const setPushToTalk = useCallback((value: boolean) => dispatch({ type: "set_ptt", value }), []);

  return useMemo(
    () => ({ state, answer, decline, hangUp, sendReply, toggleMute, setPushToTalk }),
    [state, answer, decline, hangUp, sendReply, toggleMute, setPushToTalk],
  );
}
