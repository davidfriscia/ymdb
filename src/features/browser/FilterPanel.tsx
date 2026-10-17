import { useEffect, useMemo, useState, type Dispatch } from "react";
import { formatCount } from "../catalog/catalogMessages";
import type { FacetResult, VersionRange } from "../search/searchApi";
import type { Dimension } from "../support-files/supportFilesApi";
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
import { CategoryTree } from "./filters/CategoryTree";
import { FacetBrowserModal } from "./filters/FacetBrowserModal";
import { buildCategoryTree, facetRows, matchesSearch } from "./filters/facetValues";
import { FacetValueList } from "./filters/FacetValueList";

/** The classifications of the browsed catalog. */
export interface ClassificationsState {
  /** Dimensions with an active support file source; `null` while loading. */
  available: readonly Dimension[] | null;
  /** The catalog's classifications could not be brought up to date. */
  syncError: string | null;
  /** The status could not be read. */
  error: string | null;
  onRetry: () => void;
}

interface FilterPanelProps {
  scope: Scope;
  query: ScopeQuery;
  /** Facets of the last response for this scope; `null` while unavailable. */
  facets: FacetResult<string>[] | null;
  facetsError: string | null;
  onRetryFacets: () => void;
  dispatch: Dispatch<BrowserAction>;
  debounceMs: number;
  classifications: ClassificationsState;
  /** Values of one facet for the current search and filters, for "Browse all". */
  loadFacetValues: (facet: string, search: string) => Promise<FacetResult<string>>;
  onOpenSupportFiles: () => void;
}

/** Lists with more values than this get a value search and "Browse all". */
const SEARCHABLE_FROM = 8;

export function FilterPanel({
  scope,
  query,
  facets,
  facetsError,
  onRetryFacets,
  dispatch,
  debounceMs,
  classifications,
  loadFacetValues,
  onOpenSupportFiles,
}: FilterPanelProps) {
  const [browsing, setBrowsing] = useState<FacetFilterDefinition | null>(null);
  const available = classifications.available ?? [];
  const definitions = FACET_FILTERS[scope].filter(
    (definition) =>
      definition.classification === undefined || available.includes(definition.classification),
  );
  const resultOf = (definition: FacetFilterDefinition) =>
    facets?.find((facet) => facet.facet === definition.facet) ?? null;

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

      {scope === "machines" && (
        <ClassificationNotice
          classifications={classifications}
          onOpenSupportFiles={onOpenSupportFiles}
        />
      )}

      {definitions.map((definition) => {
        const common = {
          definition,
          query,
          result: resultOf(definition),
          dispatch,
          onBrowse: () => {
            setBrowsing(definition);
          },
        };
        const key = `${scope}:${definition.facet}`;
        if (definition.presentation === "tree") return <CategorySection key={key} {...common} />;
        if (definition.presentation === "version") return <VersionSection key={key} {...common} />;
        return <FacetSection key={key} {...common} debounceMs={debounceMs} />;
      })}

      {browsing && (
        <FacetBrowserModal
          definition={browsing}
          selected={query.filters.values[browsing.facet] ?? []}
          load={(search) => loadFacetValues(browsing.facet, search)}
          onApply={(values) => {
            dispatch({ type: "setFacetValues", facet: browsing.facet, values });
          }}
          onClose={() => {
            setBrowsing(null);
          }}
          debounceMs={debounceMs}
        />
      )}
    </aside>
  );
}

/** Why classification filters are missing, with a way to add them. */
function ClassificationNotice({
  classifications,
  onOpenSupportFiles,
}: {
  classifications: ClassificationsState;
  onOpenSupportFiles: () => void;
}) {
  const { available, syncError, error, onRetry } = classifications;
  if (error !== null || syncError !== null) {
    return (
      <div className="filter-notice" role="alert">
        <p className="message message--warning">
          {error !== null
            ? `The classifications could not be read: ${error}`
            : `The classifications of this catalog could not be updated: ${syncError ?? ""}. The previous ones are shown.`}
        </p>
        <button type="button" className="link-button" onClick={onRetry}>
          Retry
        </button>
      </div>
    );
  }
  if (available === null || available.length > 0) return null;
  return (
    <div className="filter-notice">
      <p className="message message--muted">
        Import MAME support files (catver.ini, genre.ini, catlist.ini, version.ini, series.ini) to
        filter machines by genre, category, series and version added.
      </p>
      <button type="button" className="link-button" onClick={onOpenSupportFiles}>
        Open Support Files settings
      </button>
    </div>
  );
}

interface SectionProps {
  definition: FacetFilterDefinition;
  query: ScopeQuery;
  result: FacetResult<string> | null;
  dispatch: Dispatch<BrowserAction>;
  onBrowse: () => void;
}

