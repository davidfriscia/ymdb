import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useReducer,
  useRef,
  type Dispatch,
} from "react";
import { CommandFailedError, describeError } from "../../shared/ipc";
import { useLoader, type Loader } from "../../shared/useLoader";
import { getCatalogStatus, type CatalogStatus } from "../catalog/catalogApi";
import { describeFreshness, formatCount } from "../catalog/catalogMessages";
import { searchMachines, searchSoftware, type FacetResult } from "../search/searchApi";
import {
  activeFilters,
  browserReducer,
  buildFacetRequest,
  buildResultsRequest,
  countActiveFilters,
  initialBrowserState,
  type BrowserAction,
  type BrowserRequest,
  type BrowserState,
  type DetailEntry,
  type DetailTarget,
} from "./browserState";
import { MachineDetailView } from "./detail/MachineDetailView";
import { SoftwareDetailView } from "./detail/SoftwareDetailView";
import { SoftwareListDetailView } from "./detail/SoftwareListDetailView";
import type { Scope } from "./filterDefinitions";
import { FilterPanel } from "./FilterPanel";
import { Pagination } from "./Pagination";
import type { BrowserPreferences } from "./browserPreferences";
import { rendererById } from "./renderers/registry";
import type { ResultItem } from "./renderers/types";
import { Toolbar } from "./Toolbar";

interface CatalogBrowserProps {
  /** Changes when the catalog or the MAME configuration may have changed. */
  refreshKey: number;
  preferences: BrowserPreferences;
  onPreferencesChange: (change: Partial<BrowserPreferences>) => void;
  /** Opens the MAME page, where catalogs are imported. */
  onOpenMame: () => void;
  /** Delay after the last keystroke before searching. */
  debounceMs?: number;
}

export function CatalogBrowser({
  refreshKey,
  preferences,
  onPreferencesChange,
  onOpenMame,
  debounceMs = 300,
}: CatalogBrowserProps) {
  const [state, dispatch] = useReducer(browserReducer, initialBrowserState);
  const { state: status, reload } = useLoader(getCatalogStatus, refreshKey);

  // Applies the search text once typing pauses.
  useEffect(() => {
    if (state.text === state.appliedText) return;
    const timer = window.setTimeout(() => {
      dispatch({ type: "applyText" });
    }, debounceMs);
    return () => {
      window.clearTimeout(timer);
    };
  }, [state.text, state.appliedText, debounceMs]);

  return (
    <section className="browser" aria-labelledby="browser-title">
      <h1 id="browser-title" className="visually-hidden">
        Catalog
      </h1>
      {status.status === "loading" && (
        <p className="message message--muted browser__state" role="status">
          Loading catalog…
        </p>
      )}
      {status.status === "error" && (
        <div className="state-message browser__state" role="alert">
          <p className="message message--error">
            The catalog status could not be read: {describeError(status.error)}
          </p>
          <button type="button" className="button button--secondary" onClick={reload}>
            Retry
          </button>
        </div>
      )}
      {status.status === "ready" && (
        <CatalogContent
          status={status.value}
          state={state}
          dispatch={dispatch}
          refreshKey={refreshKey}
          preferences={preferences}
          onPreferencesChange={onPreferencesChange}
          onOpenMame={onOpenMame}
          debounceMs={debounceMs}
        />
      )}
    </section>
  );
}

interface CatalogContentProps {
  status: CatalogStatus;
  state: BrowserState;
  dispatch: Dispatch<BrowserAction>;
  refreshKey: number;
  preferences: BrowserPreferences;
  onPreferencesChange: (change: Partial<BrowserPreferences>) => void;
  onOpenMame: () => void;
  debounceMs: number;
}

function CatalogContent({ status, onOpenMame, ...props }: CatalogContentProps) {
  if (status.catalogError) {
    return (
      <div className="state-message browser__state" role="alert">
        <p className="message message--error">
          The catalog cannot be read: {status.catalogError}. Import the catalog again.
        </p>
        <button type="button" className="button" onClick={onOpenMame}>
          Open MAME settings
        </button>
      </div>
    );
  }
  if (!status.catalog) {
    return (
      <div className="state-message browser__state">
        <h2 className="panel__title">No catalog yet</h2>
        <p className="message message--muted">
          No catalog has been imported yet. Configure MAME and import its machine catalog to browse
          it.
        </p>
        <button type="button" className="button" onClick={onOpenMame}>
          Open MAME settings
        </button>
      </div>
    );
  }
  const freshness = status.freshness ? describeFreshness(status.freshness) : null;
  return (
    <Workspace
      {...props}
      softwareImported={status.catalog.software !== null}
      freshnessWarning={freshness?.warning ? freshness.message : null}
      onOpenMame={onOpenMame}
    />
  );
}

