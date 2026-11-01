import { useCallback, useId, useMemo, useState, type SyntheticEvent } from "react";
import { describeError } from "../../shared/ipc";
import { Modal } from "../../shared/Modal";
import { useLoader } from "../../shared/useLoader";
import { useCustomLists } from "./customListsContext";
import {
  createList,
  getEntryLists,
  listNameKey,
  setEntryMemberships,
  type ListEntry,
  type MembershipChange,
} from "./listsApi";

interface ManageListsModalProps {
  entry: ListEntry;
  /** Title of the machine, shown with its short name. */
  title: string;
  onClose: () => void;
}

/** Lists rendered at once; a search finds the others. */
const SHOWN_LISTS = 300;

function shortName(entry: ListEntry): string {
  return entry.kind === "machine" ? entry.name : `${entry.listName}:${entry.name}`;
}

/**
 * Manage Lists: the custom lists of one machine, as checkboxes. Changes are
 * a draft until Save, which sends only the lists whose state changed, in one
 * backend change: lists not touched here keep whatever happened to them
 * since the window opened. Cancel discards the draft. Favorites is not
 * listed: it keeps its own bookmark.
 */
export function ManageListsModal({ entry, title, onClose }: ManageListsModalProps) {
  const customLists = useCustomLists();
  const searchId = useId();
  const loadMemberships = useCallback(async () => new Set(await getEntryLists(entry)), [entry]);
  const memberships = useLoader(loadMemberships);
  const initial = memberships.state.status === "ready" ? memberships.state.value : null;
  // Lists checked or unchecked here, by id: the wanted state.
  const [draft, setDraft] = useState<ReadonlyMap<string, boolean>>(new Map());
  const [search, setSearch] = useState("");
  const [newName, setNewName] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const lists = customLists.lists;
  const sorted = useMemo(
    () =>
      [...(lists ?? [])].sort(
        (a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: "base" }) || 0,
      ),
    [lists],
  );
  const query = search.trim().toLowerCase();
  const matching =
    query === "" ? sorted : sorted.filter((list) => list.name.toLowerCase().includes(query));
  const shown = matching.slice(0, SHOWN_LISTS);

  const isMember = (id: string) => draft.get(id) ?? initial?.has(id) ?? false;
  // Only lists that still exist; a list deleted meanwhile is dropped.
  const changes: MembershipChange[] = [...draft]
    .filter(([id, member]) => customLists.byId(id) && member !== (initial?.has(id) ?? false))
    .map(([listId, member]) => ({ listId, member }));
  const memberCount = sorted.filter((list) => isMember(list.id)).length;

  const toggle = (id: string, member: boolean) => {
    setDraft((current) => new Map(current).set(id, member));
  };

  const run = async (work: () => Promise<void>) => {
    setBusy(true);
    setError(null);
    try {
      await work();
    } catch (failure) {
      setError(describeError(failure));
    } finally {
      setBusy(false);
    }
  };

  const create = (event: SyntheticEvent) => {
    event.preventDefault();
    const name = newName.trim();
    if (name === "") return;
    const existing = sorted.find((list) => listNameKey(list.name) === listNameKey(name));
    if (existing) {
      setError(`A list named "${existing.name}" already exists.`);
      return;
    }
    void run(async () => {
      const created = await createList(name);
      customLists.changed();
      toggle(created.id, true);
      setNewName("");
    });
  };

  const save = () => {
    if (changes.length === 0) {
      onClose();
      return;
    }
    void run(async () => {
      try {
        await setEntryMemberships(entry, changes);
      } finally {
        // Saved or not, the lists may have changed (one may have been deleted).
        customLists.changed();
      }
      onClose();
    });
  };

  return (
    <Modal title="Manage Lists" onClose={onClose} size="narrow">
      <div className="manage-lists">
        <p className="manage-lists__target">
          <span className="manage-lists__title">{title}</span>{" "}
          <span className="code">{shortName(entry)}</span>
        </p>

        {memberships.state.status === "error" || customLists.loadError !== null ? (
          <div className="message message--error" role="alert">
            <p>
              The lists could not be read:{" "}
              {describeError(
                memberships.state.status === "error"
                  ? memberships.state.error
                  : customLists.loadError,
              )}
            </p>
            <button type="button" className="link-button" onClick={memberships.reload}>
              Retry
            </button>
          </div>
        ) : initial === null || lists === null ? (
          <p className="message message--muted" role="status">
            Loading lists…
          </p>
        ) : (
          <>
            <label className="field manage-lists__search" htmlFor={searchId}>
              <span className="field__label">Search lists</span>
              <input
                id={searchId}
                className="input"
                type="search"
                value={search}
                placeholder="List name"
                onChange={(event) => {
                  setSearch(event.target.value);
                }}
              />
            </label>
            <p className="message message--muted" role="status">
              In {memberCount} of {sorted.length} {sorted.length === 1 ? "list" : "lists"}
              {query !== "" && ` · ${String(matching.length)} matching`}
            </p>
            {sorted.length === 0 ? (
              <p className="message message--muted">No lists yet: create one below.</p>
            ) : (
              <ul className="manage-lists__options" aria-label="Lists">
                {shown.map((list) => (
                  <li key={list.id}>
                    <label className="manage-lists__option">
                      <input
                        type="checkbox"
                        checked={isMember(list.id)}
                        disabled={busy}
                        onChange={(event) => {
                          toggle(list.id, event.target.checked);
                        }}
                      />
                      <span className="manage-lists__name">{list.name}</span>
                      <span className="manage-lists__count">{list.machines}</span>
                    </label>
                  </li>
                ))}
                {matching.length === 0 && (
                  <li className="message message--muted">No list matches the search.</li>
                )}
                {matching.length > shown.length && (
                  <li className="message message--muted">
                    {matching.length - shown.length} more lists: refine the search to see them.
                  </li>
                )}
              </ul>
            )}

            <form className="manage-lists__create" onSubmit={create}>
              <input
                className="input"
                aria-label="New list name"
                placeholder="New list name"
                value={newName}
                disabled={busy}
                onChange={(event) => {
                  setNewName(event.target.value);
                }}
              />
              <button
                type="submit"
                className="button button--secondary"
                disabled={busy || newName.trim() === ""}
              >
                Create list
              </button>
            </form>
          </>
        )}

        {error && (
          <p className="message message--error" role="alert">
            {error}
          </p>
        )}

        <div className="actions manage-lists__actions">
          <button
            type="button"
            className="button"
            disabled={busy || initial === null}
            onClick={save}
          >
            Save
          </button>
          <button type="button" className="button button--secondary" onClick={onClose}>
            Cancel
          </button>
        </div>
      </div>
    </Modal>
  );
}
