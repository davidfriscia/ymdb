import { useCallback, useState } from "react";
import { describeError } from "../../shared/ipc";
import { useLoader } from "../../shared/useLoader";
import { ConnectionForm } from "./ConnectionForm";
import { RemoteBrowser } from "./RemoteBrowser";
import {
  approveRemoteHostKey,
  forgetRemoteHostKey,
  getRemoteConnections,
  removeRemoteConnection,
  testRemoteConnection,
  type ConnectionInfo,
  type ConnectionSummary,
  type RemoteError,
} from "./remoteApi";
import { PROVIDER_LABELS, describeRemoteError } from "./remoteMessages";

/**
 * Settings page section: the remote connections shared by every MAME
 * installation. Only SFTP is available.
 */
export function RemoteConnectionsSettings() {
  const [version, setVersion] = useState(0);
  const load = useCallback(() => getRemoteConnections(), []);
  const { state, latest, reload } = useLoader(load, version);
  const [adding, setAdding] = useState(false);
  const changed = useCallback(() => {
    setVersion((value) => value + 1);
  }, []);

  return (
    <section className="panel" aria-label="Remote Connections">
      <p className="message message--muted">
        Repositories from which YMDB will retrieve ROM, CHD and sample files. Connections are shared
        by every MAME installation; each installation chooses what to use in its ROM Manager
        settings on the MAME page. Remote repositories are optional and YMDB never modifies them.
        Supported: SFTP over SSH with a private key.
      </p>
      {state.status === "error" && (
        <div className="state-message" role="alert">
          <p className="message message--error">
            The remote connections could not be loaded: {describeError(state.error)}
          </p>
          <button type="button" className="button button--secondary" onClick={reload}>
            Retry
          </button>
        </div>
      )}
      {latest === null && state.status === "loading" && (
        <p className="message message--muted">Loading…</p>
      )}
      {latest?.loadError && (
        <p className="message message--error" role="alert">
          {latest.loadError}. Fix or remove the file to manage connections again; YMDB does not
          replace it.
        </p>
      )}
      {latest?.connections.length === 0 && !latest.loadError && (
        <p className="message message--muted">No remote connection.</p>
      )}
      {/* First, so that it stays reachable however many connections are listed. */}
      {adding ? (
        <ConnectionForm
          connection={null}
          onSaved={() => {
            setAdding(false);
            changed();
          }}
          onCancel={() => {
            setAdding(false);
          }}
        />
      ) : (
        <div className="actions">
          <button
            type="button"
            className="button"
            disabled={latest?.loadError !== null}
            onClick={() => {
              setAdding(true);
            }}
          >
            Add connection…
          </button>
        </div>
      )}
      {latest && latest.connections.length > 0 && (
        <ul className="remote-connections" aria-label="Remote connections">
          {latest.connections.map((connection) => (
            <ConnectionItem key={connection.id} connection={connection} onChanged={changed} />
          ))}
        </ul>
      )}
    </section>
  );
}

type Outcome =
  | { kind: "success"; info: ConnectionInfo }
  | { kind: "remote"; error: RemoteError }
  | { kind: "error"; message: string };

type Busy = "test" | "approve" | "forget" | "remove";

interface ConnectionItemProps {
  connection: ConnectionSummary;
  onChanged: () => void;
}

