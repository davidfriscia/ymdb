import { useCallback, useEffect, useId, useMemo, useState, type SyntheticEvent } from "react";
import { describeError } from "../../shared/ipc";
import { TwoPaneLayout, TwoPaneNavItem } from "../../shared/TwoPaneLayout";
import { useLoader } from "../../shared/useLoader";
import { Pagination } from "../browser/Pagination";
import { formatCount } from "../catalog/catalogMessages";
import { useInstallations } from "../installations/installationsContext";
import { searchMachines } from "../search/searchApi";
import { useCustomLists } from "./customListsContext";
import { FolderImportDialog } from "./FolderImportDialog";
import {
  chooseFolderImportFile,
  createList,
  deleteList,
  getListEntries,
  listNameKey,
  renameList,
  setEntryMemberships,
  type FolderFileSummary,
  type ListSummary,
} from "./listsApi";

type ListSort = "name" | "machines" | "updated" | "created";

const SORTS: { id: ListSort; label: string }[] = [
  { id: "name", label: "Name" },
  { id: "machines", label: "Most machines" },
  { id: "updated", label: "Recently changed" },
  { id: "created", label: "Recently created" },
];

const ENTRY_PAGE_SIZE = 50;
/** Pause after typing a search before it is sent. */
const SEARCH_DELAY_MS = 250;

function plural(count: number, word: string): string {
  return `${formatCount(count)} ${count === 1 ? word : `${word}s`}`;
}

function formatDate(milliseconds: number | null): string | null {
  return milliseconds === null ? null : new Date(milliseconds).toLocaleString();
}

function sortLists(lists: readonly ListSummary[], sort: ListSort): ListSummary[] {
  const byName = (a: ListSummary, b: ListSummary) =>
    a.name.localeCompare(b.name, undefined, { sensitivity: "base" });
  const compare: Record<ListSort, (a: ListSummary, b: ListSummary) => number> = {
    name: byName,
    machines: (a, b) => b.machines - a.machines || byName(a, b),
    updated: (a, b) => (b.updatedAt ?? 0) - (a.updatedAt ?? 0) || byName(a, b),
    created: (a, b) => (b.createdAt ?? 0) - (a.createdAt ?? 0) || byName(a, b),
  };
  return [...lists].sort(compare[sort]);
}

/** The value after a pause without changes. */
function useDebounced(value: string, delay = SEARCH_DELAY_MS): string {
  const [debounced, setDebounced] = useState(value);
  useEffect(() => {
    const timer = setTimeout(() => {
      setDebounced(value);
    }, delay);
    return () => {
      clearTimeout(timer);
    };
  }, [value, delay]);
  return debounced;
}

/**
 * The Lists page: the custom lists, searchable and sortable, with their
 * management (create, import, rename, delete) and their machines. Lists are
 * global: the active MAME installation only gives the descriptions and the
 * catalog search used to add machines. Favorites is managed by its bookmark.
 */
