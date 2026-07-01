import { useCallback, useEffect, useState } from "react";

import {
  mcpStatus,
  registerMcp,
  unregisterMcp,
  voiceConfig,
  type McpStatus,
  type VoiceConfigInfo,
} from "../ipc";

/**
 * First-run / settings panel shown from the idle screen. Surfaces the two
 * things a user must get right for a call to work: an ElevenLabs voice config
 * and the MCP server registered with the Copilot CLI.
 */
export function SetupPanel({ onClose }: { onClose: () => void }) {
  const [voice, setVoice] = useState<VoiceConfigInfo | null>(null);
  const [voiceErr, setVoiceErr] = useState<string | null>(null);
  const [mcp, setMcp] = useState<McpStatus | null>(null);
  const [mcpErr, setMcpErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const refreshVoice = useCallback(async () => {
    try {
      setVoice(await voiceConfig());
      setVoiceErr(null);
    } catch (e) {
      setVoice(null);
      setVoiceErr(errMessage(e));
    }
  }, []);

  const refreshMcp = useCallback(async () => {
    try {
      setMcp(await mcpStatus());
      setMcpErr(null);
    } catch (e) {
      setMcp(null);
      setMcpErr(errMessage(e));
    }
  }, []);

  useEffect(() => {
    void refreshVoice();
    void refreshMcp();
  }, [refreshVoice, refreshMcp]);

  const doRegister = async () => {
    setBusy(true);
    try {
      setMcp(await registerMcp());
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
      setMcp(await unregisterMcp());
      setMcpErr(null);
    } catch (e) {
      setMcpErr(errMessage(e));
    } finally {
      setBusy(false);
    }
  };

  const voiceReady = !!voice?.configured;
  const mcpReady = !!mcp?.registered && !!mcp?.up_to_date;

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
              Not configured. Create{" "}
              <code>~/.copilot/elevenlabs/config.json</code> with your{" "}
              <code>apiKey</code> and a <code>voiceId</code>.
              {voiceErr && <span className="setup-err">{voiceErr}</span>}
            </p>
          )}
          <button className="setup-btn setup-btn--ghost" onClick={() => void refreshVoice()}>
            Recheck
          </button>
        </div>

        {/* MCP registration */}
        <div className="setup-card">
          <div className="setup-card__head">
            <span className={`setup-dot ${mcpReady ? "setup-dot--ok" : "setup-dot--warn"}`} />
            <h2 className="setup-card__title">Copilot MCP server</h2>
          </div>

          {mcp?.registered ? (
            mcp.up_to_date ? (
              <p className="setup-card__body">
                Registered with Copilot.
                <br />
                <span className="setup-muted setup-path">{mcp.server_path}</span>
              </p>
            ) : (
              <p className="setup-card__body">
                Registered, but the path is out of date. Update it to point at this
                build.
                <br />
                <span className="setup-muted setup-path">{mcp.server_path}</span>
              </p>
            )
          ) : (
            <p className="setup-card__body">
              Not registered yet. Register so the agent can call you.
              <br />
              <span className="setup-muted setup-path">{mcp?.server_path}</span>
            </p>
          )}

          {mcp && !mcp.server_exists && (
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
                disabled={busy || (mcp ? !mcp.server_exists : false)}
              >
                {mcp?.registered ? "Update path" : "Register with Copilot"}
              </button>
            )}
            <button
              className="setup-btn setup-btn--ghost"
              onClick={() => void refreshMcp()}
              disabled={busy}
            >
              Recheck
            </button>
          </div>
          <p className="setup-muted setup-hint">
            After registering, restart your Copilot CLI session so it picks up the new
            server.
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
