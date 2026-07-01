import { useCallback, useEffect, useState } from "react";

import { clearCalls, listCalls, type CallRecord } from "../ipc";

/** Call-history viewer: lists past calls with expandable transcripts. */
export function HistoryPanel({ onClose }: { onClose: () => void }) {
  const [calls, setCalls] = useState<CallRecord[] | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [expanded, setExpanded] = useState<number | null>(null);

  const refresh = useCallback(async () => {
    try {
      setCalls(await listCalls(100));
      setErr(null);
    } catch (e) {
      setErr(errMessage(e));
      setCalls([]);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const onClear = async () => {
    try {
      await clearCalls();
      setExpanded(null);
      await refresh();
    } catch (e) {
      setErr(errMessage(e));
    }
  };

  return (
    <section className="screen screen--setup">
      <header className="setup-head">
        <h1 className="setup-title">Call history</h1>
        <button className="setup-close" onClick={onClose} aria-label="Close history">
          ✕
        </button>
      </header>

      <div className="setup-body">
        {err && <p className="setup-err">{err}</p>}

        {calls && calls.length === 0 && !err && (
          <p className="setup-muted history-empty">No calls yet.</p>
        )}

        {calls?.map((c, i) => {
          const open = expanded === i;
          return (
            <div className="history-item" key={`${c.started_at}-${i}`}>
              <button
                className="history-row"
                onClick={() => setExpanded(open ? null : i)}
                aria-expanded={open}
              >
                <span className="history-when">{formatWhen(c.started_at)}</span>
                <span className="history-meta">
                  {formatDuration(c.ended_at - c.started_at)} · {c.entries.length} turns
                </span>
                <span className="history-caret">{open ? "▾" : "▸"}</span>
              </button>
              {c.reason && <p className="history-reason">{c.reason}</p>}
              {open && (
                <div className="history-transcript">
                  {c.entries.map((e, j) => (
                    <div className={`bubble bubble--${e.who === "you" ? "you" : "agent"}`} key={j}>
                      <span className="bubble__who">{e.who === "you" ? "You" : "Agent"}</span>
                      <span className="bubble__text">{e.text}</span>
                    </div>
                  ))}
                  {c.outcome && <p className="history-outcome">{c.outcome}</p>}
                </div>
              )}
            </div>
          );
        })}
      </div>

      {calls && calls.length > 0 && (
        <button className="setup-btn setup-btn--danger history-clear" onClick={() => void onClear()}>
          Clear history
        </button>
      )}
    </section>
  );
}

function formatWhen(ms: number): string {
  const d = new Date(ms);
  return d.toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  });
}

function formatDuration(ms: number): string {
  const s = Math.max(0, Math.round(ms / 1000));
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  const rem = s % 60;
  return `${m}m ${rem}s`;
}

function errMessage(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return String(e);
}
