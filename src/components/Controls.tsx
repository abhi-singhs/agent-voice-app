interface Props {
  muted: boolean;
  pushToTalk: boolean;
  onToggleMute: () => void;
  onSetPushToTalk: (value: boolean) => void;
  onHangUp: () => void;
}

export function Controls({ muted, pushToTalk, onToggleMute, onSetPushToTalk, onHangUp }: Props) {
  return (
    <div className="controls">
      <button
        type="button"
        className={`control ${muted ? "control--on" : ""}`}
        onClick={onToggleMute}
        title={muted ? "Unmute microphone" : "Mute microphone"}
      >
        <span className="control__icon">{muted ? "🔇" : "🎙️"}</span>
        <span className="control__label">{muted ? "Muted" : "Mute"}</span>
      </button>

      <button
        type="button"
        className={`control ${pushToTalk ? "control--on" : ""}`}
        onClick={() => onSetPushToTalk(!pushToTalk)}
        title="Toggle push-to-talk"
      >
        <span className="control__icon">{pushToTalk ? "✋" : "🖐️"}</span>
        <span className="control__label">Push-to-talk</span>
      </button>

      <button
        type="button"
        className="control control--hangup"
        onClick={onHangUp}
        title="Hang up"
      >
        <span className="control__icon">📞</span>
        <span className="control__label">End</span>
      </button>
    </div>
  );
}
