import { useCallback, useEffect, useState } from "react";
import { describeCommandError, describeError } from "../../shared/ipc";
import { useInstallationScope } from "../installations/installationsContext";
import { sameTarget, type LaunchRequest, type LaunchTarget } from "../launcher/launcherApi";
import { useLauncher } from "../launcher/launcherContext";
import { approveRemoteHostKey, type RemoteError } from "../remote/remoteApi";
import { CATEGORY_LABELS, describeRemoteError } from "../remote/remoteMessages";
import {
  AVAILABILITY_LABELS,
  JOB_MESSAGES,
  REASON_LABELS,
  TRANSFER_LABELS,
  describeTransferError,
  shortSize,
} from "./romMessages";
import {
  cancelDownload,
  getDownloadStatus,
  launchRequestOf,
  planMatches,
  prepareDownloadPlan,
  startDownload,
  type ConnectionIssue,
  type DownloadJobStatus,
  type DownloadPlan,
  type Passphrases,
  type PlanArtifact,
  type TransferState,
} from "./romsApi";

type Mode = "download" | "play";

interface RomDownloadSectionProps {
  /** The complete launch request; `null` while the user must still choose. */
  request: LaunchRequest | null;
  target: LaunchTarget;
  /** Interval between status queries while a download runs. */
  pollIntervalMs?: number;
}

/**
 * Prepare Download and Download & Play for one launch. The plan and the
 * downloads belong to the backend: leaving the page does not stop them, and
 * coming back shows them again.
 */
export function RomDownloadSection({
  request,
  target,
  pollIntervalMs = 500,
}: RomDownloadSectionProps) {
  const installationId = useInstallationScope();
  const { refreshSessions, start } = useLauncher();
  const [mode, setMode] = useState<Mode>("download");
  const [plan, setPlan] = useState<DownloadPlan | null>(null);
  const [job, setJob] = useState<DownloadJobStatus | null>(null);
  const [passphrases, setPassphrases] = useState<Passphrases>({});
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [dismissedPlanId, setDismissedPlanId] = useState<string | null>(null);

  // The last download, possibly started before this page was opened.
  useEffect(() => {
    let active = true;
    getDownloadStatus().then(
      (status) => {
        if (active) setJob(status);
      },
      () => {
        // Shown again on the next action.
      },
    );
    return () => {
      active = false;
    };
  }, []);

  const running = job?.state === "running";
  useEffect(() => {
    if (!running) return;
    let active = true;
    const timer = window.setInterval(() => {
      getDownloadStatus().then(
        (status) => {
          if (!active) return;
          setJob(status);
          if (status?.launch?.status === "started") refreshSessions();
        },
        () => {
          // Retried on the next tick.
        },
      );
    }, pollIntervalMs);
    return () => {
      active = false;
      window.clearInterval(timer);
    };
  }, [running, pollIntervalMs, refreshSessions]);

  // A plan of another installation or launch is never shown.
  const visiblePlan = plan && request && planMatches(plan, request) ? plan : null;
  const ownJob =
    job?.installationId === installationId &&
    sameTarget(job.target, target) &&
    job.planId !== dismissedPlanId &&
    (!visiblePlan || job.planId === visiblePlan.id)
      ? job
      : null;
  const otherRunning = running && !ownJob ? job : null;

  const prepare = useCallback(
    async (nextMode: Mode, nextPassphrases: Passphrases = passphrases) => {
      if (!request) return;
      setMode(nextMode);
      setBusy(true);
      setError(null);
      try {
        setPlan(await prepareDownloadPlan(request, nextPassphrases));
        setDismissedPlanId(null);
      } catch (prepareError) {
        setError(describeError(prepareError));
      } finally {
        setBusy(false);
      }
    },
    [request, passphrases],
  );

  async function download(planId: string, play: boolean) {
    setBusy(true);
    setError(null);
    try {
      setJob(await startDownload(planId, passphrases, play));
    } catch (startError) {
      setError(describeError(startError));
    } finally {
      setBusy(false);
    }
  }

  async function playNow(current: DownloadPlan) {
    setBusy(true);
    setError(null);
    try {
      await start(launchRequestOf(current));
    } catch (startError) {
      setError(describeError(startError));
    } finally {
      setBusy(false);
    }
  }

  async function cancel() {
    try {
      setJob(await cancelDownload());
    } catch (cancelError) {
      setError(describeError(cancelError));
    }
  }

  async function approve(issue: ConnectionIssue, fingerprint: string) {
    setBusy(true);
    setError(null);
    try {
      await approveRemoteHostKey(issue.connectionId, fingerprint);
    } catch (approveError) {
      setError(describeError(approveError));
      setBusy(false);
      return;
    }
    await prepare(mode);
  }

  function close() {
    setPlan(null);
    setPassphrases({});
    setError(null);
    if (job && job.state !== "running") setDismissedPlanId(job.planId);
  }

  return (
    <div className="rom-download" aria-label="ROM download">
      <div className="launch-panel__row">
        <button
          type="button"
          className="button button--secondary"
          disabled={!request || busy || running}
          onClick={() => void prepare("download")}
        >
          Prepare Download
        </button>
        <button
          type="button"
          className="button button--secondary"
          disabled={!request || busy || running}
          onClick={() => void prepare("play")}
        >
          Download &amp; Play
        </button>
        {busy && !visiblePlan && (
          <span className="message message--muted" role="status">
            Looking for the files…
          </span>
        )}
      </div>

      {error && (
        <p className="message message--error" role="alert">
          {error}
        </p>
      )}
      {otherRunning && (
        <p className="message message--muted" role="status">
          Another download is running: {otherRunning.title} ({otherRunning.installationLabel}).
        </p>
      )}

      {ownJob && (
        <JobProgress
          job={ownJob}
          plan={visiblePlan}
          busy={busy}
          onCancel={cancel}
          onRetry={download}
          onClose={close}
        />
      )}

      {visiblePlan && ownJob?.state !== "running" && (
        <PlanView
          plan={visiblePlan}
          mode={mode}
          busy={busy}
          job={ownJob}
          passphrases={passphrases}
          onPassphrase={(connectionId, value) => {
            setPassphrases({ ...passphrases, [connectionId]: value });
          }}
          onCheckAgain={() => void prepare(mode)}
          onApprove={(issue, fingerprint) => void approve(issue, fingerprint)}
          onDownload={() => void download(visiblePlan.id, mode === "play")}
          onPlay={() => void playNow(visiblePlan)}
          onClose={close}
        />
      )}
    </div>
  );
}

