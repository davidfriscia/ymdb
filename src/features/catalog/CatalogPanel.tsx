import { useEffect, useState } from "react";
import { describeError } from "../../shared/ipc";
import { formatTimestamp } from "../mame-installation/messages";
import {
  cancelCatalogImport,
  getCatalogImportStatus,
  getCatalogStatus,
  startCatalogImport,
  startSoftwareListImport,
  type CatalogInfo,
  type CatalogStatus,
  type ImportStatus,
  type SoftwareCatalogInfo,
} from "./catalogApi";
import {
  describeFreshness,
  describeImportError,
  describePhase,
  formatBytes,
  formatCount,
  formatDuration,
  importSubject,
  itemsLabel,
} from "./catalogMessages";
import type { InstallationSummary } from "../installations/installationsApi";
import { useInstallations } from "../installations/installationsContext";

interface CatalogPanelProps {
  /** The installation whose catalog is shown and imported. */
  installation: InstallationSummary;
  /** Changes whenever a catalog may have changed, to refresh the status. */
  refreshKey?: number;
  /** Interval between import status queries while an import runs. */
  pollIntervalMs?: number;
  /** Called after an import replaced the catalog. */
  onCatalogChanged?: () => void;
}

export function CatalogPanel({
  installation,
  refreshKey = 0,
  pollIntervalMs = 500,
  onCatalogChanged,
}: CatalogPanelProps) {
  const [status, setStatus] = useState<CatalogStatus | null>(null);
  const [importStatus, setImportStatus] = useState<ImportStatus>({ state: "idle" });
  const [pending, setPending] = useState<"start" | "cancel" | null>(null);
  const [error, setError] = useState<string | null>(null);
  const { labels } = useInstallations();
  const installationId = installation.id;
  const running = importStatus.state === "running";
  // Only one import runs at a time, possibly for another installation.
  const importHere =
    importStatus.state === "idle" || importStatus.installationId === installationId;

  useEffect(() => {
    let active = true;
    getCatalogStatus(installationId).then(
      (loaded) => {
        if (!active) return;
        setStatus(loaded);
        setImportStatus(loaded.import);
      },
      (loadError: unknown) => {
        if (active) setError(describeError(loadError));
      },
    );
    return () => {
      active = false;
    };
  }, [installationId, refreshKey]);

  useEffect(() => {
    if (!running) return;
    const polling = { active: true, timer: 0 };
    // Read through a function: the flag changes asynchronously in the cleanup.
    const isActive = () => polling.active;

    async function poll() {
      try {
        const next = await getCatalogImportStatus();
        if (next.state === "running") {
          if (!isActive()) return;
          setImportStatus(next);
          polling.timer = window.setTimeout(() => void poll(), pollIntervalMs);
          return;
        }
        // The import ended: read the (possibly new) catalog before leaving the
        // running state, which stops this effect.
        if (!isActive()) return;
        const refreshed = await getCatalogStatus(installationId);
        if (!isActive()) return;
        setStatus(refreshed);
        setImportStatus(next);
        // Panels of other installations may be mounted too: only this one reports.
        if (next.state === "completed" && next.installationId === installationId) {
          onCatalogChanged?.();
        }
      } catch (pollError) {
        if (isActive()) setError(describeError(pollError));
      }
    }

    polling.timer = window.setTimeout(() => void poll(), pollIntervalMs);
    return () => {
      polling.active = false;
      window.clearTimeout(polling.timer);
    };
  }, [running, installationId, pollIntervalMs, onCatalogChanged]);

  async function run(action: "start" | "cancel", work: () => Promise<ImportStatus>) {
    setPending(action);
    setError(null);
    try {
      setImportStatus(await work());
    } catch (actionError) {
      setError(describeError(actionError));
    } finally {
      setPending(null);
    }
  }

  const catalog = status?.catalog ?? null;
  const software = catalog?.software ?? null;
  const freshness = status?.freshness ? describeFreshness(status.freshness) : null;
  const idle = !running && pending === null;
  const canImportMachines = status !== null && installation.executableStatus !== "missing" && idle;
  // Software lists must come from the executable the machines came from.
  const canImportSoftware = catalog !== null && status?.freshness === "current" && idle;

  return (
    <section
      className="panel"
      aria-label={`Catalog of ${labels.get(installationId) ?? installation.label}`}
      aria-busy={running}
    >
      {status === null && error === null && (
        <p className="message message--muted">Loading catalog…</p>
      )}

      {status?.catalogError && (
        <p className="message message--error" role="alert">
          The catalog cannot be read: {status.catalogError}. Import the catalog again.
        </p>
      )}

      {status !== null && (
        <>
          <h3 className="panel__subtitle">Machines</h3>
          {catalog ? (
            <CatalogSummary catalog={catalog} />
          ) : (
            <p className="message message--muted">No catalog has been imported yet.</p>
          )}

          {freshness && (
            <p className={`message ${freshness.warning ? "message--warning" : "message--muted"}`}>
              {freshness.message}
            </p>
          )}

          <h3 className="panel__subtitle">Software lists</h3>
          {software ? (
            <SoftwareSummary software={software} />
          ) : (
            <p className="message message--muted">
              {catalog
                ? "Software lists have not been imported for this catalog."
                : "Import the machine catalog first."}
            </p>
          )}
          {software && !software.matchesMachineCatalog && (
            <p className="message message--warning">
              These software lists do not come from the MAME build of the machine catalog. Import
              them again.
            </p>
          )}
        </>
      )}

      {importHere ? (
        <ImportState status={importStatus} />
      ) : (
        running && (
          <p className="message message--muted" role="status">
            An import of {labels.get(importStatus.installationId) ?? "another installation"} is
            running. Only one import runs at a time.
          </p>
        )
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
          onClick={() => void run("start", () => startCatalogImport(installationId))}
          disabled={!canImportMachines}
        >
          {catalog ? "Reimport machines" : "Import catalog"}
        </button>
        <button
          type="button"
          className="button"
          onClick={() => void run("start", () => startSoftwareListImport(installationId))}
          disabled={!canImportSoftware}
        >
          {software ? "Update software lists" : "Import software lists"}
        </button>
        {running && importHere && (
          <button
            type="button"
            className="button button--secondary"
            onClick={() => void run("cancel", cancelCatalogImport)}
            disabled={importStatus.cancelRequested || pending !== null}
          >
            {importStatus.cancelRequested ? "Cancelling…" : "Cancel import"}
          </button>
        )}
      </div>
      {software && (
        <p className="message message--muted">
          Reimporting machines starts a new catalog: import the software lists again afterwards.
        </p>
      )}
    </section>
  );
}

function SoftwareSummary({ software }: { software: SoftwareCatalogInfo }) {
  const stats = software.statistics;
  const rows: [string, string][] = [
    ["MAME version", `${software.mameRelease} (${software.mameRevision})`],
    ["Imported", formatTimestamp(software.importedAt)],
    ["Software lists", formatCount(stats.lists)],
    ["Software", formatCount(stats.software)],
    ["Clones", formatCount(stats.clones)],
    ["Partially supported", formatCount(stats.partiallySupported)],
    ["Unsupported", formatCount(stats.unsupported)],
    ["Parts", formatCount(stats.parts)],
    ["ROMs", formatCount(stats.roms)],
    ["Disks", formatCount(stats.disks)],
    ["Lists referenced but not provided", formatCount(stats.missingReferencedLists)],
    ["Import duration", formatDuration(software.importDurationMs)],
  ];
  return (
    <dl className="details-list" aria-label="Software list summary">
      {rows.map(([label, value]) => (
        <div key={label} className="details-list__row">
          <dt>{label}</dt>
          <dd>{value}</dd>
        </div>
      ))}
    </dl>
  );
}

function CatalogSummary({ catalog }: { catalog: CatalogInfo }) {
  const stats = catalog.statistics;
  const rows: [string, string][] = [
    ["MAME version", `${catalog.mameRelease} (${catalog.mameRevision})`],
    ["Imported", formatTimestamp(catalog.importedAt)],
    ["Machines", formatCount(stats.machines)],
    ["Runnable systems", formatCount(stats.runnableSystems)],
    ["Clones", formatCount(stats.clones)],
    ["BIOS sets", formatCount(stats.bios)],
    ["Devices", formatCount(stats.devices)],
    ["Mechanical", formatCount(stats.mechanical)],
    ["ROMs", formatCount(stats.roms)],
    ["Disks (CHD)", formatCount(stats.disks)],
    ["Software lists referenced", formatCount(stats.softwareLists)],
    ["Database size", formatBytes(catalog.databaseSize)],
    ["Import duration", formatDuration(catalog.importDurationMs)],
  ];
  return (
    <dl className="details-list" aria-label="Catalog summary">
      {rows.map(([label, value]) => (
        <div key={label} className="details-list__row">
          <dt>{label}</dt>
          <dd>{value}</dd>
        </div>
      ))}
    </dl>
  );
}

function ImportState({ status }: { status: ImportStatus }) {
  switch (status.state) {
    case "idle":
      return null;
    case "running":
      return (
        <div className="import-progress" role="status">
          {/* No value: the total number of items is not known in advance. */}
          <progress className="import-progress__bar" aria-label="Import progress" />
          <p className="message">
            {describePhase(status.kind, status.phase)} {formatCount(status.itemsProcessed)}{" "}
            {itemsLabel(status.kind)} processed.
          </p>
        </div>
      );
    case "completed":
      return (
        <p className="message message--success" role="status">
          {importSubject(status.kind)} imported: {formatCount(status.items)}{" "}
          {itemsLabel(status.kind)} in {formatDuration(status.durationMs)}.
        </p>
      );
    case "cancelled":
      return (
        <p className="message message--muted" role="status">
          Import cancelled. The previous catalog, if any, was kept.
        </p>
      );
    case "failed":
      return (
        <p className="message message--error" role="alert">
          Import failed: {describeImportError(status.error)} The previous catalog, if any, was kept.
        </p>
      );
  }
}