function ConnectionItem({ connection, onChanged }: ConnectionItemProps) {
  const settings = connection.providerConfig;
  const [busy, setBusy] = useState<Busy | null>(null);
  const [outcome, setOutcome] = useState<Outcome | null>(null);
  const [passphrasePrompt, setPassphrasePrompt] = useState<{ incorrect: boolean } | null>(null);
  const [passphrase, setPassphrase] = useState("");
  const [editing, setEditing] = useState(false);
  const [browsing, setBrowsing] = useState(false);
  const [confirmingRemoval, setConfirmingRemoval] = useState(false);
  const [confirmingForget, setConfirmingForget] = useState(false);
  const titleId = `remote-connection-${connection.id}`;

  /** Runs an action; resolves to whether it succeeded. */
  async function run(action: Busy, work: () => Promise<void>): Promise<boolean> {
    setBusy(action);
    try {
      await work();
      return true;
    } catch (error) {
      setOutcome({ kind: "error", message: describeError(error) });
      return false;
    } finally {
      setBusy(null);
    }
  }

  // The passphrase is sent for this test only and cleared afterwards.
  const test = (withPassphrase: string | null) =>
    run("test", async () => {
      setOutcome(null);
      setPassphrase("");
      const result = await testRemoteConnection(connection.id, withPassphrase);
      if (result.status === "success") {
        setPassphrasePrompt(null);
        setOutcome({ kind: "success", info: result.info });
      } else if (
        result.error.kind === "passphraseRequired" ||
        result.error.kind === "passphraseIncorrect"
      ) {
        setPassphrasePrompt({ incorrect: result.error.kind === "passphraseIncorrect" });
      } else {
        setPassphrasePrompt(null);
        setOutcome({ kind: "remote", error: result.error });
      }
    });

  // After an approval, connect again to complete the test.
  async function approve(fingerprint: string) {
    const approved = await run("approve", async () => {
      await approveRemoteHostKey(connection.id, fingerprint);
      onChanged();
    });
    if (approved) await test(null);
  }

  const forget = () =>
    run("forget", async () => {
      await forgetRemoteHostKey(connection.id);
      setConfirmingForget(false);
      setOutcome(null);
      onChanged();
    });

  const remove = () =>
    run("remove", async () => {
      await removeRemoteConnection(connection.id);
      onChanged();
    });

  if (editing) {
    return (
      <li className="remote-connections__item">
        <ConnectionForm
          connection={connection}
          onSaved={() => {
            setEditing(false);
            setOutcome(null);
            onChanged();
          }}
          onCancel={() => {
            setEditing(false);
          }}
        />
      </li>
    );
  }

  return (
    <li className="remote-connections__item" aria-labelledby={titleId} aria-busy={busy !== null}>
      <p id={titleId} className="installation-list__title">
        {connection.name} <span className="badge">{PROVIDER_LABELS[connection.providerType]}</span>
      </p>
      <dl className="details-list">
        <dt>Server</dt>
        <dd className="code">
          {settings.username}@{settings.host}:{settings.port}
        </dd>
        <dt>Private key</dt>
        <dd>
          <span className="code wrap">{settings.privateKeyPath}</span>{" "}
          {!connection.privateKeyFound && (
            <span className="badge badge--error">File not found</span>
          )}
        </dd>
        <dt>Server identity</dt>
        <dd>
          {settings.approvedHostKey ? (
            <span className="code wrap">{settings.approvedHostKey.fingerprint}</span>
          ) : (
            <span className="message--muted">Not approved yet</span>
          )}
          {settings.hostKeyVerification === "knownHosts" && (
            <span className="message--muted"> (OpenSSH known_hosts first)</span>
          )}
        </dd>
      </dl>

      <div className="actions">
        <button
          type="button"
          className="button button--secondary button--small"
          disabled={busy !== null}
          onClick={() => void test(null)}
        >
          {busy === "test" ? "Testing…" : "Test Connection"}
        </button>
        <button
          type="button"
          className="button button--secondary button--small"
          aria-expanded={browsing}
          disabled={busy !== null}
          onClick={() => {
            setBrowsing(!browsing);
          }}
        >
          Browse…
        </button>
        <button
          type="button"
          className="button button--secondary button--small"
          disabled={busy !== null}
          onClick={() => {
            setEditing(true);
          }}
        >
          Edit…
        </button>
        <button
          type="button"
          className="button button--secondary button--small"
          aria-expanded={confirmingRemoval}
          disabled={busy !== null}
          onClick={() => {
            setConfirmingRemoval(!confirmingRemoval);
          }}
        >
          Remove…
        </button>
      </div>

      {busy === "test" && (
        <p className="message message--muted" role="status">
          Connecting to {settings.host}…
        </p>
      )}

      {passphrasePrompt && (
        <form
          className="inline-form"
          aria-label="Private key passphrase"
          onSubmit={(event) => {
            event.preventDefault();
            void test(passphrase);
          }}
        >
          <p className={passphrasePrompt.incorrect ? "message message--error" : "message"}>
            {passphrasePrompt.incorrect
              ? "The passphrase is not correct. Try again."
              : "The private key is protected by a passphrase."}{" "}
            It is used for this connection only and never saved.
          </p>
          <label className="field">
            <span className="field__label">Passphrase</span>
            <input
              className="input"
              type="password"
              autoComplete="off"
              value={passphrase}
              onChange={(event) => {
                setPassphrase(event.target.value);
              }}
            />
          </label>
          <div className="actions">
            <button type="submit" className="button" disabled={busy !== null || passphrase === ""}>
              Connect
            </button>
            <button
              type="button"
              className="button button--secondary"
              onClick={() => {
                setPassphrase("");
                setPassphrasePrompt(null);
              }}
            >
              Cancel
            </button>
          </div>
        </form>
      )}

      {outcome?.kind === "success" && (
        <p className="message message--success" role="status">
          Connection successful: the server identity was verified, the key was accepted and SFTP is
          available
          {outcome.info.homeDirectory ? ` (start directory ${outcome.info.homeDirectory})` : ""}.
        </p>
      )}
      {outcome?.kind === "error" && (
        <p className="message message--error" role="alert">
          {outcome.message}
        </p>
      )}
      {outcome?.kind === "remote" && outcome.error.kind === "hostKeyUnknown" && (
        <div className="confirm-box" role="group" aria-label="Approve server identity">
          <p className="message message--warning">
            The identity of {outcome.error.host}:{outcome.error.port} has not been approved yet.
            Compare this fingerprint with the one of your server (for example with ssh-keygen -lf on
            the server&apos;s host key) before approving it.
          </p>
          <dl className="details-list">
            <dt>Key type</dt>
            <dd className="code">{outcome.error.algorithm}</dd>
            <dt>Fingerprint</dt>
            <dd className="code wrap">{outcome.error.fingerprint}</dd>
          </dl>
          <div className="actions">
            <button
              type="button"
              className="button"
              disabled={busy !== null}
              onClick={() => {
                if (outcome.error.kind === "hostKeyUnknown") {
                  void approve(outcome.error.fingerprint);
                }
              }}
            >
              Approve and connect
            </button>
            <button
              type="button"
              className="button button--secondary"
              disabled={busy !== null}
              onClick={() => {
                setOutcome(null);
              }}
            >
              Cancel
            </button>
          </div>
        </div>
      )}
      {outcome?.kind === "remote" && outcome.error.kind === "hostKeyChanged" && (
        <div className="confirm-box" role="alert">
          <p className="message message--error">{describeRemoteError(outcome.error)}</p>
          <dl className="details-list">
            <dt>Presented</dt>
            <dd className="code wrap">{outcome.error.fingerprint}</dd>
            <dt>Expected</dt>
            <dd className="code wrap">{outcome.error.expectedFingerprint}</dd>
          </dl>
          {outcome.error.source === "knownHosts" ? (
            <p className="message message--muted">
              The expected identity comes from your OpenSSH known_hosts file, which YMDB never
              modifies. If the change is expected, update known_hosts and test again.
            </p>
          ) : confirmingForget ? (
            <>
              <p className="message message--warning">
                Forget the approved identity only if you know why it changed (for example the server
                was reinstalled). The next test shows the new identity for approval.
              </p>
              <div className="actions">
                <button
                  type="button"
                  className="button"
                  disabled={busy !== null}
                  onClick={() => void forget()}
                >
                  Forget approved identity
                </button>
                <button
                  type="button"
                  className="button button--secondary"
                  onClick={() => {
                    setConfirmingForget(false);
                  }}
                >
                  Cancel
                </button>
              </div>
            </>
          ) : (
            <button
              type="button"
              className="button button--secondary button--small"
              onClick={() => {
                setConfirmingForget(true);
              }}
            >
              Forget approved identity…
            </button>
          )}
        </div>
      )}
      {outcome?.kind === "remote" &&
        outcome.error.kind !== "hostKeyUnknown" &&
        outcome.error.kind !== "hostKeyChanged" && (
          <p className="message message--error" role="alert">
            {describeRemoteError(outcome.error)}
          </p>
        )}

      {confirmingRemoval && (
        <div className="confirm-box" role="group" aria-label="Remove connection">
          <p className="message message--warning">
            Remove the connection {connection.name}? Connections used by the ROM Manager of a MAME
            installation cannot be removed. The private key file and the remote files are not
            touched.
          </p>
          <div className="actions">
            <button
              type="button"
              className="button"
              disabled={busy !== null}
              onClick={() => void remove()}
            >
              {busy === "remove" ? "Removing…" : "Remove connection"}
            </button>
            <button
              type="button"
              className="button button--secondary"
              onClick={() => {
                setConfirmingRemoval(false);
              }}
            >
              Cancel
            </button>
          </div>
        </div>
      )}

      {browsing && <RemoteBrowser connectionId={connection.id} />}
    </li>
  );
}
