interface Props {
  muted: boolean;
  pushToTalk: boolean;
  bargeIn: boolean;
  onToggleMute: () => void;
  onSetPushToTalk: (value: boolean) => void;
  onSetBargeIn: (value: boolean) => void;
  onHangUp: () => void;
}

export function Controls({
  muted,
  pushToTalk,
  bargeIn,
  onToggleMute,
  onSetPushToTalk,
  onSetBargeIn,
  onHangUp,
}: Props) {
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
        className={`control ${bargeIn ? "control--on" : ""}`}
        onClick={() => onSetBargeIn(!bargeIn)}
        title={
          bargeIn
            ? "Barge-in on: start talking to interrupt the agent"
            : "Barge-in off: use the Interrupt button to cut in"
        }
      >
        <span className="control__icon">{bargeIn ? "🗣️" : "🚫"}</span>
        <span className="control__label">Barge-in</span>
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
