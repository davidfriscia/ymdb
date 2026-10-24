import { useCallback, useEffect, useState } from "react";
import { describeError } from "../../shared/ipc";
import { useLoader } from "../../shared/useLoader";
import { exportStoredPreferences } from "../../app/preferences";
import { formatCount } from "../catalog/catalogMessages";
import {
  applyBackupImport,
  cancelBackupJob,
  discardBackupJob,
  getApplicationData,
  getBackupStatus,
  openApplicationDataLocation,
  startBackupExport,
  startBackupImportCheck,
  type BackupJobStatus,
  type BackupPhase,
  type CategorySummary,
  type DataClass,
  type DataEntry,
  type ExternalKind,
  type ExternalReference,
  type RestoreOutcome,
  type RestorePreview,
} from "./appDataApi";

const POLL_INTERVAL_MS = 400;

interface ApplicationDataSettingsProps {
  /** Result of a restore applied when YMDB started, shown once. */
  restoreOutcome: RestoreOutcome | null;
}

/**
 * Settings → Application Data: where YMDB keeps its files, and a full backup
 * (export) and restore (import) of the data it owns. Paths come from the
 * backend; nothing here deletes application files.
 */
export function ApplicationDataSettings({ restoreOutcome }: ApplicationDataSettingsProps) {
  const [refreshKey, setRefreshKey] = useState(0);
  const load = useCallback(() => getApplicationData(), []);
  const inventory = useLoader(load, refreshKey);
  const [job, setJob] = useState<BackupJobStatus>({ state: "idle" });
  const [actionError, setActionError] = useState<string | null>(null);
  const [copied, setCopied] = useState<string | null>(null);
  const [restarting, setRestarting] = useState(false);

  const refreshStatus = useCallback(async () => {
    try {
      setJob(await getBackupStatus());
    } catch (error) {
      setActionError(describeError(error));
    }
  }, []);

  // A job may still run (or wait for confirmation) from an earlier visit.
  useEffect(() => {
    let active = true;
    getBackupStatus().then(
      (status) => {
        if (active) setJob(status);
      },
      (error: unknown) => {
        if (active) setActionError(describeError(error));
      },
    );
    return () => {
      active = false;
    };
  }, []);

  const running = job.state === "running";
  useEffect(() => {
    if (!running) return;
    const polling = { active: true, timer: 0 };
    // Read through a function: the flag changes asynchronously in the cleanup.
    const isActive = () => polling.active;

    async function poll() {
      try {
        const next = await getBackupStatus();
        if (!isActive()) return;
        setJob(next);
        if (next.state === "running") {
          polling.timer = window.setTimeout(() => void poll(), POLL_INTERVAL_MS);
        } else if (next.state === "exported") {
          // The sizes shown may have changed.
          setRefreshKey((key) => key + 1);
        }
      } catch (error) {
        if (isActive()) setActionError(describeError(error));
      }
    }

    polling.timer = window.setTimeout(() => void poll(), POLL_INTERVAL_MS);
    return () => {
      polling.active = false;
      window.clearTimeout(polling.timer);
    };
  }, [running]);

  async function act(work: () => Promise<unknown>) {
    setActionError(null);
    try {
      await work();
      await refreshStatus();
    } catch (error) {
      setActionError(describeError(error));
    }
  }

  async function copyPath(path: string) {
    try {
      await navigator.clipboard.writeText(path);
      setCopied(path);
    } catch {
      setActionError("The path could not be copied. Select it and copy it manually.");
    }
  }

  function confirmRestore(jobId: number) {
    setActionError(null);
    setRestarting(true);
    // On success YMDB restarts and this call never resolves.
    applyBackupImport(jobId).catch((error: unknown) => {
      setRestarting(false);
      setActionError(describeError(error));
    });
  }

  const busy = running || restarting;
  const view = inventory.latest;

  return (
    <section className="panel app-data" aria-label="Application Data">
      <p className="message message--muted">
        The files and directories YMDB uses on this computer. A backup contains every file YMDB
        owns; it never contains ROMs, CHDs, MAME installations, media files or private keys, which
        are only referenced by their paths.
      </p>

      {restoreOutcome && <OutcomeMessage outcome={restoreOutcome} />}

      <h3 className="panel__subtitle">Backup</h3>
      <div className="actions">
        <button
          type="button"
          className="button"
          disabled={busy || job.state === "prepared"}
          onClick={() => {
            void act(() => startBackupExport(exportStoredPreferences()));
          }}
        >
          Export backup…
        </button>
        <button
          type="button"
          className="button button--secondary"
          disabled={busy || job.state === "prepared" || view?.restorePending === true}
          onClick={() => {
            void act(() => startBackupImportCheck());
          }}
        >
          Import backup…
        </button>
      </div>
      <p className="message message--muted">
        Export and import are refused while MAME, a catalog import or a ROM download is running.
        Importing replaces all current application data after a confirmation, then restarts YMDB.
      </p>

      <JobPanel
        job={job}
        restarting={restarting}
        onCancel={() => {
          void act(() => cancelBackupJob());
        }}
        onDismiss={() => {
          void act(() => discardBackupJob());
        }}
        onConfirm={confirmRestore}
      />
      {actionError !== null && (
        <p className="message message--error" role="alert">
          {actionError}
        </p>
      )}

      <h3 className="panel__subtitle">Locations</h3>
      {inventory.state.status === "error" && (
        <div className="state-message" role="alert">
          <p className="message message--error">
            The application data could not be read: {describeError(inventory.state.error)}
          </p>
          <button type="button" className="button button--secondary" onClick={inventory.reload}>
            Retry
          </button>
        </div>
      )}
      {view === null ? (
        inventory.state.status === "loading" && (
          <p className="message" role="status">
            Measuring application data…
          </p>
        )
      ) : (
        <>
          <dl className="details-list">
            <div className="details-list__row">
              <dt>Configuration directory</dt>
              <dd className="app-data__path">{view.configDir}</dd>
            </div>
            <div className="details-list__row">
              <dt>Local data directory</dt>
              <dd className="app-data__path">{view.dataDir}</dd>
            </div>
          </dl>
          {view.customDataDir && (
            <p className="message message--muted">
              The YMDB_DATA_DIR environment variable chooses these directories.
            </p>
          )}
          {view.restorePending && (
            <p className="message message--warning" role="status">
              A restore is waiting for YMDB to restart.
            </p>
          )}
          {copied !== null && (
            <p className="message message--success" role="status">
              Copied {copied}
            </p>
          )}
          <ul className="app-data__list">
            {view.entries.map((entry) => (
              <EntryItem
                key={entry.id}
                entry={entry}
                onOpen={() => {
                  void act(() => openApplicationDataLocation(entry.id));
                }}
                onCopy={() => {
                  void copyPath(entry.path);
                }}
              />
            ))}
          </ul>

          <h3 className="panel__subtitle">External data</h3>
          <p className="message message--muted">
            Referenced by the configuration but not owned by YMDB: never included in a backup, never
            replaced by a restore.
          </p>
          <ExternalList references={view.externalReferences} />
        </>
      )}
    </section>
  );
}