export function ListsPage() {
  const customLists = useCustomLists();
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [search, setSearch] = useState("");
  const [sort, setSort] = useState<ListSort>("name");
  const [creating, setCreating] = useState(false);
  const [importFile, setImportFile] = useState<FolderFileSummary | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const searchId = useId();

  const lists = customLists.lists;
  const selected = selectedId === null ? undefined : customLists.byId(selectedId);
  const sorted = useMemo(() => sortLists(lists ?? [], sort), [lists, sort]);
  const query = search.trim().toLowerCase();
  const shown =
    query === "" ? sorted : sorted.filter((list) => list.name.toLowerCase().includes(query));

  const chooseImport = () => {
    setError(null);
    setNotice(null);
    chooseFolderImportFile().then(
      (file) => {
        setImportFile(file);
      },
      (failure: unknown) => {
        setError(`The file cannot be imported: ${describeError(failure)}`);
      },
    );
  };

  const navigation = (
    <div className="lists-nav">
      <div className="actions lists-nav__actions">
        <button
          type="button"
          className="button button--small"
          onClick={() => {
            setCreating(true);
            setSelectedId(null);
          }}
        >
          New list
        </button>
        <button
          type="button"
          className="button button--secondary button--small"
          onClick={chooseImport}
        >
          Import INI…
        </button>
      </div>
      <input
        id={searchId}
        className="input"
        type="search"
        aria-label="Search lists"
        placeholder="Search lists"
        value={search}
        onChange={(event) => {
          setSearch(event.target.value);
        }}
      />
      <label className="lists-nav__sort">
        <span className="field__label">Sort by</span>
        <select
          className="input"
          value={sort}
          onChange={(event) => {
            setSort(event.target.value as ListSort);
          }}
        >
          {SORTS.map((option) => (
            <option key={option.id} value={option.id}>
              {option.label}
            </option>
          ))}
        </select>
      </label>
      {lists === null ? (
        <p className="two-pane__note">
          {customLists.loadError !== null
            ? `The lists cannot be read: ${describeError(customLists.loadError)}`
            : "Loading…"}
        </p>
      ) : (
        <>
          <p className="two-pane__note" role="status">
            {query === ""
              ? plural(lists.length, "list")
              : `${formatCount(shown.length)} of ${plural(lists.length, "list")}`}
          </p>
          <ul className="two-pane__list lists-nav__items" aria-label="Lists">
            {shown.map((list) => (
              <li key={list.id}>
                <TwoPaneNavItem
                  selected={list.id === selectedId}
                  onSelect={() => {
                    setSelectedId(list.id);
                    setCreating(false);
                  }}
                >
                  <span className="lists-nav__name">{list.name}</span>
                  <span className="lists-nav__count">{formatCount(list.machines)}</span>
                </TwoPaneNavItem>
              </li>
            ))}
          </ul>
        </>
      )}
    </div>
  );

  return (
    <TwoPaneLayout
      title="Lists"
      navigationLabel="Lists"
      navigation={navigation}
      heading={creating ? "New list" : (selected?.name ?? "Lists")}
      selectionKey={creating ? "new" : (selected?.id ?? "")}
    >
      {error && (
        <p className="message message--error" role="alert">
          {error}
        </p>
      )}
      {notice && (
        <p className="message message--success" role="status">
          {notice}
        </p>
      )}
      {creating ? (
        <CreateListForm
          lists={lists ?? []}
          onCreated={(list) => {
            customLists.changed();
            setCreating(false);
            setSelectedId(list.id);
            setNotice(`The list "${list.name}" was created.`);
          }}
          onCancel={() => {
            setCreating(false);
          }}
        />
      ) : selected ? (
        <ListDetail
          key={selected.id}
          list={selected}
          lists={lists ?? []}
          onDeleted={() => {
            setSelectedId(null);
            setNotice(`The list "${selected.name}" was deleted.`);
          }}
        />
      ) : (
        <div className="lists-empty">
          <p className="message message--muted">
            Lists organize MAME machines in collections of your own, for every MAME version: a
            machine can be in several lists, and machines missing from a catalog are kept. Choose a
            list to see its machines, create one, or import a MAME folder file (.ini) such as{" "}
            <span className="code">bootlegs.ini</span> or <span className="code">category.ini</span>
            .
          </p>
          <p className="message message--muted">
            In the catalog, View Options › Lists shows the machines of one list, and the list button
            of each machine adds it to lists.
          </p>
        </div>
      )}
      {importFile && (
        <FolderImportDialog
          file={importFile}
          onClose={() => {
            setImportFile(null);
          }}
          onImported={(outcome) => {
            customLists.changed();
            setImportFile(null);
            setNotice(
              `Imported ${importFile.fileName}: ${plural(outcome.created, "list")} created, ${plural(outcome.merged, "list")} updated, ${plural(outcome.entriesAdded, "machine")} added.`,
            );
          }}
        />
      )}
    </TwoPaneLayout>
  );
}

