import { useCallback, useEffect, useState } from "react";

import {
  listVoices,
  mcpClients as listMcpClients,
  mcpStatus,
  registerMcp,
  saveVoiceConfig,
  unregisterMcp,
  voiceConfig,
  type McpClientInfo,
  type McpStatus,
  type VoiceConfigInfo,
  type VoiceSummary,
} from "../ipc";

/**
 * First-run / settings panel shown from the idle screen. Surfaces the two
 * things a user must get right for a call to work: an ElevenLabs voice config
 * and the MCP server registered with the agent client.
 */
export function SetupPanel({ onClose }: { onClose: () => void }) {
  const [mcpClientOptions, setMcpClientOptions] = useState<McpClientInfo[]>([]);
  const [selectedMcpClientId, setSelectedMcpClientId] = useState("copilot");
  const [voice, setVoice] = useState<VoiceConfigInfo | null>(null);
  const [voiceErr, setVoiceErr] = useState<string | null>(null);
  const [mcp, setMcp] = useState<McpStatus | null>(null);
  const [mcpErr, setMcpErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  // Editable ElevenLabs form state.
  const [editing, setEditing] = useState(false);
  const [apiKeyInput, setApiKeyInput] = useState("");
  const [voices, setVoices] = useState<VoiceSummary[]>([]);
  const [selectedVoiceId, setSelectedVoiceId] = useState("");
  const [fetching, setFetching] = useState(false);
  const [saving, setSaving] = useState(false);
  const [formErr, setFormErr] = useState<string | null>(null);

  const refreshVoice = useCallback(async () => {
    try {
      setVoice(await voiceConfig());
      setVoiceErr(null);
    } catch (e) {
      setVoice(null);
      setVoiceErr(errMessage(e));
    }
  }, []);

  const refreshMcpClients = useCallback(async () => {
    try {
      setMcpClientOptions(await listMcpClients());
      setMcpErr(null);
    } catch (e) {
      setMcpClientOptions([]);
      setMcpErr(errMessage(e));
    }
  }, []);

  const refreshMcp = useCallback(async (clientId: string) => {
    try {
      setMcp(await mcpStatus(clientId));
      setMcpErr(null);
    } catch (e) {
      setMcp(null);
      setMcpErr(errMessage(e));
    }
  }, []);

  useEffect(() => {
    void refreshVoice();
    void refreshMcpClients();
  }, [refreshVoice, refreshMcpClients]);

  useEffect(() => {
    void refreshMcp(selectedMcpClientId);
  }, [refreshMcp, selectedMcpClientId]);

  const voiceReady = !!voice?.configured;
  const selectedMcpStatus = mcp?.client_id === selectedMcpClientId ? mcp : null;
  const selectedMcpClient = mcpClientOptions.find((c) => c.id === selectedMcpClientId);
  const mcpClientName =
    selectedMcpStatus?.client_name ?? selectedMcpClient?.name ?? "selected client";
  const mcpReady = !!selectedMcpStatus?.registered && !!selectedMcpStatus?.up_to_date;

  /** Fetch voices with an explicit key (empty string reuses the stored key). */
  const fetchVoices = useCallback(
    async (key: string) => {
      setFetching(true);
      setFormErr(null);
      try {
        const list = await listVoices(key ? key : undefined);
        setVoices(list);
        setSelectedVoiceId((prev) => {
          const current = voice?.voice_id;
          if (current && list.some((v) => v.voice_id === current)) return current;
          if (prev && list.some((v) => v.voice_id === prev)) return prev;
          return list[0]?.voice_id ?? "";
        });
        if (list.length === 0) setFormErr("No voices found on this account.");
      } catch (e) {
        setVoices([]);
        setFormErr(errMessage(e));
      } finally {
        setFetching(false);
      }
    },
    [voice?.voice_id],
  );

  const openEdit = () => {
    setApiKeyInput("");
    setVoices([]);
    setSelectedVoiceId("");
    setFormErr(null);
    setEditing(true);
    // Already configured? Reuse the stored key so voices load immediately.
    if (voiceReady) void fetchVoices("");
  };

  const closeEdit = () => {
    setEditing(false);
    setApiKeyInput("");
    setVoices([]);
    setSelectedVoiceId("");
    setFormErr(null);
  };

  const doSave = async () => {
    if (!selectedVoiceId) return;
    setSaving(true);
    setFormErr(null);
    try {
      const key = apiKeyInput.trim();
      const name = voices.find((v) => v.voice_id === selectedVoiceId)?.name ?? null;
      const info = await saveVoiceConfig({
        apiKey: key ? key : undefined,
        voiceId: selectedVoiceId,
        voiceName: name,
      });
      setVoice(info);
      setVoiceErr(null);
      closeEdit();
    } catch (e) {
      setFormErr(errMessage(e));
    } finally {
      setSaving(false);
    }
  };

  const doRegister = async () => {
    setBusy(true);
    try {
      setMcp(await registerMcp(selectedMcpClientId));
      setMcpErr(null);
    } catch (e) {
      setMcpErr(errMessage(e));
    } finally {
      setBusy(false);
    }
  };

  const doUnregister = async () => {
    setBusy(true);
    try {
      setMcp(await unregisterMcp(selectedMcpClientId));
      setMcpErr(null);
    } catch (e) {
      setMcpErr(errMessage(e));
    } finally {
      setBusy(false);
    }
  };

  const canFetch = voiceReady || apiKeyInput.trim().length > 0;

  return (
    <section className="screen screen--setup">
      <header className="setup-head">
        <h1 className="setup-title">Setup</h1>
        <button className="setup-close" onClick={onClose} aria-label="Close settings">
          ✕
        </button>
      </header>

      <div className="setup-body">
        {/* ElevenLabs voice */}
        <div className="setup-card">
          <div className="setup-card__head">
            <span className={`setup-dot ${voiceReady ? "setup-dot--ok" : "setup-dot--warn"}`} />
            <h2 className="setup-card__title">ElevenLabs voice</h2>
          </div>

          {!editing && (
            <>
              {voiceReady ? (
                <p className="setup-card__body">
                  Ready — <strong>{voice?.voice_name ?? voice?.voice_id}</strong>
                  <br />
                  <span className="setup-muted">
                    model {voice?.model_id}
                    {voice && !voice.enabled ? " · disabled in config" : ""}
                  </span>
                </p>
              ) : (
                <p className="setup-card__body">
                  Not configured. Add your ElevenLabs API key and pick a voice.
                  {voiceErr && <span className="setup-err">{voiceErr}</span>}
                </p>
              )}
              <div className="setup-actions">
                <button className="setup-btn" onClick={openEdit}>
                  {voiceReady ? "Change" : "Set up"}
                </button>
                <button
                  className="setup-btn setup-btn--ghost"
                  onClick={() => void refreshVoice()}
                >
                  Recheck
                </button>
              </div>
            </>
          )}

          {editing && (
            <div className="setup-form">
              <label className="setup-field">
                <span className="setup-label">API key</span>
                <input
                  className="setup-input"
                  type="password"
                  autoComplete="off"
                  spellCheck={false}
                  placeholder={voiceReady ? "Leave blank to keep current key" : "sk_..."}
                  value={apiKeyInput}
                  onChange={(e) => setApiKeyInput(e.target.value)}
                />
              </label>

              <div className="setup-actions">
                <button
                  className="setup-btn setup-btn--ghost"
                  onClick={() => void fetchVoices(apiKeyInput.trim())}
                  disabled={fetching || saving || !canFetch}
                >
                  {fetching ? "Fetching…" : voices.length ? "Refresh voices" : "Fetch voices"}
                </button>
              </div>

              {voices.length > 0 && (
                <label className="setup-field">
                  <span className="setup-label">Voice</span>
                  <select
                    className="setup-select"
                    value={selectedVoiceId}
                    onChange={(e) => setSelectedVoiceId(e.target.value)}
                    disabled={saving}
                  >
                    {voices.map((v) => (
                      <option key={v.voice_id} value={v.voice_id}>
                        {v.name}
                        {v.category ? ` (${v.category})` : ""}
                      </option>
                    ))}
                  </select>
                </label>
              )}

              {formErr && <p className="setup-err">{formErr}</p>}

              <div className="setup-actions">
                <button
                  className="setup-btn"
                  onClick={() => void doSave()}
                  disabled={saving || fetching || !selectedVoiceId}
                >
                  {saving ? "Saving…" : "Save"}
                </button>
                <button
                  className="setup-btn setup-btn--ghost"
                  onClick={closeEdit}
                  disabled={saving}
                >
                  Cancel
                </button>
              </div>
              <p className="setup-muted setup-hint">
                Your key is stored locally in{" "}
                <code>~/.copilot/elevenlabs/config.json</code> and never leaves the app
                except to ElevenLabs.
              </p>
            </div>
          )}
        </div>

        {/* MCP registration */}
        <div className="setup-card">
          <div className="setup-card__head">
            <span className={`setup-dot ${mcpReady ? "setup-dot--ok" : "setup-dot--warn"}`} />
            <h2 className="setup-card__title">MCP server</h2>
          </div>

          <label className="setup-field">
            <span className="setup-label">Client</span>
            <select
              className="setup-select"
              value={selectedMcpClientId}
              onChange={(e) => setSelectedMcpClientId(e.target.value)}
              disabled={busy}
            >
              {mcpClientOptions.length ? (
                mcpClientOptions.map((client) => (
                  <option key={client.id} value={client.id}>
                    {client.name}
                  </option>
                ))
              ) : (
                <option value="copilot">Copilot CLI</option>
              )}
            </select>
          </label>

          {selectedMcpClient?.description && (
            <p className="setup-muted setup-hint">{selectedMcpClient.description}</p>
          )}

          {selectedMcpStatus?.registered ? (
            selectedMcpStatus.up_to_date ? (
              <p className="setup-card__body">
                Registered with {mcpClientName}.
                <br />
                <span className="setup-muted setup-path">{selectedMcpStatus.server_path}</span>
              </p>
            ) : (
              <p className="setup-card__body">
                Registered, but the path is out of date. Update it to point at this
                build.
                <br />
                <span className="setup-muted setup-path">{selectedMcpStatus.server_path}</span>
              </p>
            )
          ) : (
            <p className="setup-card__body">
              Not registered yet. Register so {mcpClientName} can call you.
              <br />
              <span className="setup-muted setup-path">{selectedMcpStatus?.server_path}</span>
            </p>
          )}
          <p className="setup-muted setup-hint">
            Config:{" "}
            <span className="setup-path">
              {selectedMcpStatus?.config_path ?? selectedMcpClient?.config_path}
            </span>
          </p>

          {selectedMcpStatus && !selectedMcpStatus.server_exists && (
            <p className="setup-err">
              MCP binary not found. Build it: <code>cargo build -p voice-mcp-server</code>
            </p>
          )}
          {mcpErr && <p className="setup-err">{mcpErr}</p>}

          <div className="setup-actions">
            {mcpReady ? (
              <button
                className="setup-btn setup-btn--danger"
                onClick={() => void doUnregister()}
                disabled={busy}
              >
                Unregister
              </button>
            ) : (
              <button
                className="setup-btn"
                onClick={() => void doRegister()}
                disabled={busy || (selectedMcpStatus ? !selectedMcpStatus.server_exists : false)}
              >
                {selectedMcpStatus?.registered ? "Update path" : `Register with ${mcpClientName}`}
              </button>
            )}
            <button
              className="setup-btn setup-btn--ghost"
              onClick={() => void refreshMcp(selectedMcpClientId)}
              disabled={busy}
            >
              Recheck
            </button>
          </div>
          <p className="setup-muted setup-hint">
            {selectedMcpStatus?.restart_hint ??
              selectedMcpClient?.restart_hint ??
              "After registering, restart your agent client so it picks up the new server."}
          </p>
        </div>

        <div className="setup-card">
          <div className="setup-card__head">
            <span className="setup-dot" />
            <h2 className="setup-card__title">Microphone</h2>
          </div>
          <p className="setup-card__body setup-muted">
            The first time the agent asks you to speak, macOS will prompt for microphone
            access. Allow it — you can change this later in System Settings → Privacy &
            Security → Microphone.
          </p>
        </div>
      </div>
    </section>
  );
}

function errMessage(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return String(e);
}
