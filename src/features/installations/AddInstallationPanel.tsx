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

interface AddInstallationPanelProps {
  /** Shows an installation (added, or already registered) with a message. */
  onShowInstallation: (id: string, message: string) => void;
}

/** Adds MAME installations: a chosen executable or a detected one. */
export function AddInstallationPanel({ onShowInstallation }: AddInstallationPanelProps) {
  const { view, installations, labels, reload } = useInstallations();
  const [pending, setPending] = useState<"add" | "detect" | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [rejected, setRejected] = useState<ValidationReport | null>(null);
  const [detected, setDetected] = useState<DetectedCandidate[] | null>(null);

  async function run(action: "add" | "detect", work: () => Promise<void>) {
    setPending(action);
    setActionError(null);
    try {
      await work();
    } catch (actionFailure) {
      const detail = actionFailure instanceof CommandFailedError ? actionFailure.detail : null;
      if (detail?.kind === "duplicateInstallation") {
        // Show the installation that already registers it.
        onShowInstallation(
          detail.existingInstallationId,
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
    setDetected(
      (current) =>
        current?.map((candidate) =>
          candidate.report.executablePath === path
            ? { ...candidate, registeredInstallationId: added.id }
            : candidate,
        ) ?? null,
    );
    onShowInstallation(
      added.id,
      `${added.label} was added. Its catalog has not been imported yet.`,
    );
  }

  const busy = pending !== null;
  return (
    <section className="panel" aria-label="Add a MAME installation" aria-busy={busy}>
      <p className="message message--muted">
        Register every MAME version you use. Each installation has its own catalog, launch options
        and ROM Manager settings; the one selected in the catalog is the active installation.
      </p>
      {view !== null && installations.length === 0 && (
        <p className="message message--muted">
          No MAME installation is registered yet. Add a MAME executable or detect the installed
          ones.
        </p>
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
