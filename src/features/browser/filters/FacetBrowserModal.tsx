import { useCallback, useEffect, useMemo, useState } from "react";
import { Modal } from "../../../shared/Modal";
import { describeError } from "../../../shared/ipc";
import { useLoader } from "../../../shared/useLoader";
import { formatCount } from "../../catalog/catalogMessages";
import type { FacetResult } from "../../search/searchApi";
import { COMPLETE_FACET_LIMIT } from "../browserState";
import {
  formatFacetValue,
  isCompleteFacet,
  type FacetFilterDefinition,
} from "../filterDefinitions";
import { CategoryTree } from "./CategoryTree";
import { buildCategoryTree, facetRows, matchesSearch } from "./facetValues";
import { FacetValueList } from "./FacetValueList";

interface FacetBrowserModalProps {
  definition: FacetFilterDefinition;
  /** The applied selection, the starting point of the draft. */
  selected: (string | null)[];
  /** Values of the facet for the current search and other filters. */
  load: (search: string) => Promise<FacetResult<string>>;
  onApply: (values: (string | null)[]) => void;
  onClose: () => void;
  debounceMs: number;
}

/**
 * Every value of a facet in a dialog. The selection made here is a draft:
 * Apply replaces the facet's selection at once, Cancel (or Escape) leaves the
 * filters untouched. Searching filters the values, never the selection.
 */
export function FacetBrowserModal({
  definition,
  selected,
  load,
  onApply,
  onClose,
  debounceMs,
}: FacetBrowserModalProps) {
  const [draft, setDraft] = useState<(string | null)[]>(selected);
  const [search, setSearch] = useState("");
  const [appliedSearch, setAppliedSearch] = useState("");
  const complete = isCompleteFacet(definition);

  // Complete facets are loaded once and searched here; lists are searched
  // by the backend, which knows every value.
  useEffect(() => {
    if (complete) return;
    const timer = window.setTimeout(() => {
      setAppliedSearch(search);
    }, debounceMs);
    return () => {
      window.clearTimeout(timer);
    };
  }, [search, complete, debounceMs]);
  const loadValues = useCallback(() => load(appliedSearch), [load, appliedSearch]);
  const { state, latest, reload } = useLoader(loadValues, 0);
  const result = state.status === "ready" ? state.value : latest;

  const toggle = (value: string | null) => {
    setDraft((current) =>
      current.includes(value) ? current.filter((entry) => entry !== value) : [...current, value],
    );
  };

  const rows = useMemo(() => {
    if (!result || definition.presentation === "tree") return [];
    // Alphabetical here: the panel lists values by count.
    const values = [...result.values].sort((a, b) =>
      formatFacetValue(definition, a.value).localeCompare(formatFacetValue(definition, b.value)),
    );
    const ordered = definition.presentation === "version" ? result.values : values;
    return facetRows(definition, ordered, draft).filter(
      (row) =>
        draft.includes(row.value) || matchesSearch(formatFacetValue(definition, row.value), search),
    );
  }, [result, definition, draft, search]);
  const roots = useMemo(
    () =>
      result && definition.presentation === "tree" ? buildCategoryTree(result.values, draft) : [],
    [result, definition.presentation, draft],
  );

  return (
    <Modal
      title={definition.label}
      onClose={onClose}
      size="narrow"
      closeLabel="Close without applying"
    >
      <div className="facet-browser">
        <div className="facet-browser__header">
          <input
            type="search"
            className="input"
            aria-label={`Search ${definition.label} values`}
            placeholder="Search values"
            value={search}
            onChange={(event) => {
              setSearch(event.target.value);
            }}
          />
          <p className="facet-browser__status" role="status">
            {formatCount(draft.length)} selected
            {draft.length > 0 && (
              <>
                {" · "}
                <button
                  type="button"
                  className="link-button"
                  onClick={() => {
                    setDraft([]);
                  }}
                >
                  Clear selection
                </button>
              </>
            )}
          </p>
        </div>

        {state.status === "error" && (
          <div className="state-message" role="alert">
            <p className="message message--error">
              The values could not be loaded: {describeError(state.error)}
            </p>
            <button type="button" className="button button--secondary" onClick={reload}>
              Retry
            </button>
          </div>
        )}
        {!result && state.status !== "error" && <p className="facet__empty">Loading…</p>}
        {result &&
          (definition.presentation === "tree" ? (
            <CategoryTree
              label={definition.label}
              roots={roots}
              selected={draft}
              onToggle={toggle}
              search={search}
              scroll={false}
            />
          ) : rows.length === 0 ? (
            <p className="facet__empty">No matching values</p>
          ) : (
            <FacetValueList
              definition={definition}
              rows={rows}
              selected={draft}
              onToggle={toggle}
              scroll={false}
            />
          ))}
        {result?.hasMore && (
          <p className="facet__empty">
            Only the first {formatCount(COMPLETE_FACET_LIMIT)} values are listed: search to find
            others.
          </p>
        )}

        <div className="actions facet-browser__actions">
          <button
            type="button"
            className="button"
            onClick={() => {
              onApply(draft);
              onClose();
            }}
          >
            Apply
          </button>
          <button type="button" className="button button--secondary" onClick={onClose}>
            Cancel
          </button>
        </div>
      </div>
    </Modal>
  );
}