interface SearchResult {
  scope: Scope;
  items: ResultItem[];
  meta: ResultMeta;
}

interface FacetsResult {
  scope: Scope;
  facets: FacetResult<string>[];
}

interface ResultMeta {
  total: number;
  page: number;
  pageSize: number;
  pageCount: number;
  elapsedMs: number;
}

async function runSearch({ scope, request }: BrowserRequest): Promise<SearchResult> {
  if (scope === "machines") {
    const response = await searchMachines(request);
    return {
      scope,
      items: response.items.map((machine) => ({ kind: "machine", key: machine.name, machine })),
      meta: response,
    };
  }
  const response = await searchSoftware(request);
  return {
    scope,
    items: response.items.map((software) => ({
      kind: "software",
      key: `${software.listName}/${software.name}`,
      software,
    })),
    meta: response,
  };
}

async function runFacetSearch({ scope, request }: BrowserRequest): Promise<FacetsResult> {
  const response =
    scope === "machines" ? await searchMachines(request) : await searchSoftware(request);
  return { scope, facets: response.facets };
}

function targetOf(item: ResultItem): DetailTarget {
  return item.kind === "machine"
    ? { kind: "machine", name: item.machine.name }
    : { kind: "software", list: item.software.listName, name: item.software.name };
}

interface WorkspaceProps extends Omit<CatalogContentProps, "status"> {
  softwareImported: boolean;
  freshnessWarning: string | null;
}

function Workspace({
  state,
  dispatch,
  refreshKey,
  preferences,
  onPreferencesChange,
  onOpenMame,
  debounceMs,
  softwareImported,
  freshnessWarning,
}: WorkspaceProps) {
  const { scope, appliedText, stack } = state;
  const query = state.queries[scope];
  const softwareUnavailable = scope === "software" && !softwareImported;

  // New request objects only when the query changes, so rendering never searches again.
  const request = useMemo(
    () => buildResultsRequest(scope, appliedText, query),
    [scope, appliedText, query],
  );
  // Facets do not depend on the page or the sort: changing them does not recount.
  const { filters, facetOptions } = query;
  const facetRequest = useMemo(
    () => buildFacetRequest(scope, appliedText, filters, facetOptions),
    [scope, appliedText, filters, facetOptions],
  );
  const load = useCallback(
    () => (softwareUnavailable ? Promise.resolve(null) : runSearch(request)),
    [request, softwareUnavailable],
  );
  const search = useLoader(load, refreshKey);
  const current = search.state.status === "ready" ? search.state.value : null;
  // While searching, keep showing the previous results of the same catalog type.
  const shown = current ?? (search.latest?.scope === scope ? search.latest : null);
  const loadFacets = useCallback(
    () => (softwareUnavailable ? Promise.resolve(null) : runFacetSearch(facetRequest)),
    [facetRequest, softwareUnavailable],
  );
  const facetSearch = useLoader(loadFacets, refreshKey);
  const currentFacets = facetSearch.state.status === "ready" ? facetSearch.state.value : null;
  const shownFacets =
    currentFacets ?? (facetSearch.latest?.scope === scope ? facetSearch.latest : null);

  const resultsRef = useRef<HTMLDivElement>(null);
  const savedScroll = useRef(0);
  const showingResults = stack.length === 0;

  useLayoutEffect(() => {
    if (showingResults && resultsRef.current) {
      resultsRef.current.scrollTop = savedScroll.current;
    }
  }, [showingResults]);

  const openFromResults = (index: number) => {
    const item = shown?.items[index];
    if (!item) return;
    savedScroll.current = resultsRef.current?.scrollTop ?? 0;
    dispatch({ type: "openDetail", target: targetOf(item), resultIndex: index });
  };

  const renderer = rendererById(preferences.renderer);
  const filterCount = countActiveFilters(query.filters);
  const top = stack.at(-1);

  return (
    <>
      {freshnessWarning && showingResults && (
        <p className="message message--warning browser__banner" role="status">
          {freshnessWarning}
        </p>
      )}

      <div className="browser__view" hidden={!showingResults}>
        <Toolbar
          scope={scope}
          text={state.text}
          query={query}
          appliedText={appliedText}
          filterCount={filterCount}
          preferences={preferences}
          sizable={renderer.sizable}
          dispatch={dispatch}
          onPreferencesChange={onPreferencesChange}
        />

        <div
          className={
            preferences.filtersVisible ? "browser__body" : "browser__body browser__body--no-filters"
          }
        >
          {preferences.filtersVisible && !softwareUnavailable && (
            <FilterPanel
              scope={scope}
              query={query}
              facets={shownFacets?.facets ?? null}
              facetsError={
                facetSearch.state.status === "error" ? describeError(facetSearch.state.error) : null
              }
              onRetryFacets={facetSearch.reload}
              dispatch={dispatch}
              debounceMs={debounceMs}
            />
          )}

          <div
            ref={resultsRef}
            className="browser__results"
            aria-busy={search.state.status === "loading"}
          >
            <ActiveFilterBar scope={scope} state={state} dispatch={dispatch} />
            {softwareUnavailable ? (
              <div className="state-message">
                <p className="message message--muted">
                  The software lists have not been imported yet. Import them from the MAME page to
                  browse software.
                </p>
                <button type="button" className="button" onClick={onOpenMame}>
                  Open MAME settings
                </button>
              </div>
            ) : (
              <Results
                scope={scope}
                search={search}
                shown={shown}
                filterCount={filterCount}
                preferences={preferences}
                dispatch={dispatch}
                onOpen={openFromResults}
                onOpenMame={onOpenMame}
              />
            )}
          </div>
        </div>
      </div>

      {top && (
        <DetailNavigation
          entry={top}
          depth={stack.length}
          items={shown?.items ?? []}
          refreshKey={refreshKey}
          softwareImported={softwareImported}
          dispatch={dispatch}
        />
      )}
    </>
  );
}

