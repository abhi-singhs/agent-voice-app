import { useState } from "react";

import { SetupPanel } from "./SetupPanel";

export function IdleScreen() {
  const [showSetup, setShowSetup] = useState(false);

  if (showSetup) {
    return <SetupPanel onClose={() => setShowSetup(false)} />;
  }

  return (
    <section className="screen screen--idle">
      <button
        className="idle-settings"
        onClick={() => setShowSetup(true)}
        aria-label="Open settings"
        title="Setup"
      >
        ⚙
      </button>
      <div className="idle-badge" aria-hidden>
        <span className="idle-badge__glyph">☎️</span>
      </div>
      <h1 className="idle-title">Copilot Voice Call</h1>
      <p className="idle-subtitle">Waiting for a call…</p>
      <p className="idle-hint">
        When the agent calls, this window rings and you can answer to start a spoken
        conversation.
      </p>
      <button className="idle-setup-link" onClick={() => setShowSetup(true)}>
        Setup &amp; status
      </button>
    </section>
  );
}
