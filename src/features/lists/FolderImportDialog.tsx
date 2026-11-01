import { useCallback, useEffect, useId, useState } from "react";
import { describeError } from "../../shared/ipc";
import { Modal } from "../../shared/Modal";
import { useLoader } from "../../shared/useLoader";
import { formatCount } from "../catalog/catalogMessages";
import {
  applyFolderImport,
  previewFolderImport,
  type FolderFileSummary,
  type FolderImportOutcome,
  type FolderNaming,
  type IniWarning,
  type IniWarningKind,
} from "./listsApi";

interface FolderImportDialogProps {
  file: FolderFileSummary;
  /** Called after the import was applied. */
  onImported: (outcome: FolderImportOutcome) => void;
  onClose: () => void;
}

const WARNING_LABELS: Record<IniWarningKind, string> = {
  notUtf8: "The file is not UTF-8; it was read as Windows-1252",
  missingRootFolder: "The file has no [ROOT_FOLDER] section",
  outsideSection: "Lines before the first section were ignored",
  invalidLine: "Lines that are not MAME short names were ignored",
  duplicateEntry: "Machines listed twice in a folder are imported once",
  duplicateFolder: "Folders listed twice in the file are combined",
  unnamedSection: "Sections without a name were ignored",
};

/** Planned lists shown at once; the search finds the others. */
const SHOWN_LISTS = 300;
/** Pause after typing the name before the preview is computed again. */
const PREVIEW_DELAY_MS = 250;

function plural(count: number, word: string): string {
  return `${formatCount(count)} ${count === 1 ? word : `${word}s`}`;
}

function Warnings({ warnings }: { warnings: IniWarning[] }) {
  if (warnings.length === 0) return null;
  return (
    <div className="message message--warning" role="status">
      <ul className="folder-import__warnings">
        {warnings.map((warning) => (
          <li key={warning.kind}>
            {WARNING_LABELS[warning.kind]} ({formatCount(warning.count)})
            {warning.examples.length > 0 && (
              <span className="folder-import__examples">: {warning.examples.join("; ")}</span>
            )}
          </li>
        ))}
      </ul>
    </div>
  );
}

/**
 * Import of a MAME folder file: the user names the import and, for files
 * with subfolders, chooses how their lists are named. The preview is
 * computed by the backend for these choices and changes nothing; Import
 * applies it in one change. Lists whose name already exists (ignoring case)
 * are merged: their entries are kept and only the missing machines added.
 */