const CLASS_LABELS: Record<DataClass, string> = {
  owned: "Application data",
  regenerable: "Regenerable",
  transient: "Temporary",
  legacy: "Legacy",
};

function EntryItem({
  entry,
  onOpen,
  onCopy,
}: {
  entry: DataEntry;
  onOpen: () => void;
  onCopy: () => void;
}) {
  return (
    <li className="app-data__entry">
      <div className="app-data__entry-header">
        <span className="app-data__name">{entry.name}</span>
        <span className="badge">{CLASS_LABELS[entry.class]}</span>
        <span className={entry.inBackup ? "badge badge--success" : "badge"}>
          {entry.inBackup ? "In backup" : "Not in backup"}
        </span>
      </div>
      <p className="message message--muted">{entry.description}</p>
      <p className="app-data__path">{entry.path}</p>
      <p className="message">
        {entry.kind === "directory" ? "Directory" : "File"} · {describePresence(entry)}
      </p>
      {entry.exclusionReason !== null && (
        <p className="message message--muted">{entry.exclusionReason}</p>
      )}
      <div className="actions">
        <button type="button" className="button button--secondary button--small" onClick={onOpen}>
          Open location
        </button>
        <button type="button" className="button button--secondary button--small" onClick={onCopy}>
          Copy path
        </button>
      </div>
    </li>
  );
}