interface CreateListFormProps {
  lists: readonly ListSummary[];
  onCreated: (list: ListSummary) => void;
  onCancel: () => void;
}

function CreateListForm({ lists, onCreated, onCancel }: CreateListFormProps) {
  const id = useId();
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const existing = lists.find((list) => listNameKey(list.name) === listNameKey(name));

  const submit = (event: SyntheticEvent) => {
    event.preventDefault();
    if (name.trim() === "" || existing) return;
    setBusy(true);
    setError(null);
    createList(name).then(onCreated, (failure: unknown) => {
      setError(describeError(failure));
      setBusy(false);
    });
  };

  return (
    <form className="inline-form" onSubmit={submit}>
      <label className="field" htmlFor={id}>
        <span className="field__label">Name</span>
        <input
          id={id}
          className="input"
          value={name}
          disabled={busy}
          onChange={(event) => {
            setName(event.target.value);
          }}
        />
      </label>
      {existing && (
        <p className="message message--warning" role="status">
          A list named "{existing.name}" already exists (names ignore case).
        </p>
      )}
      <p className="message message--muted">A new list is empty; add machines from the catalog.</p>
      {error && (
        <p className="message message--error" role="alert">
          {error}
        </p>
      )}
      <div className="actions">
        <button
          type="submit"
          className="button"
          disabled={busy || name.trim() === "" || existing !== undefined}
        >
          Create list
        </button>
        <button type="button" className="button button--secondary" onClick={onCancel}>
          Cancel
        </button>
      </div>
    </form>
  );
}

interface ListDetailProps {
  list: ListSummary;
  lists: readonly ListSummary[];
  onDeleted: () => void;
}