export function FolderImportDialog({ file, onImported, onClose }: FolderImportDialogProps) {
  const nameId = useId();
  const [name, setName] = useState(file.suggestedName);
  const [naming, setNaming] = useState<FolderNaming>("prefixed");
  const [previewName, setPreviewName] = useState(file.suggestedName);
  const [search, setSearch] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // The preview follows the name after a short pause, not every key press.
  useEffect(() => {
    const timer = setTimeout(() => {
      setPreviewName(name);
    }, PREVIEW_DELAY_MS);
    return () => {
      clearTimeout(timer);
    };
  }, [name]);

  const trimmed = previewName.trim();
  const loadPreview = useCallback(
    () =>
      trimmed === "" ? Promise.resolve(null) : previewFolderImport(file.importId, trimmed, naming),
    [file.importId, trimmed, naming],
  );
  const preview = useLoader(loadPreview);
  const current = preview.state.status === "ready" ? preview.state.value : null;
  const upToDate = name.trim() === trimmed && current !== null;

  const query = search.trim().toLowerCase();
  const planned = current?.lists ?? [];
  const matching =
    query === "" ? planned : planned.filter((list) => list.name.toLowerCase().includes(query));

  const apply = () => {
    if (!current) return;
    setBusy(true);
    setError(null);
    applyFolderImport(file.importId, current.name, current.naming).then(
      (outcome) => {
        onImported(outcome);
      },
      (failure: unknown) => {
        setError(describeError(failure));
        setBusy(false);
      },
    );
  };

  const hasSubfolders = file.folders > 0;
  const nothingToImport = current !== null && current.toCreate === 0 && current.newEntries === 0;

  return (
    <Modal title="Import lists" onClose={onClose}>
      <div className="folder-import">
        <p className="folder-import__file">
          <span className="code">{file.fileName}</span>:{" "}
          {file.rootEntries > 0
            ? `${plural(file.rootEntries, "machine")} in the root folder`
            : "an empty root folder"}
          {hasSubfolders && `, ${plural(file.folders, "subfolder")}`}
          {file.emptyFolders > 0 && ` (${formatCount(file.emptyFolders)} empty, ignored)`}.
        </p>
        <Warnings warnings={file.warnings} />

        <label className="field" htmlFor={nameId}>
          <span className="field__label">Import name</span>
          <input
            id={nameId}
            className="input"
            value={name}
            disabled={busy}
            onChange={(event) => {
              setName(event.target.value);
            }}
          />
        </label>
        <p className="message message--muted">
          {hasSubfolders
            ? file.rootEntries > 0
              ? "The machines of the root folder become a list with this name."
              : "The root folder is empty: it creates no list."
            : "The machines of the file become a list with this name."}
        </p>

        {hasSubfolders && (
          <fieldset className="folder-import__naming" disabled={busy}>
            <legend className="field__label">Subfolder lists are named</legend>
            <label className="view-panel__option--toggle folder-import__choice">
              <input
                type="radio"
                name={`${nameId}-naming`}
                checked={naming === "subfolder"}
                onChange={() => {
                  setNaming("subfolder");
                }}
              />
              <span>
                By subfolder name <span className="code">Arcade / Pinball</span>
              </span>
            </label>
            <label className="view-panel__option--toggle folder-import__choice">
              <input
                type="radio"
                name={`${nameId}-naming`}
                checked={naming === "prefixed"}
                onChange={() => {
                  setNaming("prefixed");
                }}
              />
              <span>
                With the import name first{" "}
                <span className="code">{(name.trim() || "Name") + " / Arcade / Pinball"}</span>
              </span>
            </label>
          </fieldset>
        )}

        <h3 className="panel__subtitle">Preview</h3>
        {trimmed === "" ? (
          <p className="message message--muted">Enter a name for the import.</p>
        ) : preview.state.status === "error" ? (
          <p className="message message--error" role="alert">
            {describeError(preview.state.error)}
          </p>
        ) : current === null ? (
          <p className="message message--muted" role="status">
            Preparing the preview…
          </p>
        ) : (
          <>
            <ul className="folder-import__counts" aria-label="Import summary">
              <li>{plural(current.toCreate, "list")} to create</li>
              <li>
                {plural(current.toMerge, "existing list")} to update (merged: nothing is removed)
              </li>
              {current.emptyFolders > 0 && (
                <li>{plural(current.emptyFolders, "empty subfolder")} ignored</li>
              )}
              <li>{plural(current.newEntries, "machine")} to add</li>
              {current.existingEntries > 0 && (
                <li>{plural(current.existingEntries, "machine")} already in their list</li>
              )}
              {current.duplicates > 0 && (
                <li>{plural(current.duplicates, "duplicate")} in the file, imported once</li>
              )}
            </ul>
            <p className="message message--muted">
              Machines missing from the catalog of the active MAME version are imported too.
            </p>
            {nothingToImport && (
              <p className="message message--success" role="status">
                Nothing to import: these lists already have every machine of the file.
              </p>
            )}
            <input
              className="input folder-import__search"
              type="search"
              aria-label="Search the resulting lists"
              placeholder="Search the resulting lists"
              value={search}
              onChange={(event) => {
                setSearch(event.target.value);
              }}
            />
            <ul className="folder-import__lists" aria-label="Resulting lists">
              {matching.slice(0, SHOWN_LISTS).map((list) => (
                <li key={list.name} className="folder-import__list">
                  <span className="folder-import__list-name">{list.name}</span>
                  <span className={`badge${list.action === "merge" ? " badge--warning" : ""}`}>
                    {list.action === "merge" ? "Update" : "New"}
                  </span>
                  <span className="folder-import__list-count">
                    {list.action === "merge"
                      ? `+${formatCount(list.newEntries)} of ${formatCount(list.entries)}`
                      : formatCount(list.entries)}
                  </span>
                </li>
              ))}
              {matching.length === 0 && (
                <li className="message message--muted">No list matches the search.</li>
              )}
              {matching.length > SHOWN_LISTS && (
                <li className="message message--muted">
                  {formatCount(matching.length - SHOWN_LISTS)} more lists: refine the search to see
                  them.
                </li>
              )}
            </ul>
          </>
        )}

        {error && (
          <p className="message message--error" role="alert">
            {error}
          </p>
        )}
        <div className="actions folder-import__actions">
          <button
            type="button"
            className="button"
            disabled={busy || !upToDate || nothingToImport}
            onClick={apply}
          >
            {busy ? "Importing…" : "Import"}
          </button>
          <button type="button" className="button button--secondary" onClick={onClose}>
            Cancel
          </button>
        </div>
      </div>
    </Modal>
  );
}
