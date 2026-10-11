import { useState } from "react";
import { describeError } from "../../shared/ipc";
import { useLauncher } from "../launcher/launcherContext";
import type { ValidationReport } from "../mame-installation/mameApi";
import { describeStatus, formatTimestamp, formatVersion } from "../mame-installation/messages";
import { RawOutput } from "../mame-installation/RawOutput";
import {
  removeInstallation,
  renameInstallation,
  verifyInstallation,
  type InstallationSummary,
  type RemovalResult,
} from "./installationsApi";
import { useInstallations } from "./installationsContext";
import { describeExecutableStatus } from "./installationMessages";

type PendingAction = "verify" | "rename" | "remove";

interface InstallationDetailsProps {
  installation: InstallationSummary;
  onRemoved: (result: RemovalResult, label: string) => void;
}

/** One installation: its executable, version, and the actions on it. */
export function InstallationDetails({ installation, onRemoved }: InstallationDetailsProps) {
  const { labels, select, reload, activeId } = useInstallations();
  const { sessions } = useLauncher();
  const [pending, setPending] = useState<PendingAction | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [report, setReport] = useState<ValidationReport | null>(null);
  const [renaming, setRenaming] = useState(false);
  const [name, setName] = useState(installation.displayName ?? "");
  const [confirmingRemoval, setConfirmingRemoval] = useState(false);
  const [deleteCatalog, setDeleteCatalog] = useState(true);
  const [deleteLaunchConfig, setDeleteLaunchConfig] = useState(false);

  const label = labels.get(installation.id) ?? installation.label;
  const active = installation.id === activeId;
  const runningSessions = sessions.filter(
    (session) => session.installationId === installation.id && session.state.state === "running",
  ).length;
  const busy = pending !== null;

  async function run(action: PendingAction, work: () => Promise<void>) {
    setPending(action);
    setError(null);
    try {
      await work();
    } catch (actionError) {
      setError(describeError(actionError));
    } finally {
      setPending(null);
    }
  }

  const verify = () =>
    run("verify", async () => {
      const result = await verifyInstallation(installation.id);
      setReport(result.report);
      await reload();
    });

  const rename = () =>
    run("rename", async () => {
      await renameInstallation(installation.id, name.trim() === "" ? null : name);
      await reload();
      setRenaming(false);
    });

  const remove = () =>
    run("remove", async () => {
      const result = await removeInstallation(installation.id, deleteCatalog, deleteLaunchConfig);
      await reload();
      onRemoved(result, label);
    });

  return (
    <section className="panel" aria-labelledby="installation-title" aria-busy={busy}>
      <h2 id="installation-title" className="panel__title">
        {label}
      </h2>
      <dl className="details-list" aria-label="Installation">
        <dt>Executable</dt>
        <dd className="code">{installation.executablePath}</dd>
        <dt>Version</dt>
        <dd>{installation.version ? formatVersion(installation.version) : "Unknown"}</dd>
        <dt>Status</dt>
        <dd>
          {report ? (
            <span
              className={report.status.kind === "valid" ? "message--success" : "message--error"}
            >
              {describeStatus(report.status)}
            </span>
          ) : (
            <span
              className={
                installation.executableStatus === "verified"
                  ? "message--success"
                  : installation.executableStatus === "notVerified"
                    ? "message--muted"
                    : "message--warning"
              }
            >
              {describeExecutableStatus(installation.executableStatus)}
            </span>
          )}
        </dd>
        <dt>Last verified</dt>
        <dd>
          {installation.verifiedAt === null ? "Never" : formatTimestamp(installation.verifiedAt)}
        </dd>
        <dt>Selection</dt>
        <dd>{active ? "Active installation" : "Not active"}</dd>
      </dl>
      {report?.output && <RawOutput report={report} />}

      {error && (
        <p className="message message--error" role="alert">
          {error}
        </p>
      )}

      <div className="actions">
        <button
          type="button"
          className="button"
          disabled={busy || active}
          onClick={() => {
            select(installation.id);
          }}
        >
          {active ? "Active" : "Set as active"}
        </button>
        <button
          type="button"
          className="button button--secondary"
          disabled={busy}
          onClick={() => void verify()}
        >
          {pending === "verify" ? "Verifying…" : "Verify"}
        </button>
        <button
          type="button"
          className="button button--secondary"
          aria-expanded={renaming}
          disabled={busy}
          onClick={() => {
            setName(installation.displayName ?? "");
            setRenaming(!renaming);
          }}
        >
          Rename…
        </button>
        <button
          type="button"
          className="button button--secondary"
          aria-expanded={confirmingRemoval}
          disabled={busy}
          onClick={() => {
            setConfirmingRemoval(!confirmingRemoval);
          }}
        >
          Remove…
        </button>
      </div>

      {renaming && (
        <form
          className="inline-form"
          onSubmit={(event) => {
            event.preventDefault();
            void rename();
          }}
        >
          <label className="field">
            <span className="field__label">Installation name</span>
            <input
              className="input"
              value={name}
              maxLength={60}
              placeholder={installation.version ? `MAME ${installation.version.release}` : ""}
              onChange={(event) => {
                setName(event.target.value);
              }}
            />
          </label>
          <p className="message message--muted">Leave empty to show the version.</p>
          <div className="actions">
            <button type="submit" className="button" disabled={busy}>
              {pending === "rename" ? "Saving…" : "Save name"}
            </button>
            <button
              type="button"
              className="button button--secondary"
              onClick={() => {
                setRenaming(false);
              }}
            >
              Cancel
            </button>
          </div>
        </form>
      )}

      {confirmingRemoval && (
        <div className="confirm-box" role="group" aria-label="Remove installation">
          <p className="message message--warning">
            Remove {label} from YMDB? The MAME executable and MAME&apos;s own files (ROMs, CHDs,
            artwork, INI files) are never deleted.
          </p>
          <label className="choice">
            <input
              type="checkbox"
              checked={deleteCatalog}
              onChange={(event) => {
                setDeleteCatalog(event.target.checked);
              }}
            />
            Also delete its imported catalog (derived from MAME: it can be imported again)
          </label>
          <label className="choice">
            <input
              type="checkbox"
              checked={deleteLaunchConfig}
              onChange={(event) => {
                setDeleteLaunchConfig(event.target.checked);
              }}
            />
            Also delete its launch options and ROM Manager settings (global options, profiles,
            overrides and remote sources)
          </label>
          {active && (
            <p className="message message--muted">
              This is the active installation: the first remaining installation becomes active.
            </p>
          )}
          {runningSessions > 0 && (
            <p className="message message--error" role="alert">
              MAME is running from this installation. Stop the session or wait for it to end before
              removing the installation.
            </p>
          )}
          <div className="actions">
            <button
              type="button"
              className="button"
              disabled={busy || runningSessions > 0}
              onClick={() => void remove()}
            >
              {pending === "remove" ? "Removing…" : "Remove installation"}
            </button>
            <button
              type="button"
              className="button button--secondary"
              disabled={busy}
              onClick={() => {
                setConfirmingRemoval(false);
              }}
            >
              Cancel
            </button>
          </div>
        </div>
      )}
    </section>
  );
}
