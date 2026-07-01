// Typed bridge between the React UI and the Tauri backend.
//
// The backend emits `voice://request` events (one per agent request awaiting a
// user reply) and exposes commands the UI calls to answer them. Message shapes
// mirror the Rust `FrontendRequest` enum and the protocol status enums.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/** Event name the backend emits for each agent request. */
export const REQUEST_EVENT = "voice://request";

/** Ring outcome reported back to the agent. */
export type CallStatus = "answered" | "declined" | "timeout" | "no_device";
/** Listen outcome reported back to the agent. */
export type ListenStatus = "ok" | "no_speech" | "call_ended";
/** One-way (say / hangup) outcome reported back to the agent. */
export type AckStatus = "ok" | "call_ended" | "error";

/** A request forwarded from the agent (via the WS server) to the webview. */
export type VoiceRequest =
  | { kind: "incoming_call"; id: number; reason: string | null; timeout_sec: number | null }
  | {
      kind: "say_and_listen";
      id: number;
      text: string;
      listen: boolean;
      listen_timeout_sec: number | null;
    }
  | { kind: "say"; id: number; text: string }
  | { kind: "hangup"; id: number; farewell: string | null };

/** Subscribe to incoming agent requests. Returns an unlisten function. */
export function onVoiceRequest(handler: (req: VoiceRequest) => void): Promise<UnlistenFn> {
  return listen<VoiceRequest>(REQUEST_EVENT, (event) => handler(event.payload));
}

/** Reply to an `incoming_call` request. */
export function respondCall(id: number, status: CallStatus): Promise<void> {
  return invoke("respond_call", { id, status });
}

/** Reply to a `say_and_listen` request with the transcript (if any). */
export function respondListen(
  id: number,
  heard: string | null,
  status: ListenStatus,
): Promise<void> {
  return invoke("respond_listen", { id, heard, status });
}

/** Reply to a one-way `say` / `hangup` request. */
export function respondAck(id: number, status: AckStatus): Promise<void> {
  return invoke("respond_ack", { id, status });
}

/** Tell the agent the user hung up from the app. */
export function notifyHangup(): Promise<void> {
  return invoke("notify_hangup");
}

/** Non-secret ElevenLabs config, mirrors the Rust `VoiceConfigInfo`. */
export interface VoiceConfigInfo {
  voice_id: string;
  voice_name: string | null;
  model_id: string;
  enabled: boolean;
  configured: boolean;
}

/** Fetch the current ElevenLabs voice config (throws if not configured). */
export function voiceConfig(): Promise<VoiceConfigInfo> {
  return invoke("voice_config");
}

/** Synthesize `text` to speech; resolves with mp3 bytes. */
export async function tts(text: string): Promise<ArrayBuffer> {
  return invoke<ArrayBuffer>("tts", { text });
}

/** Transcribe base64-encoded audio; resolves with the recognized text. */
export function stt(audio: string, mime: string, filename: string): Promise<string> {
  return invoke("stt", { audio, mime, filename });
}
