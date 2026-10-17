import { useState } from "react";
import { describeError } from "../../shared/ipc";
import { useLoader } from "../../shared/useLoader";
import { formatCount } from "../catalog/catalogMessages";
import { formatTimestamp } from "../mame-installation/messages";
import {
  DIMENSIONS,
  DIMENSION_LABELS,
  FORMAT_FILE_NAMES,
  FORMAT_LABELS,
  WARNING_LABELS,
  applySupportFileImport,
  getSupportFiles,
  previewSupportFileImport,
  removeSupportFile,
  setActiveSupportSource,
  type Dimension,
  type ParseWarning,
  type SupportFileFormat,
  type SupportFileImportPreview,
  type SupportFilesView,
  type SupportSource,
} from "./supportFilesApi";

type Status =
  | { kind: "idle" }
  | { kind: "busy" }
  | { kind: "done"; message: string }
  | { kind: "error"; message: string };

interface SupportFilesSettingsProps {
  /** Called after every change, so that catalogs reload their classifications. */
  onChanged: () => void;
}

/**
 * Settings › Support Files: the MAME support files imported into YMDB, the
 * same for every MAME installation. Files are imported manually, checked and
 * summarized before they are stored, and never modified; removing a source
 * removes it from YMDB only. Catalogs pick up every change the next time they
 * are opened.
 */
export function SupportFilesSettings({ onChanged }: SupportFilesSettingsProps) {
  const { state, reload } = useLoader(getSupportFiles, 0);
  const [changed, setView] = useState<SupportFilesView | null>(null);
  const view = changed ?? (state.status === "ready" ? state.value : null);
  const [status, setStatus] = useState<Status>({ kind: "idle" });
  const [preview, setPreview] = useState<SupportFileImportPreview | null>(null);
  const [removing, setRemoving] = useState<SupportFileFormat | null>(null);
  const busy = status.kind === "busy";

  async function run(work: () => Promise<string | null>) {
    setStatus({ kind: "busy" });
    try {
      const message = await work();
      setStatus(message === null ? { kind: "idle" } : { kind: "done", message });
    } catch (error) {
      setStatus({ kind: "error", message: describeError(error) });
    }
  }

  function chooseFile(expected: SupportFileFormat | null) {
    setPreview(null);
    setRemoving(null);
    void run(async () => {
      setPreview(await previewSupportFileImport(expected));
      return null;
    });
  }

  function confirmImport() {
    if (!preview) return;
    const { importId, fileName } = preview;
    void run(async () => {
      setView(await applySupportFileImport(importId));
      setPreview(null);
      onChanged();
      return `${fileName} imported.`;
    });
  }

  function remove(format: SupportFileFormat) {
    void run(async () => {
      setView(await removeSupportFile(format));
      setRemoving(null);
      onChanged();
      return `${FORMAT_FILE_NAMES[format]} removed from YMDB. The file itself was not touched.`;
    });
  }

  function setActive(dimension: Dimension, format: SupportFileFormat | null) {
    void run(async () => {
      setView(await setActiveSupportSource(dimension, format));
      onChanged();
      return null;
    });
  }

  return (
    <section className="panel support-files" aria-label="Support Files">
      <p className="message message--muted">
        MAME support files classify machines by genre, category, series and the MAME version that
        added them. They are maintained by third parties (for example progettoSNAPS): download them
        yourself and import them here. YMDB keeps a copy of their data for every MAME installation,
        matched by machine short name, and never changes the original files.
      </p>
      <div className="actions">
        <button
          type="button"
          className="button"
          disabled={busy}
          onClick={() => {
            chooseFile(null);
          }}
        >
          Import support file…
        </button>
      </div>

      {status.kind === "error" && (
        <p className="message message--error" role="alert">
          {status.message}
        </p>
      )}
      {status.kind === "done" && (
        <p className="message message--success" role="status">
          {status.message}
        </p>
      )}

      {preview && (
        <ImportSummary
          preview={preview}
          busy={busy}
          onConfirm={confirmImport}
          onCancel={() => {
            setPreview(null);
          }}
        />
      )}

      {view === null ? (
        state.status === "error" ? (
          <div className="state-message" role="alert">
            <p className="message message--error">
              The support files could not be read: {describeError(state.error)}
            </p>
            <button type="button" className="button button--secondary" onClick={reload}>
              Retry
            </button>
          </div>
        ) : (
          <p className="message message--muted">Loading support files…</p>
        )
      ) : (
        <>
          <h3 className="panel__subtitle">Classifications</h3>
          <ActiveSources view={view} busy={busy} onChange={setActive} />

          <h3 className="panel__subtitle">Imported files</h3>
          {view.sources.length === 0 ? (
            <p className="message message--muted">
              No support file has been imported yet. Supported files: catver.ini, genre.ini,
              catlist.ini, version.ini and series.ini.
            </p>
          ) : (
            <ul className="support-files__sources">
              {view.sources.map((source) => (
                <SourceCard
                  key={source.format}
                  source={source}
                  busy={busy}
                  removing={removing === source.format}
                  onReimport={() => {
                    chooseFile(source.format);
                  }}
                  onRemove={() => {
                    setPreview(null);
                    setRemoving(source.format);
                  }}
                  onConfirmRemove={() => {
                    remove(source.format);
                  }}
                  onCancelRemove={() => {
                    setRemoving(null);
                  }}
                />
              ))}
            </ul>
          )}
        </>
      )}
    </section>
  );
}