function ActiveFilterBar({
  scope,
  state,
  dispatch,
}: {
  scope: Scope;
  state: BrowserState;
  dispatch: Dispatch<BrowserAction>;
}) {
  const active = activeFilters(scope, state.queries[scope].filters);
  if (active.length === 0) return null;
  return (
    <div className="active-filters" aria-label="Active filters" role="group">
      {active.map((filter) => (
        <span key={filter.key} className="chip">
          {filter.label}
          <button
            type="button"
            className="chip__remove"
            aria-label={`Remove filter ${filter.label}`}
            onClick={() => {
              dispatch(filter.remove);
            }}
          >
            ×
          </button>
        </span>
      ))}
      <button
        type="button"
        className="link-button"
        onClick={() => {
          dispatch({ type: "clearFilters" });
        }}
      >
        Clear all filters
      </button>
    </div>
  );
}

interface ResultsProps {
  scope: Scope;
  search: Loader<SearchResult | null>;
  shown: SearchResult | null;
  filterCount: number;
  preferences: BrowserPreferences;
  dispatch: Dispatch<BrowserAction>;
  onOpen: (index: number) => void;
  onOpenMame: () => void;
}

function Results({
  scope,
  search,
  shown,
  filterCount,
  preferences,
  dispatch,
  onOpen,
  onOpenMame,
}: ResultsProps) {
  const { state } = search;
  if (state.status === "error") {
    const missingSoftware =
      state.error instanceof CommandFailedError &&
      state.error.detail.kind === "softwareListsNotImported";
    return (
      <div className="state-message" role="alert">
        <p className="message message--error">
          {missingSoftware
            ? "The software lists have not been imported yet."
            : `The search failed: ${describeError(state.error)}`}
        </p>
        {missingSoftware ? (
          <button type="button" className="button" onClick={onOpenMame}>
            Open MAME settings
          </button>
        ) : (
          <button type="button" className="button button--secondary" onClick={search.reload}>
            Retry
          </button>
        )}
      </div>
    );
  }
  if (!shown) {
    return (
      <p className="message message--muted" role="status">
        Searching…
      </p>
    );
  }
  const { meta, items } = shown;
  const Renderer = rendererById(preferences.renderer).Component;
  const first = meta.page * meta.pageSize + 1;
  const kind = scope === "machines" ? "machines" : "software";

  return (
    <>
      <p className="results__summary" role="status">
        {meta.total === 0
          ? `No ${kind} found`
          : items.length === 0
            ? `${formatCount(meta.total)} ${kind}`
            : `${formatCount(first)}–${formatCount(first + items.length - 1)} of ${formatCount(meta.total)} ${kind}`}
        <span className="results__timing"> · {meta.elapsedMs.toFixed(1)} ms</span>
        {state.status === "loading" && <span className="results__busy"> · Searching…</span>}
      </p>

      {meta.total === 0 && (
        <div className="state-message">
          <p className="message message--muted">No results match the search and filters.</p>
          {filterCount > 0 && (
            <button
              type="button"
              className="button button--secondary"
              onClick={() => {
                dispatch({ type: "clearFilters" });
              }}
            >
              Clear all filters
            </button>
          )}
        </div>
      )}

      {meta.total > 0 && items.length === 0 && (
        <div className="state-message">
          <p className="message message--muted">This page is past the last page of results.</p>
          <button
            type="button"
            className="button button--secondary"
            onClick={() => {
              dispatch({ type: "setPage", page: meta.pageCount - 1 });
            }}
          >
            Go to the last page
          </button>
        </div>
      )}

      <div className={state.status === "loading" ? "results results--stale" : "results"}>
        <Renderer items={items} itemSize={preferences.cardSize} onOpen={onOpen} />
      </div>

      {meta.pageCount > 1 && (
        <Pagination
          page={meta.page}
          pageCount={meta.pageCount}
          pageSize={meta.pageSize}
          onPage={(page) => {
            dispatch({ type: "setPage", page });
          }}
          onPageSize={(pageSize) => {
            dispatch({ type: "setPageSize", pageSize });
          }}
        />
      )}
    </>
  );
}