function ListDetail({ list, lists, onDeleted }: ListDetailProps) {
  const customLists = useCustomLists();
  const { activeId } = useInstallations();
  const [renaming, setRenaming] = useState<string | null>(null);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [search, setSearch] = useState("");
  const [page, setPage] = useState(0);
  const [pageSize, setPageSize] = useState(ENTRY_PAGE_SIZE);
  const appliedSearch = useDebounced(search);

  const loadEntries = useCallback(
    () => getListEntries(list.id, activeId, appliedSearch, page, pageSize),
    [list.id, activeId, appliedSearch, page, pageSize],
  );
  // Reloaded after every saved change of a list (an entry removed here, a
  // machine added from the catalog or the Manage Lists window).
  const entries = useLoader(loadEntries, customLists.revision);
  const shown = entries.state.status === "ready" ? entries.state.value : entries.latest;
  const pageCount = shown ? Math.ceil(shown.matched / shown.pageSize) : 0;
  // Removing the last entry of the last page: show the new last page
  // (adjusted while rendering, as React recommends for derived state).
  if (shown && page > 0 && page >= pageCount) setPage(Math.max(0, pageCount - 1));

  const run = (work: () => Promise<void>) => {
    setBusy(true);
    setError(null);
    work()
      .catch((failure: unknown) => {
        setError(describeError(failure));
      })
      .finally(() => {
        setBusy(false);
      });
  };

  const conflict =
    renaming === null
      ? undefined
      : lists.find(
          (other) => other.id !== list.id && listNameKey(other.name) === listNameKey(renaming),
        );
  const rename = (event: SyntheticEvent) => {
    event.preventDefault();
    if (renaming === null || renaming.trim() === "" || conflict) return;
    const name = renaming;
    run(async () => {
      await renameList(list.id, name);
      customLists.changed();
      setRenaming(null);
    });
  };

  const remove = (name: string) => {
    run(async () => {
      await setEntryMemberships({ kind: "machine", name }, [{ listId: list.id, member: false }]);
      customLists.changed();
    });
  };

  const software = list.entries - list.machines;

  return (
    <div className="list-detail">
      <p className="message message--muted list-detail__meta">
        {plural(list.machines, "machine")}
        {formatDate(list.createdAt) && ` · created ${formatDate(list.createdAt) ?? ""}`}
        {formatDate(list.updatedAt) && ` · changed ${formatDate(list.updatedAt) ?? ""}`}
      </p>

      {renaming !== null ? (
        <form className="inline-form" onSubmit={rename}>
          <label className="field">
            <span className="field__label">New name</span>
            <input
              className="input"
              value={renaming}
              disabled={busy}
              onChange={(event) => {
                setRenaming(event.target.value);
              }}
            />
          </label>
          {conflict && (
            <p className="message message--warning" role="status">
              A list named "{conflict.name}" already exists. Renaming never merges lists: choose
              another name.
            </p>
          )}
          <div className="actions">
            <button
              type="submit"
              className="button"
              disabled={busy || renaming.trim() === "" || conflict !== undefined}
            >
              Rename
            </button>
            <button
              type="button"
              className="button button--secondary"
              onClick={() => {
                setRenaming(null);
              }}
            >
              Cancel
            </button>
          </div>
        </form>
      ) : confirmDelete ? (
        <div className="confirm-box" role="alertdialog" aria-label="Delete list">
          <p className="message message--warning">
            Delete the list "{list.name}" and its {plural(list.entries, "entry")}? The machines, the
            catalogs, Favorites and the other lists are not changed. Saved filters using this list
            will report it as deleted.
          </p>
          <div className="actions">
            <button
              type="button"
              className="button"
              disabled={busy}
              onClick={() => {
                run(async () => {
                  await deleteList(list.id);
                  customLists.changed();
                  onDeleted();
                });
              }}
            >
              Delete list
            </button>
            <button
              type="button"
              className="button button--secondary"
              onClick={() => {
                setConfirmDelete(false);
              }}
            >
              Cancel
            </button>
          </div>
        </div>
      ) : (
        <div className="actions">
          <button
            type="button"
            className="button button--secondary"
            onClick={() => {
              setRenaming(list.name);
            }}
          >
            Rename…
          </button>
          <button
            type="button"
            className="button button--secondary"
            onClick={() => {
              setConfirmDelete(true);
            }}
          >
            Delete…
          </button>
        </div>
      )}
      {error && (
        <p className="message message--error" role="alert">
          {error}
        </p>
      )}

      <AddMachines listId={list.id} installationId={activeId} />

      <h3 className="panel__subtitle">Machines</h3>
      <input
        className="input list-detail__search"
        type="search"
        aria-label="Search the machines of the list"
        placeholder="Search by short name or description"
        value={search}
        onChange={(event) => {
          setSearch(event.target.value);
          setPage(0);
        }}
      />
      {software > 0 && (
        <p className="message message--muted">
          The list also has {plural(software, "software entry")}, kept but not shown here.
        </p>
      )}
      {entries.state.status === "error" ? (
        <div className="message message--error" role="alert">
          <p>The machines cannot be read: {describeError(entries.state.error)}</p>
          <button type="button" className="link-button" onClick={entries.reload}>
            Retry
          </button>
        </div>
      ) : shown === null ? (
        <p className="message message--muted" role="status">
          Loading…
        </p>
      ) : (
        <>
          <p className="message message--muted" role="status">
            {shown.matched === 0
              ? list.machines === 0
                ? "This list is empty."
                : "No machine of the list matches the search."
              : appliedSearch.trim() === ""
                ? plural(shown.matched, "machine")
                : `${plural(shown.matched, "machine")} match`}
            {shown.installationId === null &&
              " · No catalog for the active MAME version: machines are shown by short name."}
          </p>
          {shown.items.length > 0 && (
            <table className="list-entries" aria-busy={entries.state.status === "loading"}>
              <thead>
                <tr>
                  <th scope="col">Short name</th>
                  <th scope="col">Description</th>
                  <th scope="col">Catalog</th>
                  <th scope="col">
                    <span className="visually-hidden">Actions</span>
                  </th>
                </tr>
              </thead>
              <tbody>
                {shown.items.map((item) => (
                  <tr key={item.name}>
                    <td className="code">{item.name}</td>
                    <td>{item.description ?? "—"}</td>
                    <td>
                      {item.inCatalog === null ? (
                        "—"
                      ) : item.inCatalog ? (
                        <span className="badge badge--success">Present</span>
                      ) : (
                        <span className="badge badge--warning">Not in this catalog</span>
                      )}
                    </td>
                    <td>
                      <button
                        type="button"
                        className="link-button"
                        disabled={busy}
                        aria-label={`Remove ${item.name} from the list`}
                        onClick={() => {
                          remove(item.name);
                        }}
                      >
                        Remove
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
          {pageCount > 1 && (
            <Pagination
              page={Math.min(page, pageCount - 1)}
              pageCount={pageCount}
              pageSize={pageSize}
              onPage={setPage}
              onPageSize={(size) => {
                setPageSize(size);
                setPage(0);
              }}
            />
          )}
        </>
      )}
    </div>
  );
}

const ADD_RESULTS = 10;

/** Adds machines found in the catalog of the active MAME version. */
function AddMachines({
  listId,
  installationId,
}: {
  listId: string;
  installationId: string | null;
}) {
  const customLists = useCustomLists();
  const [text, setText] = useState("");
  const [added, setAdded] = useState<ReadonlySet<string>>(new Set());
  const [error, setError] = useState<string | null>(null);
  const applied = useDebounced(text).trim();

  const load = useCallback(async () => {
    if (installationId === null || applied === "") return null;
    const response = await searchMachines(installationId, {
      text: applied,
      pageSize: ADD_RESULTS,
    });
    return response;
  }, [installationId, applied]);
  const results = useLoader(load);
  const response = results.state.status === "ready" ? results.state.value : null;

  const add = (name: string) => {
    setError(null);
    setEntryMemberships({ kind: "machine", name }, [{ listId, member: true }]).then(
      () => {
        setAdded((current) => new Set(current).add(name));
        customLists.changed();
      },
      (failure: unknown) => {
        setError(describeError(failure));
      },
    );
  };

  return (
    <section className="list-add" aria-label="Add machines">
      <h3 className="panel__subtitle">Add machines</h3>
      {installationId === null ? (
        <p className="message message--muted">
          Register a MAME installation and import its catalog to add machines by search.
        </p>
      ) : (
        <>
          <input
            className="input list-add__search"
            type="search"
            aria-label="Search the catalog to add machines"
            placeholder="Search the catalog (name, description, manufacturer)"
            value={text}
            onChange={(event) => {
              setText(event.target.value);
            }}
          />
          {results.state.status === "error" && (
            <p className="message message--error" role="alert">
              {describeError(results.state.error)}
            </p>
          )}
          {error && (
            <p className="message message--error" role="alert">
              {error}
            </p>
          )}
          {response && (
            <ul className="list-add__results" aria-label="Catalog results">
              {response.items.length === 0 && (
                <li className="message message--muted">No machine matches.</li>
              )}
              {response.items.map((machine) => (
                <li key={machine.name} className="list-add__result">
                  <span className="list-add__title">
                    {machine.description} <span className="code">{machine.name}</span>
                  </span>
                  <button
                    type="button"
                    className="button button--secondary button--small"
                    disabled={added.has(machine.name)}
                    aria-label={`Add ${machine.name} to the list`}
                    onClick={() => {
                      add(machine.name);
                    }}
                  >
                    {added.has(machine.name) ? "Added" : "Add"}
                  </button>
                </li>
              ))}
              {response.total > response.items.length && (
                <li className="message message--muted">
                  {formatCount(response.total - response.items.length)} more: refine the search.
                </li>
              )}
            </ul>
          )}
        </>
      )}
    </section>
  );
}
