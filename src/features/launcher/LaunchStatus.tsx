import { useId, useState } from "react";
import { describeError } from "../../shared/ipc";
import { Disclosure } from "./Disclosure";
import type { SessionInfo } from "./launcherApi";

const timeFormat = new Intl.DateTimeFormat("en-US", { hour: "2-digit", minute: "2-digit" });

interface LaunchStatusProps {
  /** Messages about the launch configuration or the last start attempt. */
  messages: { text: string; tone: "error" | "warning" }[];
  /** The latest session of the machine or software, if any. */
  session: SessionInfo | undefined;
  onStop: (id: number) => Promise<void>;
}

/**
 * The status of the last launch below the launch bar: the messages and a
 * summary line of the latest session, always visible, and its details and
 * MAME output in a collapsed body. Shown only when there is something to say.
 */
export function LaunchStatus({ messages, session, onStop }: LaunchStatusProps) {
  const [expanded, setExpanded] = useState(false);
  const bodyId = useId();
  if (messages.length === 0 && !session) return null;

  return (
    <section className="launch-section launch-status" aria-label="Launch status">
      {messages.map((message) => (
        <p key={message.text} className={`message message--${message.tone}`} role="alert">
          {message.text}
        </p>
      ))}
      {session && (
        <>
          <div className="launch-section__header">
            <Disclosure
              expanded={expanded}
              controls={bodyId}
              onToggle={() => {
                setExpanded(!expanded);
              }}
            >
              Last launch
            </Disclosure>
            <SessionSummary key={session.id} session={session} onStop={onStop} />
          </div>
          <div id={bodyId} className="launch-section__body" hidden={!expanded}>
            <SessionDetails session={session} />
          </div>
        </>
      )}
    </section>
  );
}

/** The state of the session in one line, with Stop while it runs. */
function SessionSummary({
  session,
  onStop,
}: {
  session: SessionInfo;
  onStop: (id: number) => Promise<void>;
}) {
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
          {!confirming && (
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
          {confirming && (
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
          )}
        </>
      )}
      {state.state === "stopped" && (
        <p className="session-status__line">
          <span className="badge">Stopped</span> The session was stopped.
        </p>
      )}
      {state.state === "exited" && (
        <p className="session-status__line">
          <span className={state.success ? "badge badge--success" : "badge badge--error"}>
            {state.success ? "Completed" : "Failed"}
          </span>{" "}
          {state.message}
          {!state.success && state.exitCode !== null && ` (exit code ${String(state.exitCode)})`}
        </p>
      )}
      {error && <p className="message message--error">{error}</p>}
    </div>
  );
}

/** When and where the session ran, its exit code and the end of MAME's output. */
function SessionDetails({ session }: { session: SessionInfo }) {
  const { state } = session;
  return (
    <>
      <dl className="details-list">
        <div className="details-list__row">
          <dt>Installation</dt>
          <dd>{session.installationLabel}</dd>
        </div>
        <div className="details-list__row">
          <dt>Started</dt>
          <dd>{timeFormat.format(session.startedAt)}</dd>
        </div>
        {state.state !== "running" && (
          <div className="details-list__row">
            <dt>Finished</dt>
            <dd>{timeFormat.format(state.finishedAt)}</dd>
          </div>
        )}
        {state.state === "exited" && state.exitCode !== null && (
          <div className="details-list__row">
            <dt>Exit code</dt>
            <dd className="code">{state.exitCode}</dd>
          </div>
        )}
      </dl>
      <div className="session-status__output">
        <h4 className="session-status__output-title">MAME output</h4>
        {state.state === "exited" && state.logTail ? (
          <pre aria-label="MAME output">{state.logTail}</pre>
        ) : (
          <p className="message message--muted">
            {state.state === "running"
              ? "The output of MAME is shown when the session ends."
              : "No output of MAME was captured for this session."}
          </p>
        )}
      </div>
    </>
  );
}