interface DetailNavigationProps {
  entry: DetailEntry;
  depth: number;
  items: ResultItem[];
  refreshKey: number;
  softwareImported: boolean;
  dispatch: Dispatch<BrowserAction>;
}

function DetailNavigation({
  entry,
  depth,
  items,
  refreshKey,
  softwareImported,
  dispatch,
}: DetailNavigationProps) {
  const { target, resultIndex, tab } = entry;
  const neighbor = (offset: number) => {
    if (resultIndex === null) return null;
    const item = items[resultIndex + offset];
    return item ? { target: targetOf(item), resultIndex: resultIndex + offset } : null;
  };
  const previous = neighbor(-1);
  const next = neighbor(1);
  const onOpen = (opened: DetailTarget) => {
    dispatch({ type: "openDetail", target: opened, resultIndex: null });
  };
  const onTab = (selected: string) => {
    dispatch({ type: "setDetailTab", tab: selected });
  };
  const key =
    target.kind === "software"
      ? `software:${target.list}/${target.name}`
      : `${target.kind}:${target.name}`;

  return (
    <div className="browser__detail">
      <nav className="detail-nav" aria-label="Detail navigation">
        <button
          type="button"
          className="button button--secondary"
          onClick={() => {
            dispatch({ type: "back" });
          }}
        >
          {depth > 1 ? "Back" : "Back to results"}
        </button>
        {depth > 1 && (
          <button
            type="button"
            className="button button--secondary"
            onClick={() => {
              dispatch({ type: "backToResults" });
            }}
          >
            Back to results
          </button>
        )}
        {resultIndex !== null && (
          <span className="detail-nav__step">
            <button
              type="button"
              className="button button--secondary"
              disabled={!previous}
              onClick={() => {
                if (previous) dispatch({ type: "replaceDetail", ...previous });
              }}
            >
              Previous
            </button>
            <span className="message--muted">
              {resultIndex + 1} / {items.length}
            </span>
            <button
              type="button"
              className="button button--secondary"
              disabled={!next}
              onClick={() => {
                if (next) dispatch({ type: "replaceDetail", ...next });
              }}
            >
              Next
            </button>
          </span>
        )}
      </nav>

      {target.kind === "machine" ? (
        <MachineDetailView
          key={key}
          name={target.name}
          tab={tab}
          refreshKey={refreshKey}
          onTab={onTab}
          onOpen={onOpen}
        />
      ) : target.kind === "software" ? (
        <SoftwareDetailView
          key={key}
          list={target.list}
          name={target.name}
          tab={tab}
          refreshKey={refreshKey}
          onTab={onTab}
          onOpen={onOpen}
        />
      ) : (
        <SoftwareListDetailView
          key={key}
          name={target.name}
          tab={tab}
          refreshKey={refreshKey}
          softwareImported={softwareImported}
          onTab={onTab}
          onOpen={onOpen}
        />
      )}
    </div>
  );
}
