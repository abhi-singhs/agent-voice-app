import { useState } from "react";

import type { CallState } from "../callMachine";
import { Controls } from "./Controls";
import { Transcript } from "./Transcript";

interface Props {
  state: CallState;
  onSendReply: (text: string) => void;
  onToggleMute: () => void;
  onSetPushToTalk: (value: boolean) => void;
  onHangUp: () => void;
}

function statusText(phase: CallState["phase"]): string {
  switch (phase) {
    case "speaking":
      return "Copilot is speaking…";
    case "listening":
      return "Listening for your reply…";
    case "ended":
      return "Call ended";
    default:
      return "Connected";
  }
}

export function CallScreen({
  state,
  onSendReply,
  onToggleMute,
  onSetPushToTalk,
  onHangUp,
}: Props) {
  const [draft, setDraft] = useState("");
  const listening = state.phase === "listening";
  const ended = state.phase === "ended";

  const submit = () => {
    if (!draft.trim()) return;
    onSendReply(draft);
    setDraft("");
  };

  return (
    <section className="screen screen--call">
      <header className="call-header">
        <span className={`status-dot status-dot--${state.phase}`} />
        <span className="call-status">{statusText(state.phase)}</span>
      </header>

      {state.caption && (
        <div className={`caption ${state.phase === "speaking" ? "caption--live" : ""}`}>
          {state.caption}
        </div>
      )}

      <Transcript lines={state.transcript} />

      {!ended && (
        <div className={`reply ${listening ? "reply--active" : ""}`}>
          <input
            className="reply__input"
            value={draft}
            placeholder={listening ? "Type your reply…" : "Waiting for Copilot…"}
            disabled={!listening}
            onChange={(e) => setDraft(e.currentTarget.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") submit();
            }}
          />
          <button
            type="button"
            className="reply__send"
            disabled={!listening || !draft.trim()}
            onClick={submit}
          >
            Send
          </button>
        </div>
      )}

      {ended ? (
        <div className="ended-note">{state.endedReason ?? "Call ended."}</div>
      ) : (
        <Controls
          muted={state.muted}
          pushToTalk={state.pushToTalk}
          onToggleMute={onToggleMute}
          onSetPushToTalk={onSetPushToTalk}
          onHangUp={onHangUp}
        />
      )}
    </section>
  );
}
