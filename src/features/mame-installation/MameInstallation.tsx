import { useEffect, useState } from "react";
import {
  clearMameConfiguration,
  detectMameInstallations,
  getMameConfiguration,
  MameCommandError,
  pickMameExecutable,
  setMameExecutable,
  verifyMameConfiguration,
  type DetectedInstallation,
  type MameConfiguration,
  type ValidationReport,
} from "./mameApi";
import {
  describeCommandError,
  describeSource,
  describeStatus,
  formatTimestamp,
  formatVersion,
} from "./messages";

type PendingAction = "load" | "select" | "detect" | "verify" | "remove";

const UNCONFIGURED: MameConfiguration = { executablePath: null, executableExists: false };

function describeError(error: unknown): string {
  return error instanceof MameCommandError ? describeCommandError(error.detail) : String(error);
}

export function MameInstallation() {
  const [configuration, setConfiguration] = useState<MameConfiguration>(UNCONFIGURED);
  const [report, setReport] = useState<ValidationReport | null>(null);
  const [detected, setDetected] = useState<DetectedInstallation[] | null>(null);
  const [pending, setPending] = useState<PendingAction | null>("load");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    getMameConfiguration()
      .then(
        (loaded) => {
          if (active) setConfiguration(loaded);
        },
        (loadError: unknown) => {
          if (active) setError(describeError(loadError));
        },
      )
      .finally(() => {
        if (active) setPending(null);
      });
    return () => {
      active = false;
    };
  }, []);

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

  async function applyExecutable(path: string) {
    const result = await setMameExecutable(path);
    setReport(result);
    if (result.status.kind === "valid") {
      setConfiguration({ executablePath: result.executablePath, executableExists: true });
    }
  }

  const selectExecutable = () =>
    run("select", async () => {
      const path = await pickMameExecutable();
      if (path !== null) {
        await applyExecutable(path);
      }
    });

  const applyDetected = (path: string) => run("select", () => applyExecutable(path));

  const detect = () =>
    run("detect", async () => {
      setDetected(await detectMameInstallations());
    });

  const verify = () =>
    run("verify", async () => {
      setReport(await verifyMameConfiguration());
    });

  const remove = () =>
    run("remove", async () => {
      await clearMameConfiguration();
      setConfiguration(UNCONFIGURED);
      setReport(null);
    });

  const configuredPath = configuration.executablePath;
  const busy = pending !== null;
  const configuredReport = report?.executablePath === configuredPath ? report : null;
  const rejectedReport = report !== null && report !== configuredReport ? report : null;

  return (
    <section className="panel" aria-labelledby="mame-installation-title" aria-busy={busy}>
      <h2 id="mame-installation-title" className="panel__title">
        MAME Installation
      </h2>

      {pending === "load" ? (
        <p className="message message--muted">Loading configuration…</p>
      ) : (
        <dl className="details-list" aria-label="MAME configuration">
          <dt>Executable</dt>
          <dd className="code">{configuredPath ?? "Not configured"}</dd>
          <dt>Version</dt>
          <dd>{configuredReport?.version ? formatVersion(configuredReport.version) : "Unknown"}</dd>
          <dt>Status</dt>
          <dd>
            <ConfigurationStatus configuration={configuration} report={configuredReport} />
          </dd>
          {configuredReport && (
            <>
              <dt>Last checked</dt>
              <dd>{formatTimestamp(configuredReport.checkedAt)}</dd>
            </>
          )}
        </dl>
      )}

      {configuredReport?.output && <RawOutput report={configuredReport} />}

      {rejectedReport && (
        <div role="alert">
          <p className="message message--error">
            The selected file was not saved: {describeStatus(rejectedReport.status)}
          </p>
          <p className="message code">{rejectedReport.executablePath}</p>
          {rejectedReport.output && <RawOutput report={rejectedReport} />}
        </div>
      )}

      {error && (
        <p className="message message--error" role="alert">
          {error}
        </p>
      )}

      <div className="actions">
        <button
          type="button"
          className="button"
          onClick={() => void selectExecutable()}
          disabled={busy}
        >
          {pending === "select" ? "Validating…" : "Select executable…"}
        </button>
        <button
          type="button"
          className="button"
          onClick={() => void verify()}
          disabled={busy || configuredPath === null}
        >
          {pending === "verify" ? "Verifying…" : "Verify"}
        </button>
        <button
          type="button"
          className="button button--secondary"
          onClick={() => void detect()}
          disabled={busy}
        >
          {pending === "detect" ? "Detecting…" : "Detect installations"}
        </button>
        <button
          type="button"
          className="button button--secondary"
          onClick={() => void remove()}
          disabled={busy || configuredPath === null}
        >
          {pending === "remove" ? "Removing…" : "Remove configuration"}
        </button>
      </div>

      {detected && (
        <DetectedInstallations
          installations={detected}
          configuredPath={configuredPath}
          disabled={busy}
          onUse={(path) => void applyDetected(path)}
        />
      )}
    </section>
  );
}

function ConfigurationStatus({
  configuration,
  report,
}: {
  configuration: MameConfiguration;
  report: ValidationReport | null;
}) {
  if (configuration.executablePath === null) {
    return <span className="message--muted">No MAME executable configured.</span>;
  }
  if (report) {
    const valid = report.status.kind === "valid";
    return (
      <span className={valid ? "message--success" : "message--error"}>
        {describeStatus(report.status)}
      </span>
    );
  }
  if (!configuration.executableExists) {
    return (
      <span className="message--warning">
        The configured executable no longer exists. Select another executable or remove the
        configuration.
      </span>
    );
  }
  return <span className="message--muted">Not verified yet.</span>;
}

function RawOutput({ report }: { report: ValidationReport }) {
  if (!report.output) {
    return null;
  }
  const { exitCode, stdout, stderr } = report.output;
  return (
    <details className="raw-output">
      <summary>Version command output</summary>
      <pre className="code">
        {`Exit code: ${exitCode === null ? "none" : String(exitCode)}\n`}
        {`stdout:\n${stdout || "(empty)"}\n`}
        {`stderr:\n${stderr || "(empty)"}`}
      </pre>
    </details>
  );
}

function DetectedInstallations({
  installations,
  configuredPath,
  disabled,
  onUse,
}: {
  installations: DetectedInstallation[];
  configuredPath: string | null;
  disabled: boolean;
  onUse: (path: string) => void;
}) {
  return (
    <>
      <h3 className="panel__subtitle">Detected installations</h3>
      {installations.length === 0 ? (
        <p className="message message--muted">
          No MAME executable was found in PATH or in standard locations. Use “Select executable…” to
          choose one manually.
        </p>
      ) : (
        <ul className="installation-list" aria-label="Detected installations">
          {installations.map(({ source, report }) => {
            const valid = report.status.kind === "valid";
            const inUse = report.executablePath === configuredPath;
            return (
              <li key={report.executablePath} className="installation-list__item">
                <div className="installation-list__info">
                  <p className="installation-list__path code">{report.executablePath}</p>
                  <p className="installation-list__meta">
                    {describeSource(source)} ·{" "}
                    {report.version ? formatVersion(report.version) : describeStatus(report.status)}
                  </p>
                </div>
                {valid &&
                  (inUse ? (
                    <span className="message--muted">In use</span>
                  ) : (
                    <button
                      type="button"
                      className="button button--secondary"
                      onClick={() => {
                        onUse(report.executablePath);
                      }}
                      disabled={disabled}
                      aria-label={`Use ${report.executablePath}`}
                    >
                      Use this installation
                    </button>
                  ))}
              </li>
            );
          })}
        </ul>
      )}
    </>
  );
}