interface PlanViewProps {
  plan: DownloadPlan;
  mode: Mode;
  busy: boolean;
  job: DownloadJobStatus | null;
  passphrases: Passphrases;
  onPassphrase: (connectionId: string, value: string) => void;
  onCheckAgain: () => void;
  onApprove: (issue: ConnectionIssue, fingerprint: string) => void;
  onDownload: () => void;
  onPlay: () => void;
  onClose: () => void;
}

function PlanView({
  plan,
  mode,
  busy,
  job,
  passphrases,
  onPassphrase,
  onCheckAgain,
  onApprove,
  onDownload,
  onPlay,
  onClose,
}: PlanViewProps) {
  const { summary } = plan;
  const nothingToDownload = summary.toDownload === 0;
  const finished = job !== null && job.state !== "running";
  return (
    <section className="rom-plan" aria-label="Download plan">
      <h3 className="rom-plan__title">Download plan</h3>
      <p className="rom-plan__summary" role="status">
        <span>{summary.present} present</span>
        <span>
          {summary.toDownload} to download
          {summary.toDownload > 0 &&
            ` (${shortSize(summary.downloadBytes)}${summary.unknownSizes > 0 ? " + unknown sizes" : ""})`}
        </span>
        <span>{summary.notFound} not found</span>
        {summary.unresolved > 0 && <span>{summary.unresolved} unresolved</span>}
        <span>
          {summary.warnings} warning{summary.warnings === 1 ? "" : "s"}
        </span>
      </p>

      {plan.connectionIssues.map((issue) => (
        <ConnectionIssueView
          key={issue.connectionId}
          issue={issue}
          busy={busy}
          passphrase={passphrases[issue.connectionId] ?? ""}
          onPassphrase={(value) => {
            onPassphrase(issue.connectionId, value);
          }}
          onCheckAgain={onCheckAgain}
          onApprove={(fingerprint) => {
            onApprove(issue, fingerprint);
          }}
        />
      ))}
      {plan.warnings.map((warning, index) => (
        <p key={index} className="message message--warning">
          {warning.message}
        </p>
      ))}

      {plan.artifacts.length === 0 ? (
        <p className="message message--muted">
          The catalog lists no ROM, CHD or sample file for this launch.
        </p>
      ) : (
        <ArtifactTable
          artifacts={plan.artifacts}
          transfers={job?.planId === plan.id ? job.transfers : []}
        />
      )}

      <p className="message message--muted">
        Files are whole sets chosen from the catalog (parents, BIOS, devices, disks, samples) and
        may be more than strictly needed. A present or downloaded file is not verified: MAME decides
        whether the set can run.
      </p>
      {nothingToDownload && !finished && (
        <p className="message message--muted">Nothing to download.</p>
      )}

      {!finished && (
        <div className="actions">
          {mode === "play" ? (
            <button
              type="button"
              className="button"
              disabled={busy}
              onClick={nothingToDownload ? onPlay : onDownload}
            >
              {nothingToDownload ? "Play" : `Download ${String(summary.toDownload)} & Play`}
            </button>
          ) : (
            <button
              type="button"
              className="button"
              disabled={busy || nothingToDownload}
              onClick={onDownload}
            >
              Download {summary.toDownload} file{summary.toDownload === 1 ? "" : "s"}
            </button>
          )}
          <button
            type="button"
            className="button button--secondary"
            onClick={onCheckAgain}
            disabled={busy}
          >
            Check again
          </button>
          <button type="button" className="button button--secondary" onClick={onClose}>
            Close
          </button>
        </div>
      )}
    </section>
  );
}