function ImportSummary({
  preview,
  busy,
  onConfirm,
  onCancel,
}: {
  preview: SupportFileImportPreview;
  busy: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  const { replaces } = preview;
  return (
    <div className="support-files__preview" role="group" aria-label="Import summary">
      <h3 className="panel__subtitle">
        Import {preview.fileName} ({FORMAT_LABELS[preview.format]})
      </h3>
      <ul className="support-files__facts">
        <li>Declared MAME version: {preview.declaredVersion ?? "not declared"}</li>
        <li>{formatCount(preview.machineCount)} machines</li>
        {preview.dimensions.map((entry) => (
          <li key={entry.dimension}>
            {DIMENSION_LABELS[entry.dimension]}: {formatCount(entry.valueCount)} values for{" "}
            {formatCount(entry.machineCount)} machines
            {entry.activeAfterImport
              ? " — will be used"
              : " — another source is active; select this one below to use it"}
          </li>
        ))}
      </ul>
      <Warnings warnings={preview.warnings} />
      {replaces && (
        <p className="message message--warning" role="alert">
          This replaces the {FORMAT_FILE_NAMES[replaces.format]} imported on{" "}
          {formatTimestamp(replaces.importedAt)} ({replaces.fileName}
          {replaces.declaredVersion ? `, MAME ${replaces.declaredVersion}` : ""}).
        </p>
      )}
      <div className="actions">
        <button type="button" className="button" disabled={busy} onClick={onConfirm}>
          {replaces ? "Replace" : "Import"}
        </button>
        <button
          type="button"
          className="button button--secondary"
          disabled={busy}
          onClick={onCancel}
        >
          Cancel
        </button>
      </div>
      <p className="message message--muted">
        Machines missing from a catalog are kept: they are used by the catalogs that have them.
      </p>
    </div>
  );
}

function ActiveSources({
  view,
  busy,
  onChange,
}: {
  view: SupportFilesView;
  busy: boolean;
  onChange: (dimension: Dimension, format: SupportFileFormat | null) => void;
}) {
  return (
    <div className="support-files__dimensions">
      {DIMENSIONS.map((dimension) => {
        const entry = view.dimensions.find((candidate) => candidate.dimension === dimension);
        const sources = entry?.availableSources ?? [];
        const active = entry?.activeSource ?? null;
        return (
          <div key={dimension} className="support-files__dimension">
            <label className="support-files__dimension-label">
              <span>{DIMENSION_LABELS[dimension]}</span>
              <select
                className="input"
                value={active ?? ""}
                disabled={busy || sources.length === 0}
                onChange={(event) => {
                  onChange(
                    dimension,
                    event.target.value === "" ? null : (event.target.value as SupportFileFormat),
                  );
                }}
              >
                <option value="">{sources.length === 0 ? "No source imported" : "Not used"}</option>
                {sources.map((format) => (
                  <option key={format} value={format}>
                    {FORMAT_FILE_NAMES[format]}
                  </option>
                ))}
              </select>
            </label>
            {active === null && sources.length > 0 && (
              <p className="message message--warning">
                {DIMENSION_LABELS[dimension]} has no active source: select one to use it.
              </p>
            )}
          </div>
        );
      })}
      <p className="message message--muted">
        Each classification uses one source. The first file imported for a classification becomes
        its source; afterwards it changes only when you select another one here.
      </p>
    </div>
  );
}

function SourceCard({
  source,
  busy,
  removing,
  onReimport,
  onRemove,
  onConfirmRemove,
  onCancelRemove,
}: {
  source: SupportSource;
  busy: boolean;
  removing: boolean;
  onReimport: () => void;
  onRemove: () => void;
  onConfirmRemove: () => void;
  onCancelRemove: () => void;
}) {
  const active = source.dimensions.filter((entry) => entry.active);
  return (
    <li className="support-files__source" aria-label={FORMAT_FILE_NAMES[source.format]}>
      <div className="support-files__source-header">
        <strong>{FORMAT_LABELS[source.format]}</strong>
        <span className={active.length > 0 ? "badge badge--success" : "badge"}>
          {active.length > 0 ? "Active" : "Inactive"}
        </span>
      </div>
      <dl className="details-list">
        <div className="details-list__row">
          <dt>File</dt>
          <dd className="code">{source.fileName}</dd>
        </div>
        <div className="details-list__row">
          <dt>MAME version</dt>
          <dd>{source.declaredVersion ?? "Not declared"}</dd>
        </div>
        <div className="details-list__row">
          <dt>Imported</dt>
          <dd>{formatTimestamp(source.importedAt)}</dd>
        </div>
        <div className="details-list__row">
          <dt>Machines</dt>
          <dd>{formatCount(source.machineCount)}</dd>
        </div>
        {source.dimensions.map((entry) => (
          <div key={entry.dimension} className="details-list__row">
            <dt>{DIMENSION_LABELS[entry.dimension]}</dt>
            <dd>
              {formatCount(entry.valueCount)}{" "}
              {entry.dimension === "versionAdded" ? "versions" : "values"}
              {entry.active ? " · active" : " · not used"}
            </dd>
          </div>
        ))}
      </dl>
      <Warnings warnings={source.warnings} />
      {removing ? (
        <div className="support-files__confirm" role="alert">
          <p className="message message--warning">
            Remove {FORMAT_FILE_NAMES[source.format]} from YMDB?{" "}
            {active.length > 0
              ? `${active.map((entry) => DIMENSION_LABELS[entry.dimension]).join(" and ")} will have no source until you select or import another one.`
              : "It is not used by any classification."}{" "}
            The file on disk is not deleted.
          </p>
          <div className="actions">
            <button type="button" className="button" disabled={busy} onClick={onConfirmRemove}>
              Remove
            </button>
            <button
              type="button"
              className="button button--secondary"
              disabled={busy}
              onClick={onCancelRemove}
            >
              Cancel
            </button>
          </div>
        </div>
      ) : (
        <div className="actions">
          <button
            type="button"
            className="button button--secondary button--small"
            disabled={busy}
            onClick={onReimport}
            aria-label={`Reimport ${FORMAT_FILE_NAMES[source.format]}`}
          >
            Reimport…
          </button>
          <button
            type="button"
            className="button button--secondary button--small"
            disabled={busy}
            onClick={onRemove}
            aria-label={`Remove ${FORMAT_FILE_NAMES[source.format]}`}
          >
            Remove…
          </button>
        </div>
      )}
    </li>
  );
}

function Warnings({ warnings }: { warnings: ParseWarning[] }) {
  if (warnings.length === 0) return null;
  const total = warnings.reduce((sum, warning) => sum + warning.count, 0);
  return (
    <details className="support-files__warnings">
      <summary>
        {formatCount(total)} {total === 1 ? "warning" : "warnings"}
      </summary>
      <ul>
        {warnings.map((warning) => (
          <li key={warning.kind}>
            {WARNING_LABELS[warning.kind]}: {formatCount(warning.count)}
            {warning.examples.length > 0 && (
              <span className="code support-files__examples"> ({warning.examples.join("; ")})</span>
            )}
          </li>
        ))}
      </ul>
    </details>
  );
}
