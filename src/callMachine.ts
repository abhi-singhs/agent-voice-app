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
  saveCall,
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
  setBargeIn: (value: boolean) => void;
  interrupt: () => void;
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
  const bargeInRef = useRef(true);
  const endTimer = useRef<number | null>(null);
  // The active listen turn (mic capture), if any.
  const listenRef = useRef<ListenHandle | null>(null);
  // True while TTS is actively playing. Updated *synchronously* (unlike phaseRef,
  // which is mirrored from state via an effect and therefore lags a dispatch), so
  // the barge-drop guard in consumeListen can reliably tell whether a listen
  // resolved during the agent's speech.
  const ttsActiveRef = useRef(false);
  // Epoch ms when the current call was answered (for history).
  const callStartRef = useRef<number | null>(null);

  useEffect(() => {
    pendingRef.current = state.pending;
    phaseRef.current = state.phase;
    mutedRef.current = state.muted;
    pttRef.current = state.pushToTalk;
    bargeInRef.current = state.bargeIn;
  }, [state.pending, state.phase, state.muted, state.pushToTalk, state.bargeIn]);

  // Auto-return to idle a few seconds after a call ends, and persist the
  // transcript to history (best-effort) once per ended call.
  useEffect(() => {
    if (state.phase === "ended") {
      if (state.transcript.length > 0) {
        void saveCall({
          started_at: callStartRef.current ?? Date.now(),
          ended_at: Date.now(),
          reason: state.reason,
          outcome: state.endedReason,
          entries: state.transcript.map((l) => ({ who: l.who, text: l.text })),
        }).catch((err) => console.error("save_call failed:", err));
      }
      callStartRef.current = null;
      endTimer.current = window.setTimeout(() => dispatch({ type: "reset" }), 3500);
      return () => {
        if (endTimer.current !== null) clearTimeout(endTimer.current);
      };
    }
  }, [state.phase, state.transcript, state.reason, state.endedReason]);

  // Speak text via TTS; resolves true on success, false if audio failed. A
  // failure is non-fatal — captions still show — but we surface a note so the
  // user knows to rely on text.
  const speak = useCallback(async (text: string): Promise<boolean> => {
    ttsActiveRef.current = true;
    try {
      const bytes = await tts(text);
      await playTts(bytes);
      return true;
    } catch (err) {
      console.error("TTS failed:", err);
      dispatch({ type: "note", text: "Couldn’t play audio — check ElevenLabs setup." });
      return false;
    } finally {
      ttsActiveRef.current = false;
    }
  }, []);

  // Consume a listen turn's result: transcribe and reply to the agent.
  const consumeListen = useCallback(async (id: number, handle: ListenHandle) => {
    const res = await handle.result;
    if (listenRef.current === handle) listenRef.current = null;
    // Someone else already resolved this request (hang-up / typed reply).
    if (res.status === "aborted") return;
    // A barge listen that ends *without a transcript* while the agent is still
    // speaking means the user never engaged (e.g. the mic failed to open before
    // barge-in). Drop it silently so the post-TTS path starts a proper listen
    // (which surfaces any mic error then). A real "ok" is always delivered.
    if (res.status !== "ok" && ttsActiveRef.current) return;
    const p = pendingRef.current;
    if (!(p?.kind === "listen" && p.id === id)) return;

    if (res.status === "error") {
      // Mic capture failed (e.g. permission denied) — let the user type.
      dispatch({ type: "note", text: "Microphone unavailable — type your reply below." });
      void respondListen(id, null, "no_speech");
      dispatch({ type: "return_to_connected" });
      return;
    }

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
        dispatch({ type: "note", text: "Couldn’t transcribe audio — type your reply below." });
      }
    }
    // no_speech / empty transcript
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

  // Open the mic *during* the agent's speech so the user can barge in. Watches
  // only for speech onset; when the user starts talking it cuts off TTS and
  // promotes to a normal capture turn.
  const beginBargeListen = useCallback(
    (id: number, timeoutSec: number | null) => {
      const handle = startListening({
        barge: true,
        startTimeoutMs: timeoutSec ? timeoutSec * 1000 : undefined,
        onBarge: () => {
          stopPlayback();
          dispatch({ type: "speaking_done" });
        },
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
          // Open the mic during playback so the user can barge in, unless barge-in
          // is off, muted, or in push-to-talk (which capture only on demand).
          if (req.listen && bargeInRef.current && !mutedRef.current && !pttRef.current) {
            beginBargeListen(req.id, req.listen_timeout_sec);
          }
          void speak(req.text).then(() => {
            // Bail out if the call ended (or moved on) while speaking.
            const p = pendingRef.current;
            if (!(p && p.id === req.id)) return;
            if (!req.listen) {
              void respondListen(req.id, null, "ok");
              dispatch({ type: "return_to_connected" });
              return;
            }
            // -> listening (idempotent if a barge-in already transitioned us).
            if (phaseRef.current === "speaking") dispatch({ type: "speaking_done" });
            if (mutedRef.current || pttRef.current) {
              // The user will type or use push-to-talk; drop any armed capture.
              listenRef.current?.cancel();
              listenRef.current = null;
              return;
            }
            if (listenRef.current) {
              // A concurrent barge listen is open — promote it to a real capture.
              listenRef.current.beginCapture();
            } else {
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
    [speak, beginListen, beginBargeListen],
  );

  useEffect(() => {
    const unlisten = onVoiceRequest(handleRequest);
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, [handleRequest]);

  const answer = useCallback(() => {
    ringer.current.stop();
    callStartRef.current = Date.now();
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

  const setBargeIn = useCallback((value: boolean) => {
    dispatch({ type: "set_barge_in", value });
    // Turning barge-in off while the agent is speaking closes the armed mic; a
    // fresh listen opens once the agent finishes.
    if (!value && phaseRef.current === "speaking" && listenRef.current) {
      listenRef.current.cancel();
      listenRef.current = null;
    }
  }, []);

  // Cut the agent off mid-sentence. Stopping playback resolves the speak()
  // promise, whose continuation hands the turn to the user (say_and_listen) or
  // acks a one-way announcement.
  const interrupt = useCallback(() => {
    if (phaseRef.current !== "speaking") return;
    stopPlayback();
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
      setBargeIn,
      interrupt,
      startTalking,
      stopTalking,
    }),
    [
      state,
      answer,
      decline,
      hangUp,
      sendReply,
      toggleMute,
      setPushToTalk,
      setBargeIn,
      interrupt,
      startTalking,
      stopTalking,
    ],
  );
}
