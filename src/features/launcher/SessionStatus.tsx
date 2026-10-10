import { useState } from "react";
import { describeError } from "../../shared/ipc";
import type { SessionInfo } from "./launcherApi";

const timeFormat = new Intl.DateTimeFormat("en-US", { hour: "2-digit", minute: "2-digit" });

interface SessionStatusProps {
  session: SessionInfo;
  onStop: (id: number) => Promise<void>;
}

/** State of the latest session of a machine or software. */
export function SessionStatus({ session, onStop }: SessionStatusProps) {
  const [confirming, setConfirming] = useState(false);
  const [stopping, setStopping] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const { state } = session;

  async function stop() {
    setStopping(true);
    setError(null);
    try {
      await onStop(session.id);
      setConfirming(false);
    } catch (stopError) {
      setError(describeError(stopError));
    } finally {
      setStopping(false);
    }
  }

  return (
    <div className="session-status" role="status" aria-label="Session">
      {state.state === "running" && (
        <>
          <p className="session-status__line">
            <span className="badge badge--success">Running</span> {session.installationLabel} is
            running since {timeFormat.format(session.startedAt)}.
          </p>
          {confirming ? (
            <div className="session-status__confirm">
              <p className="message message--warning">
                Stop MAME now? The emulation is terminated immediately: unsaved progress and NVRAM
                changes of this session are lost.
              </p>
              <div className="actions">
                <button
                  type="button"
                  className="button"
                  disabled={stopping}
                  onClick={() => void stop()}
                >
                  {stopping ? "Stopping…" : "Stop now"}
                </button>
                <button
                  type="button"
                  className="button button--secondary"
                  disabled={stopping}
                  onClick={() => {
                    setConfirming(false);
                  }}
                >
                  Keep running
                </button>
              </div>
            </div>
          ) : (
            <button
              type="button"
              className="button button--secondary button--small"
              onClick={() => {
                setConfirming(true);
              }}
            >
              Stop
            </button>
          )}
        </>
      )}
      {state.state === "stopped" && (
        <p className="session-status__line">
          <span className="badge">Stopped</span> The session was stopped.
        </p>
      )}
      {state.state === "exited" && (
        <>
          <p className="session-status__line">
            <span className={state.success ? "badge badge--success" : "badge badge--error"}>
              {state.success ? "Finished" : "Failed"}
            </span>{" "}
            {state.message}
            {!state.success && state.exitCode !== null && ` (exit code ${String(state.exitCode)})`}
          </p>
          {!state.success && state.logTail && (
            <details className="raw-output">
              <summary>MAME output</summary>
              <pre>{state.logTail}</pre>
            </details>
          )}
        </>
      )}
      {error && <p className="message message--error">{error}</p>}
    </div>
  );
}
