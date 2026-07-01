interface Props {
  reason: string | null;
  onAnswer: () => void;
  onDecline: () => void;
}

export function RingScreen({ reason, onAnswer, onDecline }: Props) {
  return (
    <section className="screen screen--ring">
      <div className="ring-avatar" aria-hidden>
        <span className="ring-avatar__glyph">🤖</span>
        <span className="ring-pulse" />
      </div>

      <h1 className="ring-title">Copilot is calling…</h1>
      <p className="ring-reason">{reason ?? "Incoming voice call"}</p>

      <div className="ring-actions">
        <button type="button" className="round-btn round-btn--decline" onClick={onDecline}>
          <span className="round-btn__icon">✕</span>
          <span className="round-btn__label">Decline</span>
        </button>
        <button type="button" className="round-btn round-btn--answer" onClick={onAnswer}>
          <span className="round-btn__icon">📞</span>
          <span className="round-btn__label">Answer</span>
        </button>
      </div>
    </section>
  );
}