interface ConnectionIssueViewProps {
  issue: ConnectionIssue;
  busy: boolean;
  passphrase: string;
  onPassphrase: (value: string) => void;
  onCheckAgain: () => void;
  onApprove: (fingerprint: string) => void;
}

function ConnectionIssueView({
  issue,
  busy,
  passphrase,
  onPassphrase,
  onCheckAgain,
  onApprove,
}: ConnectionIssueViewProps) {
  const { error } = issue;
  const needsPassphrase =
    error.kind === "passphraseRequired" || error.kind === "passphraseIncorrect";
  return (
    <div className="rom-plan__issue" role="alert">
      <p className="message message--warning">
        {issue.connectionName}: {describeRemoteError(error)} Its files could not be looked up.
      </p>
      {needsPassphrase && (
        <div className="launch-panel__row">
          <label className="launch-panel__field">
            <span>Passphrase for {issue.connectionName}</span>
            <input
              className="input"
              type="password"
              autoComplete="off"
              value={passphrase}
              onChange={(event) => {
                onPassphrase(event.target.value);
              }}
            />
          </label>
          <button
            type="button"
            className="button button--secondary"
            disabled={busy || passphrase === ""}
            onClick={onCheckAgain}
          >
            Unlock and check again
          </button>
        </div>
      )}
      {error.kind === "hostKeyUnknown" && (
        <HostKeyApproval error={error} busy={busy} onApprove={onApprove} />
      )}
      {error.kind === "hostKeyChanged" && (
        <p className="message message--muted">
          Expected {error.expectedFingerprint}, received {error.fingerprint}. Check the server, then
          fix the connection in Settings → Remote Connections.
        </p>
      )}
    </div>
  );
}

function HostKeyApproval({
  error,
  busy,
  onApprove,
}: {
  error: Extract<RemoteError, { kind: "hostKeyUnknown" }>;
  busy: boolean;
  onApprove: (fingerprint: string) => void;
}) {
  return (
    <div className="launch-panel__row">
      <p className="message message--muted">
        {error.algorithm} key fingerprint <span className="code">{error.fingerprint}</span>. Approve
        it only if it is the fingerprint of your server.
      </p>
      <button
        type="button"
        className="button button--secondary"
        disabled={busy}
        onClick={() => {
          onApprove(error.fingerprint);
        }}
      >
        Approve and check again
      </button>
    </div>
  );
}