function describePresence(entry: DataEntry): string {
  if (!entry.exists) return "Not created yet";
  const parts: string[] = [];
  if (entry.size !== null) parts.push(formatSize(entry.size));
  if (entry.files !== null)
    parts.push(`${formatCount(entry.files)} ${plural(entry.files, "file")}`);
  return parts.join(", ") || "Present";
}

const EXTERNAL_LABELS: Record<ExternalKind, string> = {
  mameExecutable: "MAME executable",
  mediaDirectory: "Media directory",
  romDestination: "ROM destination",
  privateKey: "Private key",
};

function ExternalList({ references }: { references: ExternalReference[] }) {
  if (references.length === 0) {
    return <p className="message">No external files are referenced.</p>;
  }
  return (
    <ul className="app-data__external">
      {references.map((reference, index) => (
        <li key={`${reference.kind}-${reference.path}-${String(index)}`}>
          <span className="app-data__name">{EXTERNAL_LABELS[reference.kind]}</span>{" "}
          <span className="message--muted">({reference.owner})</span>{" "}
          {reference.exists ? (
            <span className="badge badge--success">Available</span>
          ) : (
            <span className="badge badge--warning">Not found</span>
          )}
          <div className="app-data__path">{reference.path}</div>
        </li>
      ))}
    </ul>
  );
}

const PHASE_LABELS: Record<BackupPhase, string> = {
  snapshot: "Copying the application data…",
  writing: "Writing the archive…",
  verifying: "Verifying the archive…",
  extracting: "Extracting the backup…",
  checking: "Checking the backup…",
};

function JobPanel({
  job,
  restarting,
  onCancel,
  onDismiss,
  onConfirm,
}: {
  job: BackupJobStatus;
  restarting: boolean;
  onCancel: () => void;
  onDismiss: () => void;
  onConfirm: (jobId: number) => void;
}) {
  switch (job.state) {
    case "idle":
      return null;
    case "running":
      return (
        <div className="import-progress" role="status">
          <progress
            className="import-progress__bar"
            aria-label={job.kind === "export" ? "Export progress" : "Import progress"}
            {...(job.bytesTotal > 0 ? { max: job.bytesTotal, value: job.bytesDone } : {})}
          />
          <p className="message">
            {PHASE_LABELS[job.phase]}
            {job.bytesTotal > 0 &&
              ` ${formatSize(job.bytesDone)} of ${formatSize(job.bytesTotal)}.`}
          </p>
          <div className="actions">
            <button
              type="button"
              className="button button--secondary"
              disabled={job.cancelRequested}
              onClick={onCancel}
            >
              {job.cancelRequested ? "Cancelling…" : "Cancel"}
            </button>
          </div>
        </div>
      );
    case "exported":
      return (
        <div className="confirm-box" role="status">
          <p className="message message--success">Backup exported ({formatSize(job.size)}) to:</p>
          <p className="app-data__path">{job.path}</p>
          <div className="actions">
            <button type="button" className="button button--secondary" onClick={onDismiss}>
              Done
            </button>
          </div>
        </div>
      );
    case "failed":
    case "cancelled":
      return (
        <div className="confirm-box" role="alert">
          <p className={job.state === "failed" ? "message message--error" : "message"}>
            {job.kind === "export" ? "Export" : "Import"}{" "}
            {job.state === "failed" ? `failed: ${job.message}` : "cancelled."}
          </p>
          <p className="message message--muted">
            {job.kind === "export"
              ? "No archive was written and the application data was not changed."
              : "The current application data was not changed."}
          </p>
          <div className="actions">
            <button type="button" className="button button--secondary" onClick={onDismiss}>
              Close
            </button>
          </div>
        </div>
      );
    case "prepared":
      return (
        <RestoreConfirmation
          preview={job.preview}
          restarting={restarting}
          onCancel={onDismiss}
          onConfirm={() => {
            onConfirm(job.jobId);
          }}
        />
      );
  }
}

