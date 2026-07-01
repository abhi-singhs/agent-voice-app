import { useCallback, useEffect, useMemo, useReducer, useRef } from "react";

import { playTts, stopPlayback } from "./audio/player";
import { startListening, type ListenHandle } from "./audio/listen";
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
  stt,
  tts,
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
  startTalking: () => void;
  stopTalking: () => void;
}

export function useCallMachine(): CallController {
  const [state, dispatch] = useReducer(reducer, initialState);
  const ringer = useRef<Ringer>(new Ringer());
  // Mirrors of state for use inside stable event callbacks.
  const pendingRef = useRef<Pending>(null);
  const phaseRef = useRef<Phase>("idle");
  const mutedRef = useRef(false);
  const pttRef = useRef(false);
  const endTimer = useRef<number | null>(null);
  // The active listen turn (mic capture), if any.
  const listenRef = useRef<ListenHandle | null>(null);

  useEffect(() => {
    pendingRef.current = state.pending;
    phaseRef.current = state.phase;
    mutedRef.current = state.muted;
    pttRef.current = state.pushToTalk;
  }, [state.pending, state.phase, state.muted, state.pushToTalk]);

  // Auto-return to idle a few seconds after a call ends.
  useEffect(() => {
    if (state.phase === "ended") {
      endTimer.current = window.setTimeout(() => dispatch({ type: "reset" }), 3500);
      return () => {
        if (endTimer.current !== null) clearTimeout(endTimer.current);
      };
    }
  }, [state.phase]);

  // Speak text via TTS; resolves when playback ends. Never throws — a TTS
  // failure just means the captions play silently.
  const speak = useCallback(async (text: string) => {
    try {
      const bytes = await tts(text);
      await playTts(bytes);
    } catch (err) {
      console.error("TTS failed:", err);
    }
  }, []);

  // Consume a listen turn's result: transcribe and reply to the agent.
  const consumeListen = useCallback(async (id: number, handle: ListenHandle) => {
    const res = await handle.result;
    if (listenRef.current === handle) listenRef.current = null;
    // Someone else already resolved this request (hang-up / typed reply).
    if (res.status === "aborted") return;
    const p = pendingRef.current;
    if (!(p?.kind === "listen" && p.id === id)) return;

    if (res.status === "ok") {
      try {
        const text = (await stt(res.wavBase64, "audio/wav", "reply.wav")).trim();
        if (text) {
          void respondListen(id, text, "ok");
          dispatch({ type: "user_reply", text });
          return;
        }
      } catch (err) {
        console.error("STT failed:", err);
      }
    }
    // no_speech / error / empty transcript
    void respondListen(id, null, "no_speech");
    dispatch({ type: "return_to_connected" });
  }, []);

  // Start a hands-free (VAD) or manual (push-to-talk) listen turn.
  const beginListen = useCallback(
    (id: number, timeoutSec: number | null, manual: boolean) => {
      const handle = startListening({
        manual,
        startTimeoutMs: timeoutSec ? timeoutSec * 1000 : undefined,
      });
      listenRef.current = handle;
      void consumeListen(id, handle);
    },
    [consumeListen],
  );

  const handleRequest = useCallback(
    (req: VoiceRequest) => {
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
          // One-way announcement: speak, then acknowledge.
          dispatch({ type: "agent_say", text: req.text, pending: { kind: "ack", id: req.id } });
          void speak(req.text).then(() => {
            const p = pendingRef.current;
            if (p?.kind === "ack" && p.id === req.id) {
              void respondAck(req.id, "ok");
              dispatch({ type: "return_to_connected" });
            }
          });
          break;
        }

        case "say_and_listen": {
          const pending: Pending = req.listen
            ? { kind: "listen", id: req.id }
            : { kind: "ack", id: req.id };
          dispatch({ type: "agent_say", text: req.text, pending });
          void speak(req.text).then(() => {
            // Bail out if the call ended (or moved on) while speaking.
            const p = pendingRef.current;
            if (!(p && p.id === req.id)) return;
            if (!req.listen) {
              void respondListen(req.id, null, "ok");
              dispatch({ type: "return_to_connected" });
              return;
            }
            dispatch({ type: "speaking_done" }); // -> listening
            // Hands-free capture unless muted or in push-to-talk mode.
            if (!mutedRef.current && !pttRef.current) {
              beginListen(req.id, req.listen_timeout_sec, false);
            }
          });
          break;
        }

        case "hangup": {
          void respondAck(req.id, "ok");
          stopPlayback();
          listenRef.current?.cancel();
          listenRef.current = null;
          dispatch({ type: "ended", reason: "The agent ended the call.", farewell: req.farewell });
          break;
        }
      }
    },
    [speak, beginListen],
  );

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
    stopPlayback();
    listenRef.current?.cancel();
    listenRef.current = null;
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
    if (p?.kind === "listen") {
      // Cancel any live mic capture so it doesn't also answer.
      listenRef.current?.cancel();
      listenRef.current = null;
      void respondListen(p.id, trimmed, "ok");
    }
    dispatch({ type: "user_reply", text: trimmed });
  }, []);

  const toggleMute = useCallback(() => {
    const nowMuted = !mutedRef.current;
    dispatch({ type: "toggle_mute" });
    if (phaseRef.current === "listening" && pendingRef.current?.kind === "listen") {
      if (nowMuted) {
        // Stop capturing; user will type or use push-to-talk.
        listenRef.current?.cancel();
        listenRef.current = null;
      } else if (!pttRef.current && !listenRef.current) {
        beginListen(pendingRef.current.id, null, false);
      }
    }
  }, [beginListen]);

  const setPushToTalk = useCallback((value: boolean) => {
    dispatch({ type: "set_ptt", value });
    // Entering push-to-talk cancels any hands-free capture in progress.
    if (value && listenRef.current) {
      listenRef.current.cancel();
      listenRef.current = null;
    }
  }, []);

  const startTalking = useCallback(() => {
    const p = pendingRef.current;
    if (phaseRef.current !== "listening" || p?.kind !== "listen") return;
    if (listenRef.current) return;
    beginListen(p.id, null, true);
  }, [beginListen]);

  const stopTalking = useCallback(() => {
    listenRef.current?.finish();
  }, []);

  return useMemo(
    () => ({
      state,
      answer,
      decline,
      hangUp,
      sendReply,
      toggleMute,
      setPushToTalk,
      startTalking,
      stopTalking,
    }),
    [state, answer, decline, hangUp, sendReply, toggleMute, setPushToTalk, startTalking, stopTalking],
  );
}