function ArtifactTable({
  artifacts,
  transfers,
}: {
  artifacts: PlanArtifact[];
  transfers: TransferState[];
}) {
  const showTransfers = transfers.length > 0;
  return (
    <div className="rom-plan__table-wrapper">
      <table className="rom-plan__table">
        <thead>
          <tr>
            <th scope="col">File</th>
            <th scope="col">Category</th>
            <th scope="col">Reason</th>
            <th scope="col">Status</th>
            <th scope="col">Size</th>
            <th scope="col">Location</th>
            {showTransfers && <th scope="col">Transfer</th>}
          </tr>
        </thead>
        <tbody>
          {artifacts.map((artifact) => {
            const transfer = transfers.find((entry) => entry.artifactId === artifact.id);
            return (
              <tr key={artifact.id} data-availability={artifact.availability}>
                <td>
                  <span className="code">{artifact.relativePath}</span>
                  {artifact.warning && (
                    <span className={`rom-plan__note rom-plan__note--${artifact.warning.kind}`}>
                      {artifact.warning.message}
                    </span>
                  )}
                </td>
                <td>{CATEGORY_LABELS[artifact.category]}</td>
                <td>{REASON_LABELS[artifact.reason]}</td>
                <td>
                  <span className={`badge ${availabilityBadge(artifact)}`}>
                    {artifact.availability === "remoteAvailable" && !artifact.download
                      ? "Not downloaded"
                      : AVAILABILITY_LABELS[artifact.availability]}
                  </span>
                </td>
                <td>{shortSize(artifact.local?.size ?? artifact.remoteSize)}</td>
                <td className="rom-plan__path">
                  {artifact.local?.path ?? artifact.destination ?? "—"}
                </td>
                {showTransfers && (
                  <td>
                    {transfer ? (
                      <>
                        {TRANSFER_LABELS[transfer.status]}
                        {transfer.error && (
                          <span className="rom-plan__note rom-plan__note--error">
                            {describeTransferError(transfer.error)}
                          </span>
                        )}
                      </>
                    ) : (
                      "—"
                    )}
                  </td>
                )}
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}

function availabilityBadge(artifact: PlanArtifact): string {
  switch (artifact.availability) {
    case "present":
      return "badge--success";
    case "remoteAvailable":
      return artifact.download ? "" : "badge--warning";
    case "remoteMissing":
      return artifact.warning?.kind === "informational" ? "" : "badge--warning";
    case "missing":
    case "unresolved":
      return "badge--warning";
  }
}

interface JobProgressProps {
  job: DownloadJobStatus;
  plan: DownloadPlan | null;
  busy: boolean;
  onCancel: () => Promise<void>;
  onRetry: (planId: string, play: boolean) => Promise<void>;
  onClose: () => void;
}

function JobProgress({ job, plan, busy, onCancel, onRetry, onClose }: JobProgressProps) {
  const done = job.transfers.filter(
    (transfer) => transfer.status === "downloaded" || transfer.status === "skipped",
  ).length;
  const percent =
    job.bytesTotal !== null && job.bytesTotal > 0
      ? Math.min(100, Math.round((job.bytesTransferred / job.bytesTotal) * 100))
      : null;
  const current = job.transfers.find((transfer) => transfer.status === "downloading");
  const fileName = (id: string) =>
    plan?.artifacts.find((artifact) => artifact.id === id)?.relativePath ??
    id.replace(/^[^:]+:/, "");

  if (job.state === "running") {
    return (
      <section className="rom-job" aria-label="Download progress">
        <p className="rom-job__line" role="status">
          {job.cancelRequested ? "Cancelling…" : `Downloading ${job.play ? "before playing " : ""}`}
          {!job.cancelRequested && (
            <>
              file {Math.min(done + 1, job.transfers.length)} of {job.transfers.length}
              {current && (
                <>
                  : <span className="code">{fileName(current.artifactId)}</span> (
                  {shortSize(current.bytesTransferred)} of {shortSize(current.size)})
                </>
              )}
            </>
          )}
        </p>
        <progress
          className="rom-job__bar"
          aria-label="Overall progress"
          max={100}
          value={percent ?? undefined}
        />
        <p className="message message--muted">
          {shortSize(job.bytesTransferred)}
          {job.bytesTotal !== null && ` of ${shortSize(job.bytesTotal)}`}
          {percent !== null && ` (${String(percent)}%)`}
        </p>
        <div className="actions">
          <button
            type="button"
            className="button button--secondary"
            disabled={job.cancelRequested}
            onClick={() => void onCancel()}
          >
            Cancel download
          </button>
        </div>
      </section>
    );
  }

  const failures = job.transfers.filter((transfer) => transfer.status === "failed");
  const retryable = job.state === "failed" || job.state === "cancelled";
  return (
    <section className="rom-job" aria-label="Download result">
      <p
        className={`message ${job.state === "completed" ? "message--success" : job.state === "completedWithWarnings" ? "message--warning" : "message--error"}`}
        role="status"
      >
        {JOB_MESSAGES[job.state]}
      </p>
      {failures.length > 0 && (
        <ul className="rom-job__failures">
          {failures.map((transfer) => (
            <li key={transfer.artifactId}>
              <span className="code">{fileName(transfer.artifactId)}</span>:{" "}
              {transfer.error ? describeTransferError(transfer.error) : "failed"}
            </li>
          ))}
        </ul>
      )}
      {job.launch?.status === "started" && (
        <p className="message message--muted">MAME was started with the downloaded files.</p>
      )}
      {job.launch?.status === "notStarted" && (
        <p className="message message--muted">
          MAME was not started because the downloads did not complete. Retry, or use Play to start
          it with the files already present.
        </p>
      )}
      {job.launch?.status === "failed" && (
        <p className="message message--error" role="alert">
          The downloads completed but MAME could not be started:{" "}
          {describeCommandError(job.launch.error)}
        </p>
      )}
      <div className="actions">
        {retryable && (
          <button
            type="button"
            className="button"
            disabled={busy}
            onClick={() => void onRetry(job.planId, job.play)}
          >
            Retry
          </button>
        )}
        <button type="button" className="button button--secondary" onClick={onClose}>
          Close
        </button>
      </div>
    </section>
  );
}