function RestoreConfirmation({
  preview,
  restarting,
  onCancel,
  onConfirm,
}: {
  preview: RestorePreview;
  restarting: boolean;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  const missing = preview.externalReferences.filter((reference) => !reference.exists);
  return (
    <div className="confirm-box" role="group" aria-label="Restore backup">
      <h3 className="panel__subtitle">Full restore — replace current application data</h3>
      <dl className="details-list">
        <div className="details-list__row">
          <dt>File</dt>
          <dd className="app-data__path">{preview.archivePath}</dd>
        </div>
        <div className="details-list__row">
          <dt>Created</dt>
          <dd>{new Date(preview.createdAt).toLocaleString("en-US")}</dd>
        </div>
        <div className="details-list__row">
          <dt>YMDB version</dt>
          <dd>{preview.appVersion}</dd>
        </div>
        <div className="details-list__row">
          <dt>Platform</dt>
          <dd>
            {preview.platform.os} ({preview.platform.arch})
          </dd>
        </div>
        <div className="details-list__row">
          <dt>Backup format</dt>
          <dd>Version {preview.formatVersion}</dd>
        </div>
        <div className="details-list__row">
          <dt>Total size</dt>
          <dd>{formatSize(preview.totalSize)}</dd>
        </div>
      </dl>
      <p className="message">Contents of the backup:</p>
      <SummaryList summaries={preview.categories} empty="No data." />
      <p className="message">Current data that will be replaced:</p>
      <SummaryList summaries={preview.replaced} empty="No current data." />
      {preview.notes.map((note) => (
        <p key={note} className="message message--warning">
          {note}
        </p>
      ))}
      {missing.length > 0 && (
        <>
          <p className="message message--warning">
            These external files and directories are not available on this computer. YMDB keeps the
            references; correct them after the restore if needed:
          </p>
          <ExternalList references={missing} />
        </>
      )}
      <p className="message message--muted">
        YMDB restarts to replace its data. The current data is kept until the restored data is in
        place and is put back if anything fails.
      </p>
      <div className="actions">
        <button type="button" className="button" disabled={restarting} onClick={onConfirm}>
          {restarting ? "Restarting…" : "Replace data and restart"}
        </button>
        <button
          type="button"
          className="button button--secondary"
          disabled={restarting}
          onClick={onCancel}
        >
          Cancel
        </button>
      </div>
    </div>
  );
}

function SummaryList({ summaries, empty }: { summaries: CategorySummary[]; empty: string }) {
  if (summaries.length === 0) return <p className="message message--muted">{empty}</p>;
  return (
    <ul className="app-data__summary">
      {summaries.map((summary) => (
        <li key={summary.category}>
          {summary.label}: {formatCount(summary.files)} {plural(summary.files, "file")},{" "}
          {formatSize(summary.size)}
        </li>
      ))}
    </ul>
  );
}

function OutcomeMessage({ outcome }: { outcome: RestoreOutcome }) {
  if (outcome.kind === "restored") {
    return (
      <p className="message message--success" role="status">
        Application data restored from the backup created on{" "}
        {new Date(outcome.createdAt).toLocaleString("en-US")} by YMDB {outcome.appVersion}.
      </p>
    );
  }
  return (
    <p className="message message--error" role="alert">
      The restore failed: {outcome.message}
      {outcome.previousDataKept ? "" : " Check the application data before continuing."}
    </p>
  );
}

function plural(count: number, word: string): string {
  return count === 1 ? word : `${word}s`;
}

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${formatCount(bytes)} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value.toFixed(1)} ${units[unit] ?? "TB"}`;
}
