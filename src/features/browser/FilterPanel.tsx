import { useEffect, useState, type Dispatch } from "react";
import { formatCount } from "../catalog/catalogMessages";
import type { FacetResult } from "../search/searchApi";
import {
  FACET_LIMIT_MAX,
  facetOptionsOf,
  type BrowserAction,
  type ScopeQuery,
} from "./browserState";
import {
  FACET_FILTERS,
  FLAG_FILTERS,
  formatFacetValue,
  type FacetFilterDefinition,
  type Scope,
} from "./filterDefinitions";

interface FilterPanelProps {
  scope: Scope;
  query: ScopeQuery;
  /** Facets of the last response for this scope; `null` while unavailable. */
  facets: FacetResult<string>[] | null;
  facetsError: string | null;
  onRetryFacets: () => void;
  dispatch: Dispatch<BrowserAction>;
  debounceMs: number;
}

export function FilterPanel({
  scope,
  query,
  facets,
  facetsError,
  onRetryFacets,
  dispatch,
  debounceMs,
}: FilterPanelProps) {
  return (
    <aside className="filter-panel" aria-label="Filters">
      {facetsError && (
        <div className="state-message" role="alert">
          <p className="message message--error">Filter values could not be loaded: {facetsError}</p>
          <button type="button" className="button button--secondary" onClick={onRetryFacets}>
            Retry
          </button>
        </div>
      )}
      {FLAG_FILTERS[scope].map((definition) => (
        <label key={definition.id} className="filter-flag">
          <span className="filter-flag__label">{definition.label}</span>
          <select
            className="input"
            value={query.filters.flags[definition.id] ?? ""}
            onChange={(event) => {
              dispatch({
                type: "setFlag",
                flag: definition.id,
                value: event.target.value === "" ? null : event.target.value,
              });
            }}
          >
            <option value="">Any</option>
            {definition.options.map((option) => (
              <option key={option.value} value={option.value}>
                {option.label}
              </option>
            ))}
          </select>
        </label>
      ))}

      {FACET_FILTERS[scope].map((definition) => (
        <FacetSection
          key={`${scope}:${definition.facet}`}
          definition={definition}
          query={query}
          result={facets?.find((facet) => facet.facet === definition.facet) ?? null}
          dispatch={dispatch}
          debounceMs={debounceMs}
        />
      ))}
    </aside>
  );
}

interface FacetSectionProps {
  definition: FacetFilterDefinition;
  query: ScopeQuery;
  result: FacetResult<string> | null;
  dispatch: Dispatch<BrowserAction>;
  debounceMs: number;
}

function FacetSection({ definition, query, result, dispatch, debounceMs }: FacetSectionProps) {
  const { facet, label } = definition;
  const options = facetOptionsOf(query, facet);
  const selected = query.filters.values[facet] ?? [];
  const [search, setSearch] = useState(options.search);

  useEffect(() => {
    if (search === options.search) return;
    const timer = window.setTimeout(() => {
      dispatch({ type: "setFacetSearch", facet, search });
    }, debounceMs);
    return () => {
      window.clearTimeout(timer);
    };
  }, [search, options.search, facet, dispatch, debounceMs]);

  const values = result?.values ?? [];
  // Selected values can fall outside the returned values (limit or value search).
  const missing = selected.filter((value) => !values.some((entry) => entry.value === value));
  const rows = [
    ...missing.map((value) => ({ value, count: null as number | null })),
    ...values.filter((entry) => definition.nullable || entry.value !== null),
  ];
  const searchable = (result?.hasMore ?? false) || options.search !== "" || values.length > 8;
  const id = `facet-${facet}`;

  return (
    <fieldset className="facet" aria-labelledby={`${id}-title`}>
      <legend id={`${id}-title`} className="facet__title">
        {label}
        {selected.length > 0 && (
          <button
            type="button"
            className="link-button facet__clear"
            aria-label={`Clear ${label} filter`}
            onClick={() => {
              dispatch({ type: "clearFacet", facet });
            }}
          >
            Clear
          </button>
        )}
      </legend>
      {searchable && (
        <input
          type="search"
          className="input facet__search"
          aria-label={`Search ${label} values`}
          placeholder="Search values"
          value={search}
          onChange={(event) => {
            setSearch(event.target.value);
          }}
        />
      )}
      {result === null && <p className="facet__empty">Loading…</p>}
      {result !== null && rows.length === 0 && <p className="facet__empty">No values</p>}
      <ul className="facet__values">
        {rows.map(({ value, count }) => {
          const text = formatFacetValue(definition, value);
          return (
            <li key={value ?? "\u0000"}>
              <label className="facet__value">
                <input
                  type="checkbox"
                  checked={selected.includes(value)}
                  onChange={() => {
                    dispatch({ type: "toggleValue", facet, value });
                  }}
                />
                <span
                  className={value === null ? "facet__label facet__label--empty" : "facet__label"}
                >
                  {text}
                </span>
                <span className="facet__count">{count === null ? "—" : formatCount(count)}</span>
              </label>
            </li>
          );
        })}
      </ul>
      {result?.hasMore && options.limit < FACET_LIMIT_MAX && (
        <button
          type="button"
          className="link-button"
          onClick={() => {
            dispatch({ type: "showMoreFacetValues", facet });
          }}
        >
          Show more
        </button>
      )}
      {result?.hasMore && options.limit >= FACET_LIMIT_MAX && (
        <p className="facet__empty">Search to find more values.</p>
      )}
    </fieldset>
  );
}
