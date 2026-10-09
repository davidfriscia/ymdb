import { useEffect, useState } from "react";
import { describeError } from "../../shared/ipc";
import { formatTimestamp } from "../mame-installation/messages";
import {
  cancelCatalogImport,
  getCatalogImportStatus,
  getCatalogStatus,
  startCatalogImport,
  type CatalogInfo,
  type CatalogStatus,
  type ImportStatus,
} from "./catalogApi";
import {
  describeFreshness,
  describeImportError,
  describePhase,
  formatBytes,
  formatCount,
  formatDuration,
} from "./catalogMessages";

interface CatalogPanelProps {
  /** Changes whenever the MAME configuration changes, to refresh the status. */
  refreshKey?: number;
  /** Interval between import status queries while an import runs. */
  pollIntervalMs?: number;
}

export function CatalogPanel({ refreshKey = 0, pollIntervalMs = 500 }: CatalogPanelProps) {
  const [status, setStatus] = useState<CatalogStatus | null>(null);
  const [importStatus, setImportStatus] = useState<ImportStatus>({ state: "idle" });
  const [pending, setPending] = useState<"start" | "cancel" | null>(null);
  const [error, setError] = useState<string | null>(null);
  const running = importStatus.state === "running";

  useEffect(() => {
    let active = true;
    getCatalogStatus().then(
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
  }, [refreshKey]);

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
        const refreshed = await getCatalogStatus();
        if (!isActive()) return;
        setStatus(refreshed);
        setImportStatus(next);
      } catch (pollError) {
        if (isActive()) setError(describeError(pollError));
      }
    }

    polling.timer = window.setTimeout(() => void poll(), pollIntervalMs);
    return () => {
      polling.active = false;
      window.clearTimeout(polling.timer);
    };
  }, [running, pollIntervalMs]);

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
  const freshness = status?.freshness ? describeFreshness(status.freshness) : null;
  const canImport = status?.mameConfigured === true && !running && pending === null;

  return (
    <section className="panel" aria-labelledby="catalog-title" aria-busy={running}>
      <h2 id="catalog-title" className="panel__title">
        MAME Catalog
      </h2>

      {status === null && error === null && (
        <p className="message message--muted">Loading catalog…</p>
      )}

      {status !== null &&
        (catalog ? (
          <CatalogSummary catalog={catalog} />
        ) : (
          <p className="message message--muted">No catalog has been imported yet.</p>
        ))}

      {status?.catalogError && (
        <p className="message message--error" role="alert">
          The catalog cannot be read: {status.catalogError}. Import the catalog again.
        </p>
      )}

      {freshness && (
        <p className={`message ${freshness.warning ? "message--warning" : "message--muted"}`}>
          {freshness.message}
        </p>
      )}

      <ImportState status={importStatus} />

      {error && (
        <p className="message message--error" role="alert">
          {error}
        </p>
      )}

      <div className="actions">
        <button
          type="button"
          className="button"
          onClick={() => void run("start", startCatalogImport)}
          disabled={!canImport}
        >
          {catalog ? "Reimport catalog" : "Import catalog"}
        </button>
        {running && (
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
    </section>
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
    ["Software lists", formatCount(stats.softwareLists)],
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
          {/* No value: the total number of machines is not known in advance. */}
          <progress className="import-progress__bar" aria-label="Import progress" />
          <p className="message">
            {describePhase(status.phase)} {formatCount(status.machinesProcessed)} machines
            processed.
          </p>
        </div>
      );
    case "completed":
      return (
        <p className="message message--success" role="status">
          Catalog imported: {formatCount(status.machines)} machines in{" "}
          {formatDuration(status.durationMs)}.
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
