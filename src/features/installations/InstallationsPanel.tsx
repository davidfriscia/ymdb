import { useState } from "react";
import { CommandFailedError, describeError } from "../../shared/ipc";
import type { ValidationReport } from "../mame-installation/mameApi";
import { describeSource, describeStatus, formatVersion } from "../mame-installation/messages";
import { RawOutput } from "../mame-installation/RawOutput";
import {
  addInstallation,
  detectMameInstallations,
  pickMameExecutable,
  type DetectedCandidate,
} from "./installationsApi";
import { useInstallations } from "./installationsContext";
import { describeCatalog, describeExecutableStatus } from "./installationMessages";

interface InstallationsPanelProps {
  /** The installation shown in the panels below. */
  selectedId: string | null;
  onSelect: (id: string) => void;
}

/** The registered MAME installations, adding new ones and detection. */
export function InstallationsPanel({ selectedId, onSelect }: InstallationsPanelProps) {
  const { view, error, installations, labels, reload } = useInstallations();
  const [pending, setPending] = useState<"add" | "detect" | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [rejected, setRejected] = useState<ValidationReport | null>(null);
  const [detected, setDetected] = useState<DetectedCandidate[] | null>(null);

  async function run(action: "add" | "detect", work: () => Promise<void>) {
    setPending(action);
    setActionError(null);
    setNotice(null);
    try {
      await work();
    } catch (actionFailure) {
      const detail = actionFailure instanceof CommandFailedError ? actionFailure.detail : null;
      if (detail?.kind === "duplicateInstallation") {
        // Show the installation that already registers it.
        onSelect(detail.existingInstallationId);
        setNotice(
          `This executable is already registered as ${labels.get(detail.existingInstallationId) ?? "another installation"}.`,
        );
      } else {
        setActionError(describeError(actionFailure));
      }
    } finally {
      setPending(null);
    }
  }

  async function register(path: string) {
    const result = await addInstallation(path);
    if (result.installation === null) {
      setRejected(result.report);
      return;
    }
    const added = result.installation;
    setRejected(null);
    await reload();
    onSelect(added.id);
    setNotice(`${added.label} was added. Its catalog has not been imported yet.`);
    setDetected(
      (current) =>
        current?.map((candidate) =>
          candidate.report.executablePath === path
            ? { ...candidate, registeredInstallationId: added.id }
            : candidate,
        ) ?? null,
    );
  }

  const busy = pending !== null;
  return (
    <section className="panel" aria-labelledby="installations-title" aria-busy={busy}>
      <h2 id="installations-title" className="panel__title">
        MAME Installations
      </h2>
      <p className="message message--muted">
        Register every MAME version you use. Each installation has its own catalog and launch
        options; the one selected in the catalog is the active installation.
      </p>

      {view === null && error === null && (
        <p className="message message--muted">Loading installations…</p>
      )}
      {error !== null && (
        <div className="state-message" role="alert">
          <p className="message message--error">
            The MAME installations could not be read: {describeError(error)}
          </p>
          <button type="button" className="button button--secondary" onClick={() => void reload()}>
            Retry
          </button>
        </div>
      )}
      {view !== null && installations.length === 0 && (
        <p className="message message--muted">
          No MAME installation is registered yet. Add a MAME executable or detect the installed
          ones.
        </p>
      )}

      {installations.length > 0 && (
        <ul className="installation-list" aria-label="Registered installations">
          {installations.map((installation) => {
            const label = labels.get(installation.id) ?? installation.label;
            const selected = installation.id === selectedId;
            return (
              <li
                key={installation.id}
                className={`installation-list__item${selected ? " installation-list__item--selected" : ""}`}
              >
                <div className="installation-list__info">
                  <p className="installation-list__title">
                    {label}
                    {installation.active && <span className="badge badge--success">Active</span>}
                  </p>
                  <p className="installation-list__path code">{installation.executablePath}</p>
                  <p className="installation-list__meta">
                    {installation.version ? formatVersion(installation.version) : "Version unknown"}{" "}
                    · {describeCatalog(installation)}
                    {installation.executableStatus !== "verified" &&
                      ` · ${describeExecutableStatus(installation.executableStatus)}`}
                  </p>
                </div>
                <button
                  type="button"
                  className="button button--secondary"
                  aria-pressed={selected}
                  aria-label={`Manage ${label}`}
                  onClick={() => {
                    onSelect(installation.id);
                  }}
                >
                  {selected ? "Selected" : "Manage"}
                </button>
              </li>
            );
          })}
        </ul>
      )}

      {rejected && (
        <div role="alert">
          <p className="message message--error">
            The selected file was not added: {describeStatus(rejected.status)}
          </p>
          <p className="message code">{rejected.executablePath}</p>
          {rejected.output && <RawOutput report={rejected} />}
        </div>
      )}
      {notice && (
        <p className="message message--success" role="status">
          {notice}
        </p>
      )}
      {actionError && (
        <p className="message message--error" role="alert">
          {actionError}
        </p>
      )}

      <div className="actions">
        <button
          type="button"
          className="button"
          disabled={busy}
          onClick={() =>
            void run("add", async () => {
              const path = await pickMameExecutable();
              if (path !== null) await register(path);
            })
          }
        >
          {pending === "add" ? "Validating…" : "Add installation…"}
        </button>
        <button
          type="button"
          className="button button--secondary"
          disabled={busy}
          onClick={() =>
            void run("detect", async () => {
              setDetected(await detectMameInstallations());
            })
          }
        >
          {pending === "detect" ? "Detecting…" : "Detect installations"}
        </button>
      </div>

      {detected && (
        <DetectedList
          candidates={detected}
          disabled={busy}
          onAdd={(path) => void run("add", () => register(path))}
        />
      )}
    </section>
  );
}

function DetectedList({
  candidates,
  disabled,
  onAdd,
}: {
  candidates: DetectedCandidate[];
  disabled: boolean;
  onAdd: (path: string) => void;
}) {
  return (
    <>
      <h3 className="panel__subtitle">Detected installations</h3>
      {candidates.length === 0 ? (
        <p className="message message--muted">
          No MAME executable was found in PATH or in standard locations. Use “Add installation…” to
          choose one manually.
        </p>
      ) : (
        <ul className="installation-list" aria-label="Detected installations">
          {candidates.map(({ source, report, registeredInstallationId }) => (
            <li key={report.executablePath} className="installation-list__item">
              <div className="installation-list__info">
                <p className="installation-list__path code">{report.executablePath}</p>
                <p className="installation-list__meta">
                  {describeSource(source)} ·{" "}
                  {report.version ? formatVersion(report.version) : describeStatus(report.status)}
                </p>
              </div>
              {registeredInstallationId !== null ? (
                <span className="message--muted">Registered</span>
              ) : (
                report.status.kind === "valid" && (
                  <button
                    type="button"
                    className="button button--secondary"
                    disabled={disabled}
                    aria-label={`Add ${report.executablePath}`}
                    onClick={() => {
                      onAdd(report.executablePath);
                    }}
                  >
                    Add
                  </button>
                )
              )}
            </li>
          ))}
        </ul>
      )}
    </>
  );
}
