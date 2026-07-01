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

/** A voice available on the ElevenLabs account, mirrors the Rust `VoiceSummary`. */
export interface VoiceSummary {
  voice_id: string;
  name: string;
  category: string | null;
}

/**
 * List the voices for an API key. Pass the key the user just typed, or omit it
 * to reuse the stored key (e.g. to change voice without re-entering the key).
 * Throws if the key is invalid or missing.
 */
export function listVoices(apiKey?: string): Promise<VoiceSummary[]> {
  return invoke("list_voices", { apiKey: apiKey ?? null });
}

/**
 * Save the API key and selected voice to `~/.copilot/elevenlabs/config.json`,
 * merging with any existing config. Omit `apiKey` to keep the stored key.
 * Resolves with the updated (non-secret) config.
 */
export function saveVoiceConfig(args: {
  apiKey?: string;
  voiceId: string;
  voiceName?: string | null;
}): Promise<VoiceConfigInfo> {
  return invoke("save_voice_config", {
    apiKey: args.apiKey ?? null,
    voiceId: args.voiceId,
    voiceName: args.voiceName ?? null,
  });
}

/** Synthesize `text` to speech; resolves with mp3 bytes. */
export async function tts(text: string): Promise<ArrayBuffer> {
  return invoke<ArrayBuffer>("tts", { text });
}

/** Transcribe base64-encoded audio; resolves with the recognized text. */
export function stt(audio: string, mime: string, filename: string): Promise<string> {
  return invoke("stt", { audio, mime, filename });
}

/** MCP-registration status, mirrors the Rust `McpStatus`. */
export interface McpStatus {
  registered: boolean;
  up_to_date: boolean;
  config_path: string;
  server_path: string;
  server_exists: boolean;
}

/** Current MCP-registration status (read-only). */
export function mcpStatus(): Promise<McpStatus> {
  return invoke("mcp_status");
}

/** Register (or update) the MCP server with the Copilot CLI. */
export function registerMcp(): Promise<McpStatus> {
  return invoke("register_mcp");
}

/** Remove the MCP server entry from the Copilot CLI config. */
export function unregisterMcp(): Promise<McpStatus> {
  return invoke("unregister_mcp");
}

/** Enable/disable do-not-disturb (auto-decline incoming calls). */
export function setDnd(on: boolean): Promise<void> {
  return invoke("set_dnd", { on });
}

/** Whether do-not-disturb is currently enabled. */
export function getDnd(): Promise<boolean> {
  return invoke("get_dnd");
}

/** One line of a saved call transcript. */
export interface TranscriptEntry {
  who: string;
  text: string;
}

/** A saved call, mirrors the Rust `CallRecord`. */
export interface CallRecord {
  started_at: number;
  ended_at: number;
  reason: string | null;
  outcome: string | null;
  entries: TranscriptEntry[];
}

/** Persist a completed call to history. */
export function saveCall(record: CallRecord): Promise<void> {
  return invoke("save_call", { record });
}

/** List recent calls, newest first (optionally limited). */
export function listCalls(limit?: number): Promise<CallRecord[]> {
  return invoke("list_calls", { limit: limit ?? null });
}

/** Delete all saved call history. */
export function clearCalls(): Promise<void> {
  return invoke("clear_calls");
}
