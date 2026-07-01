import { useEffect, useRef } from "react";

import type { TranscriptLine } from "../callMachine";

interface Props {
  lines: TranscriptLine[];
}

export function Transcript({ lines }: Props) {
  const endRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    endRef.current?.scrollIntoView({ behavior: "smooth", block: "end" });
  }, [lines]);

  if (lines.length === 0) {
    return <div className="transcript transcript--empty">No transcript yet.</div>;
  }

  return (
    <div className="transcript">
      {lines.map((line) => (
        <div key={line.key} className={`bubble bubble--${line.who}`}>
          <span className="bubble__who">{line.who === "agent" ? "Copilot" : "You"}</span>
          <span className="bubble__text">{line.text}</span>
        </div>
      ))}
      <div ref={endRef} />
    </div>
  );
}