/** Title with the number of selected values and Clear. */
function FacetLegend({
  id,
  definition,
  count,
  dispatch,
}: {
  id: string;
  definition: FacetFilterDefinition;
  count: number;
  dispatch: Dispatch<BrowserAction>;
}) {
  const { facet, label } = definition;
  return (
    <legend id={id} className="facet__title">
      <span>
        {label}
        {count > 0 && (
          <>
            {" "}
            <span className="facet__selected">({formatCount(count)})</span>
          </>
        )}
      </span>
      {count > 0 && (
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
  );
}

function BrowseAllButton({ label, onBrowse }: { label: string; onBrowse: () => void }) {
  return (
    <button
      type="button"
      className="link-button"
      aria-label={`Browse all ${label} values`}
      onClick={onBrowse}
    >
      Browse all
    </button>
  );
}

/** A list facet: values searched by the backend, "Show more" and "Browse all". */
function FacetSection({
  definition,
  query,
  result,
  dispatch,
  onBrowse,
  debounceMs,
}: SectionProps & { debounceMs: number }) {
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
  const rows = facetRows(definition, values, selected);
  const searchable =
    (result?.hasMore ?? false) || options.search !== "" || values.length > SEARCHABLE_FROM;
  const id = `facet-${facet}`;

  return (
    <fieldset className="facet" aria-labelledby={`${id}-title`}>
      <FacetLegend
        id={`${id}-title`}
        definition={definition}
        count={selected.length}
        dispatch={dispatch}
      />
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
      <FacetValueList
        definition={definition}
        rows={rows}
        selected={selected}
        onToggle={(value) => {
          dispatch({ type: "toggleValue", facet, value });
        }}
      />
      <div className="facet__more">
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
        {searchable && <BrowseAllButton label={label} onBrowse={onBrowse} />}
      </div>
    </fieldset>
  );
}

/** Category: a tree of the paths in the data, searched here. */
function CategorySection({ definition, query, result, dispatch, onBrowse }: SectionProps) {
  const { facet, label } = definition;
  const selectedValues = query.filters.values[facet];
  const selected = useMemo(() => selectedValues ?? [], [selectedValues]);
  const [search, setSearch] = useState("");
  const values = result?.values;
  const roots = useMemo(() => buildCategoryTree(values ?? [], selected), [values, selected]);
  const id = `facet-${facet}`;

  return (
    <fieldset className="facet" aria-labelledby={`${id}-title`}>
      <FacetLegend
        id={`${id}-title`}
        definition={definition}
        count={selected.length}
        dispatch={dispatch}
      />
      <input
        type="search"
        className="input facet__search"
        aria-label={`Search ${label} values`}
        placeholder="Search categories"
        value={search}
        onChange={(event) => {
          setSearch(event.target.value);
        }}
      />
      {result === null ? (
        <p className="facet__empty">Loading…</p>
      ) : (
        <CategoryTree
          label={label}
          roots={roots}
          selected={selected}
          onToggle={(path) => {
            dispatch({ type: "toggleValue", facet, value: path });
          }}
          search={search}
        />
      )}
      <div className="facet__more">
        <BrowseAllButton label={label} onBrowse={onBrowse} />
      </div>
    </fieldset>
  );
}

/**
 * Version Added: a range in MAME version order (the backend orders the
 * values), and exact versions, newest first, searched here.
 */
function VersionSection({ definition, query, result, dispatch, onBrowse }: SectionProps) {
  const { facet, label } = definition;
  const selected = query.filters.values[facet] ?? [];
  const range = query.filters.ranges[facet];
  const [search, setSearch] = useState("");
  const versions = (result?.values ?? []).flatMap((entry) =>
    entry.value === null ? [] : [entry.value],
  );
  // Bounds of a saved range stay selectable even if this catalog lacks them.
  const options = [
    ...new Set([
      ...versions,
      ...[range?.from, range?.to].filter((v): v is string => v !== undefined),
    ]),
  ];
  const rows = facetRows(definition, [...(result?.values ?? [])].reverse(), selected).filter(
    (row) =>
      selected.includes(row.value) ||
      matchesSearch(formatFacetValue(definition, row.value), search),
  );
  const setBound = (bound: keyof VersionRange, value: string) => {
    dispatch({
      type: "setRange",
      facet,
      range: { ...range, [bound]: value === "" ? undefined : value },
    });
  };
  const id = `facet-${facet}`;

  return (
    <fieldset className="facet" aria-labelledby={`${id}-title`}>
      <FacetLegend
        id={`${id}-title`}
        definition={definition}
        count={selected.length + (range ? 1 : 0)}
        dispatch={dispatch}
      />
      <div className="facet-range">
        {(["from", "to"] as const).map((bound) => (
          <label key={bound} className="facet-range__bound">
            <span>{bound === "from" ? "From" : "To"}</span>
            <select
              className="input"
              aria-label={`${label} ${bound}`}
              value={range?.[bound] ?? ""}
              onChange={(event) => {
                setBound(bound, event.target.value);
              }}
            >
              <option value="">Any</option>
              {options.map((version) => (
                <option key={version} value={version}>
                  {version}
                </option>
              ))}
            </select>
          </label>
        ))}
      </div>
      <input
        type="search"
        className="input facet__search"
        aria-label={`Search ${label} values`}
        placeholder="Search versions"
        value={search}
        onChange={(event) => {
          setSearch(event.target.value);
        }}
      />
      {result === null && <p className="facet__empty">Loading…</p>}
      {result !== null && rows.length === 0 && <p className="facet__empty">No values</p>}
      <FacetValueList
        definition={definition}
        rows={rows}
        selected={selected}
        onToggle={(value) => {
          dispatch({ type: "toggleValue", facet, value });
        }}
      />
      <div className="facet__more">
        <BrowseAllButton label={label} onBrowse={onBrowse} />
      </div>
    </fieldset>
  );
}
