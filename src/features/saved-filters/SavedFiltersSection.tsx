import { useCallback, useState, type SyntheticEvent } from "react";
import { describeError } from "../../shared/ipc";
import { useLoader } from "../../shared/useLoader";
import type { BrowserCriteria } from "../browser/browserState";
import {
  CRITERIA_VERSION,
  encodeCriteria,
  parseCriteria,
  sameCriteria,
  type ParsedCriteria,
} from "./savedFilterCriteria";
import {
  createSavedFilter,
  deleteSavedFilter,
  getSavedFilters,
  renameSavedFilter,
  updateSavedFilter,
  type SavedFilter,
} from "./savedFiltersApi";

interface SavedFiltersSectionProps {
  /** The current search and filters of the browser. */
  criteria: BrowserCriteria;
  /** The preset loaded last, if any. */
  activeId: string | null;
  /** Applies a preset's criteria (all at once) and marks it as loaded. */
  onLoad: (id: string, criteria: BrowserCriteria, ignored: string[]) => void;
  /** The loaded preset changed without loading (saved, deleted). */
  onActiveChange: (id: string | null) => void;
}

type Editing = { kind: "rename"; id: string; name: string } | { kind: "delete"; id: string };

/**
 * Saved Filters in View Options: save the current search and filters under
 * a name, load, update, rename and delete presets. Presets are global (the
 * same for every MAME installation) and never change on their own: after
 * loading one, changing a filter marks it as modified until it is updated.
 */
