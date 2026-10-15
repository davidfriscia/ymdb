import { useState } from "react";
import { describeError } from "../../shared/ipc";
import { formatCount } from "../catalog/catalogMessages";
import { useFavorites } from "./favoritesContext";
import {
  FAVORITES_LIST_ID,
  applyListsImport,
  exportLists,
  previewListsImport,
  type ImportMode,
  type ListsImportPreview,
} from "./listsApi";

type Status =
  | { kind: "idle" }
  | { kind: "busy" }
  | { kind: "done"; message: string }
  | { kind: "error"; message: string };

/**
 * The Favorites list in Settings: export it to a JSON file and import one,
 * merging or replacing after a summary. Favorites are global: they are the
 * same for every MAME installation.
 */
export function FavoritesSettings() {
  const favorites = useFavorites();
  const [status, setStatus] = useState<Status>({ kind: "idle" });
  const [preview, setPreview] = useState<ListsImportPreview | null>(null);
  const [confirmReplace, setConfirmReplace] = useState(false);
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

  function exportFavorites() {
    void run(async () => {
      const path = await exportLists([FAVORITES_LIST_ID]);
      return path === null ? null : `Favorites exported to ${path}.`;
    });
  }

  function chooseImport() {
    setPreview(null);
    setConfirmReplace(false);
    void run(async () => {
      setPreview(await previewListsImport());
      return null;
    });
  }

  function applyImport(mode: ImportMode) {
    if (!preview) return;
    const { importId } = preview;
    void run(async () => {
      const [outcome] = await applyListsImport(importId, mode);
      setPreview(null);
      setConfirmReplace(false);
      favorites.reload();
      if (!outcome) return "Nothing was imported.";
      return `Favorites imported: ${formatCount(outcome.added)} added, ${formatCount(outcome.removed)} removed, ${formatCount(outcome.entries)} in total.`;
    });
  }

  const summary = preview?.lists.find((list) => list.listId === FAVORITES_LIST_ID) ?? null;

  return (
    <section className="panel" aria-label="Favorites">
      <p className="message message--muted">
        Favorites are machines and software you bookmark in the catalog. They are saved by YMDB for
        every MAME installation, by MAME short name (and software list): a favorite shows up in
        every catalog that has it, and is kept when a catalog does not.
      </p>
      {favorites.loadError !== null ? (
        <div className="state-message" role="alert">
          <p className="message message--error">
            The favorites could not be loaded: {describeError(favorites.loadError)}
          </p>
          <button type="button" className="button button--secondary" onClick={favorites.reload}>
            Retry
          </button>
        </div>
      ) : (
        <p className="message" role="status">
          {favorites.ready
            ? `${formatCount(favorites.count)} ${favorites.count === 1 ? "favorite" : "favorites"}`
            : "Loading favorites…"}
        </p>
      )}

      <div className="actions">
        <button
          type="button"
          className="button button--secondary"
          disabled={busy || !favorites.ready}
          onClick={exportFavorites}
        >
          Export favorites…
        </button>
        <button
          type="button"
          className="button button--secondary"
          disabled={busy || favorites.loadError !== null}
          onClick={chooseImport}
        >
          Import favorites…
        </button>
      </div>
      <p className="message message--muted">
        Exports are JSON files with MAME names only: no paths, installations or settings. They can
        be imported on another computer or with another MAME version.
      </p>

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

      {preview && summary && (
        <div className="favorites-import" role="group" aria-label="Import summary">
          <h3 className="panel__subtitle">Import favorites</h3>
          <ul className="favorites-import__counts">
            <li>{formatCount(summary.entries)} in the file</li>
            <li>{formatCount(summary.newEntries)} new</li>
            <li>{formatCount(summary.existingEntries)} already in your favorites</li>
            {summary.duplicates > 0 && (
              <li>{formatCount(summary.duplicates)} duplicates in the file, imported once</li>
            )}
          </ul>
          {preview.ignoredLists.length > 0 && (
            <p className="message message--muted">
              Other lists in the file are not imported: {preview.ignoredLists.join(", ")}.
            </p>
          )}
          <p className="message message--muted">
            Entries missing from the catalog of the active MAME version are imported too.
          </p>
          {confirmReplace ? (
            <>
              <p className="message message--warning" role="alert">
                Replace your favorites with the {formatCount(summary.entries)} of the file?{" "}
                {summary.removedByReplace > 0
                  ? `${formatCount(summary.removedByReplace)} favorites that are not in the file will be removed.`
                  : "No favorite will be removed."}
              </p>
              <div className="actions">
                <button
                  type="button"
                  className="button"
                  disabled={busy}
                  onClick={() => {
                    applyImport("replace");
                  }}
                >
                  Replace favorites
                </button>
                <button
                  type="button"
                  className="button button--secondary"
                  disabled={busy}
                  onClick={() => {
                    setConfirmReplace(false);
                  }}
                >
                  Back
                </button>
              </div>
            </>
          ) : (
            <div className="actions">
              <button
                type="button"
                className="button"
                disabled={busy}
                onClick={() => {
                  applyImport("merge");
                }}
              >
                Merge
              </button>
              <button
                type="button"
                className="button button--secondary"
                disabled={busy}
                onClick={() => {
                  setConfirmReplace(true);
                }}
              >
                Replace…
              </button>
              <button
                type="button"
                className="button button--secondary"
                disabled={busy}
                onClick={() => {
                  setPreview(null);
                }}
              >
                Cancel
              </button>
            </div>
          )}
          <p className="message message--muted">
            Merge adds the new favorites and removes nothing. Replace makes your favorites exactly
            the ones of the file.
          </p>
        </div>
      )}
    </section>
  );
}
