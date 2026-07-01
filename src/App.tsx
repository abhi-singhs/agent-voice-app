import { useCallMachine } from "./callMachine";
import { CallScreen } from "./components/CallScreen";
import { IdleScreen } from "./components/IdleScreen";
import { RingScreen } from "./components/RingScreen";
import "./App.css";

function App() {
  const {
    state,
    answer,
    decline,
    hangUp,
    sendReply,
    toggleMute,
    setPushToTalk,
    setBargeIn,
    interrupt,
    startTalking,
    stopTalking,
  } = useCallMachine();

  return (
    <main className="app">
      {state.phase === "idle" && <IdleScreen />}

      {state.phase === "incoming" && (
        <RingScreen reason={state.reason} onAnswer={answer} onDecline={decline} />
      )}

      {(state.phase === "connected" ||
        state.phase === "speaking" ||
        state.phase === "listening" ||
        state.phase === "ended") && (
        <CallScreen
          state={state}
          onSendReply={sendReply}
          onToggleMute={toggleMute}
          onSetPushToTalk={setPushToTalk}
          onSetBargeIn={setBargeIn}
          onInterrupt={interrupt}
          onStartTalking={startTalking}
          onStopTalking={stopTalking}
          onHangUp={hangUp}
        />
      )}
    </main>
  );
}

export default App;