export function SavedFiltersSection({
  criteria,
  activeId,
  onLoad,
  onActiveChange,
}: SavedFiltersSectionProps) {
  const { state, reload } = useLoader(getSavedFilters, 0);
  // The list returned by the last change, else the loaded one.
  const [changed, setPresets] = useState<SavedFilter[] | null>(null);
  const presets = changed ?? (state.status === "ready" ? state.value : null);
  const [name, setName] = useState("");
  const [editing, setEditing] = useState<Editing | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const active = presets?.find((preset) => preset.id === activeId) ?? null;
  const parsedActive = active ? parseCriteria(active.criteriaVersion, active.criteria) : null;
  const modified =
    parsedActive?.kind === "ok" ? !sameCriteria(parsedActive.criteria, criteria) : false;

  const run = useCallback(async (work: () => Promise<void>) => {
    setBusy(true);
    setError(null);
    try {
      await work();
    } catch (failure) {
      setError(describeError(failure));
    } finally {
      setBusy(false);
    }
  }, []);

  const save = (event: SyntheticEvent) => {
    event.preventDefault();
    const trimmed = name.trim();
    if (trimmed === "") return;
    void run(async () => {
      const created = await createSavedFilter(trimmed, CRITERIA_VERSION, encodeCriteria(criteria));
      setPresets(created.presets);
      setName("");
      onActiveChange(created.id);
    });
  };

  const update = (preset: SavedFilter) => {
    void run(async () => {
      setPresets(await updateSavedFilter(preset.id, CRITERIA_VERSION, encodeCriteria(criteria)));
    });
  };

  const load = (preset: SavedFilter, parsed: ParsedCriteria) => {
    if (parsed.kind !== "ok") return;
    setError(null);
    onLoad(preset.id, parsed.criteria, parsed.ignored);
  };

  if (presets === null) {
    return (
      <fieldset className="view-panel__section">
        <legend className="view-panel__title">Saved Filters</legend>
        {state.status === "error" ? (
          <div role="alert">
            <p className="message message--error">
              The saved filters could not be loaded: {describeError(state.error)}
            </p>
            <button type="button" className="link-button" onClick={reload}>
              Retry
            </button>
          </div>
        ) : (
          <p className="message message--muted">Loading saved filters…</p>
        )}
      </fieldset>
    );
  }

  return (
    <fieldset className="view-panel__section saved-filters">
      <legend className="view-panel__title">Saved Filters</legend>

      {active && (
        <div className="saved-filters__current" role="status">
          <span>
            Loaded: <strong>{active.name}</strong>
            {modified && (
              <span className="badge badge--warning saved-filters__badge">Modified</span>
            )}
          </span>
          {modified && (
            <button
              type="button"
              className="button button--secondary button--small"
              disabled={busy}
              onClick={() => {
                update(active);
              }}
            >
              Update “{active.name}”
            </button>
          )}
        </div>
      )}

      {presets.length === 0 ? (
        <p className="message message--muted">
          No saved filters yet. Save the current search and filters to find them again with any MAME
          version.
        </p>
      ) : (
        <ul className="saved-filters__list" aria-label="Saved filters">
          {presets.map((preset) => {
            const parsed = parseCriteria(preset.criteriaVersion, preset.criteria);
            const current = preset.id === activeId;
            if (editing?.id === preset.id && editing.kind === "rename") {
              return (
                <li key={preset.id}>
                  <form
                    className="saved-filters__edit"
                    onSubmit={(event) => {
                      event.preventDefault();
                      void run(async () => {
                        setPresets(await renameSavedFilter(preset.id, editing.name.trim()));
                        setEditing(null);
                      });
                    }}
                  >
                    <input
                      className="input"
                      aria-label={`New name of ${preset.name}`}
                      value={editing.name}
                      maxLength={100}
                      autoFocus
                      onChange={(event) => {
                        setEditing({ ...editing, name: event.target.value });
                      }}
                    />
                    <button
                      type="submit"
                      className="button button--small"
                      disabled={busy || editing.name.trim() === ""}
                    >
                      Rename
                    </button>
                    <button
                      type="button"
                      className="button button--secondary button--small"
                      onClick={() => {
                        setEditing(null);
                      }}
                    >
                      Cancel
                    </button>
                  </form>
                </li>
              );
            }
            if (editing?.id === preset.id && editing.kind === "delete") {
              return (
                <li key={preset.id} className="saved-filters__confirm" role="group">
                  <span>Delete “{preset.name}”?</span>
                  <button
                    type="button"
                    className="button button--small"
                    disabled={busy}
                    onClick={() => {
                      void run(async () => {
                        setPresets(await deleteSavedFilter(preset.id));
                        setEditing(null);
                        if (current) onActiveChange(null);
                      });
                    }}
                  >
                    Delete
                  </button>
                  <button
                    type="button"
                    className="button button--secondary button--small"
                    onClick={() => {
                      setEditing(null);
                    }}
                  >
                    Cancel
                  </button>
                </li>
              );
            }
            return (
              <li key={preset.id} className="saved-filters__item">
                <button
                  type="button"
                  className="saved-filters__load"
                  aria-current={current ? "true" : undefined}
                  aria-label={`Load ${preset.name}`}
                  title={
                    parsed.kind === "ok"
                      ? `Load “${preset.name}”`
                      : "This saved filter cannot be loaded by this version of YMDB"
                  }
                  disabled={parsed.kind !== "ok"}
                  onClick={() => {
                    load(preset, parsed);
                  }}
                >
                  <span className="saved-filters__name">{preset.name}</span>
                  {parsed.kind === "ok" && (
                    <span className="saved-filters__scope">
                      {parsed.criteria.scope === "machines" ? "Machines" : "Software"}
                    </span>
                  )}
                </button>
                {parsed.kind !== "ok" && (
                  <span className="saved-filters__note">
                    {parsed.kind === "unsupported"
                      ? `Saved by a newer YMDB (format ${String(parsed.version)}); kept unchanged.`
                      : `Unreadable: ${parsed.message}.`}
                  </span>
                )}
                <span className="saved-filters__actions">
                  <button
                    type="button"
                    className="link-button"
                    aria-label={`Rename ${preset.name}`}
                    onClick={() => {
                      setEditing({ kind: "rename", id: preset.id, name: preset.name });
                    }}
                  >
                    Rename
                  </button>
                  <button
                    type="button"
                    className="link-button"
                    aria-label={`Delete ${preset.name}`}
                    onClick={() => {
                      setEditing({ kind: "delete", id: preset.id });
                    }}
                  >
                    Delete
                  </button>
                </span>
              </li>
            );
          })}
        </ul>
      )}

      <form className="saved-filters__save" onSubmit={save}>
        <input
          className="input"
          aria-label="Saved filter name"
          placeholder="Name"
          value={name}
          maxLength={100}
          onChange={(event) => {
            setName(event.target.value);
          }}
        />
        <button
          type="submit"
          className="button button--small"
          disabled={busy || name.trim() === ""}
        >
          Save current filters
        </button>
      </form>

      {error && (
        <p className="message message--error" role="alert">
          {error}
        </p>
      )}
    </fieldset>
  );
}
