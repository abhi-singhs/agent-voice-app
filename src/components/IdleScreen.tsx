export function IdleScreen() {
  return (
    <section className="screen screen--idle">
      <div className="idle-badge" aria-hidden>
        <span className="idle-badge__glyph">☎️</span>
      </div>
      <h1 className="idle-title">Copilot Voice Call</h1>
      <p className="idle-subtitle">Waiting for a call…</p>
      <p className="idle-hint">
        When the agent calls, this window rings and you can answer to start a spoken
        conversation.
      </p>
    </section>
  );
}
