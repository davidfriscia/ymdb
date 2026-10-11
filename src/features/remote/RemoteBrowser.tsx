import { useCallback, useState } from "react";
import { CommandFailedError, describeError } from "../../shared/ipc";
import { useLoader } from "../../shared/useLoader";
import { formatSize } from "../browser/detail/format";
import { browseRemoteDirectory } from "./remoteApi";

interface RemoteBrowserProps {
  connectionId: string;
}

/**
 * Read-only browsing of a connection's directories, to check paths. Each
 * listing opens a connection. A passphrase entered here stays in memory
 * while the browser is open and is never saved.
 */
export function RemoteBrowser({ connectionId }: RemoteBrowserProps) {
  const [target, setTarget] = useState("");
  const [typed, setTyped] = useState("");
  const [passphrase, setPassphrase] = useState<string | null>(null);
  const [typedPassphrase, setTypedPassphrase] = useState("");
  const load = useCallback(
    () => browseRemoteDirectory(connectionId, target, passphrase),
    [connectionId, target, passphrase],
  );
  const { state, reload } = useLoader(load);

  const remoteError =
    state.status === "error" &&
    state.error instanceof CommandFailedError &&
    state.error.detail.kind === "remote"
      ? state.error.detail.error
      : null;
  const needsPassphrase =
    remoteError?.kind === "passphraseRequired" || remoteError?.kind === "passphraseIncorrect";

  function open(path: string) {
    setTyped(path);
    setTarget(path);
  }

  return (
    <div className="remote-browser" role="group" aria-label="Remote directories">
      <form
        className="remote-browser__path"
        onSubmit={(event) => {
          event.preventDefault();
          open(typed.trim());
        }}
      >
        <input
          className="input"
          aria-label="Remote path"
          placeholder="/volume1/mame"
          value={typed}
          onChange={(event) => {
            setTyped(event.target.value);
          }}
        />
        <button type="submit" className="button button--secondary button--small">
          Open
        </button>
        {state.status === "ready" && state.value.parent !== null && (
          <button
            type="button"
            className="button button--secondary button--small"
            onClick={() => {
              if (state.value.parent !== null) open(state.value.parent);
            }}
          >
            Up
          </button>
        )}
      </form>

      {state.status === "loading" && (
        <p className="message message--muted" role="status">
          Connecting…
        </p>
      )}
      {needsPassphrase && (
        <form
          className="remote-browser__path"
          aria-label="Private key passphrase"
          onSubmit={(event) => {
            event.preventDefault();
            setPassphrase(typedPassphrase);
            reload();
            setTypedPassphrase("");
          }}
        >
          <span className="message">
            {remoteError.kind === "passphraseIncorrect"
              ? "The passphrase is not correct."
              : "The private key needs its passphrase."}
          </span>
          <input
            className="input"
            type="password"
            aria-label="Passphrase"
            autoComplete="off"
            value={typedPassphrase}
            onChange={(event) => {
              setTypedPassphrase(event.target.value);
            }}
          />
          <button type="submit" className="button button--small" disabled={typedPassphrase === ""}>
            Connect
          </button>
        </form>
      )}
      {state.status === "error" && !needsPassphrase && (
        <div className="state-message" role="alert">
          <p className="message message--error">{describeError(state.error)}</p>
          <button type="button" className="button button--secondary button--small" onClick={reload}>
            Retry
          </button>
        </div>
      )}
      {state.status === "ready" && (
        <>
          <p className="message message--muted">
            <span className="code">{state.value.path}</span>: {state.value.entries.length}{" "}
            {state.value.entries.length === 1 ? "entry" : "entries"}
            {state.value.truncated && " (only the first entries are shown)"}
          </p>
          <ul className="remote-browser__list">
            {state.value.entries.map((entry) => {
              const path = `${state.value.path === "/" ? "" : state.value.path}/${entry.name}`;
              return (
                <li key={entry.name} className="remote-browser__entry">
                  {entry.kind === "directory" ? (
                    <button
                      type="button"
                      className="link-button"
                      onClick={() => {
                        open(path);
                      }}
                    >
                      {entry.name}/
                    </button>
                  ) : (
                    <span className="code">{entry.name}</span>
                  )}
                  <span className="message--muted">
                    {entry.kind === "file" ? (formatSize(entry.size) ?? "") : entry.kind}
                  </span>
                </li>
              );
            })}
          </ul>
        </>
      )}
    </div>
  );
}
