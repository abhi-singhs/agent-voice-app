import { useEffect, useState } from "react";

import { getDnd, setDnd } from "../ipc";
import { AgentAvatar } from "./AgentAvatar";
import { HistoryPanel } from "./HistoryPanel";
import { SetupPanel } from "./SetupPanel";

export function IdleScreen() {
  const [view, setView] = useState<"idle" | "setup" | "history">("idle");
  const [dnd, setDndState] = useState(false);

  useEffect(() => {
    getDnd()
      .then(setDndState)
      .catch(() => {});
  }, []);

  const toggleDnd = () => {
    const next = !dnd;
    setDndState(next);
    void setDnd(next).catch(() => setDndState(!next));
  };

  if (view === "setup") {
    return <SetupPanel onClose={() => setView("idle")} />;
  }
  if (view === "history") {
    return <HistoryPanel onClose={() => setView("idle")} />;
  }

  return (
    <section className="screen screen--idle">
      <button
        className="idle-settings"
        onClick={() => setView("setup")}
        aria-label="Open settings"
        title="Setup"
      >
        ⚙
      </button>
      <div className="idle-badge" aria-hidden>
        <AgentAvatar className="idle-badge__glyph" />
      </div>
      <h1 className="idle-title">Agent Voice App</h1>
      <p className="idle-subtitle">{dnd ? "Do Not Disturb" : "Waiting for a call…"}</p>
      <p className="idle-hint">
        {dnd
          ? "Incoming calls are automatically declined."
          : "When the agent calls, this window rings and you can answer to start a spoken conversation."}
      </p>
      <button
        className={`idle-dnd ${dnd ? "idle-dnd--on" : ""}`}
        onClick={toggleDnd}
        aria-pressed={dnd}
      >
        {dnd ? "🔕 Do Not Disturb: On" : "🔔 Do Not Disturb: Off"}
      </button>
      <div className="idle-links">
        <button className="idle-setup-link" onClick={() => setView("setup")}>
          Setup &amp; status
        </button>
        <button className="idle-setup-link" onClick={() => setView("history")}>
          Call history
        </button>
      </div>
    </section>
  );
}
