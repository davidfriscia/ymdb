import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useReducer,
  useRef,
  useState,
  type Dispatch,
} from "react";
import { CommandFailedError, describeError } from "../../shared/ipc";
import { useLoader, type Loader } from "../../shared/useLoader";
import { getCatalogStatus, type CatalogStatus } from "../catalog/catalogApi";
import { describeFreshness, formatCount } from "../catalog/catalogMessages";
import { InstallationSelect } from "../installations/InstallationSelect";
import { InstallationScope, useInstallations } from "../installations/installationsContext";
import { useFavorites } from "../lists/favoritesContext";
import { FAVORITES_LIST_ID } from "../lists/listsApi";
import { searchMachines, searchSoftware, type FacetResult } from "../search/searchApi";
import { SavedFiltersSection } from "../saved-filters/SavedFiltersSection";
import {
  DIMENSION_LABELS,
  findMissingClassificationValues,
  getClassificationStatus,
  type Dimension,
  type DimensionValues,
} from "../support-files/supportFilesApi";
import {
  activeFilters,
  browserReducer,
  buildFacetRequest,
  buildFacetValuesRequest,
  buildResultsRequest,
  countActiveFilters,
  initialBrowserState,
  type BrowserAction,
  type BrowserCriteria,
  type BrowserRequest,
  type BrowserState,
  type DetailEntry,
  type DetailTarget,
} from "./browserState";
import { MachineDetailView } from "./detail/MachineDetailView";
import { SoftwareDetailView } from "./detail/SoftwareDetailView";
import { SoftwareListDetailView } from "./detail/SoftwareListDetailView";
import type { DetailNavigation } from "./detail/DetailParts";
import { FACET_FILTERS, type Scope } from "./filterDefinitions";
import { FilterPanel, type ClassificationsState } from "./FilterPanel";
import { Pagination } from "./Pagination";
import type { BrowserPreferences } from "./browserPreferences";
import { rendererById } from "./renderers/registry";
import { ViewOptionsPanel } from "./ViewOptionsPanel";
import type { ResultItem } from "./renderers/types";
import { Toolbar } from "./Toolbar";

interface CatalogBrowserProps {
  /** Changes when a catalog may have changed (for example after an import). */
  refreshKey: number;
  preferences: BrowserPreferences;
  onPreferencesChange: (change: Partial<BrowserPreferences>) => void;
  /** Opens the MAME page, where installations are managed and catalogs imported. */
  onOpenMame: () => void;
  /** Opens Settings › Support Files, where classifications are imported. */
  onOpenSupportFiles: () => void;
  /** Delay after the last keystroke before searching. */
  debounceMs?: number;
}

/**
 * Browses the catalog of the active MAME installation. Every request names
 * that installation; switching to another one keeps the search text, filters
 * and sorting, returns to the first page and closes the detail views, which
 * belong to the previous catalog.
 */
export function CatalogBrowser({
  refreshKey,
  preferences,
  onPreferencesChange,
  onOpenMame,
  onOpenSupportFiles,
  debounceMs = 300,
}: CatalogBrowserProps) {
  const [state, dispatch] = useReducer(browserReducer, initialBrowserState);
  const installations = useInstallations();
  const installationId = installations.activeId;

  // Reset while rendering, so that a detail view of the previous catalog is
  // never rendered for the new installation.
  const [shownInstallation, setShownInstallation] = useState(installationId);
  if (shownInstallation !== installationId) {
    setShownInstallation(installationId);
    if (shownInstallation !== null) dispatch({ type: "installationChanged" });
  }

  const loadStatus = useCallback(
    () => (installationId === null ? Promise.resolve(null) : getCatalogStatus(installationId)),
    [installationId],
  );
  const { state: status, latest, reload } = useLoader(loadStatus, refreshKey);

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

  // While the status of a newly selected installation loads, the workspace
  // stays visible when that installation has a catalog: its searches already
  // name the new installation.
  const switching =
    status.status === "loading" &&
    installations.active?.catalogImported === true &&
    latest?.catalog != null;
  const current = status.status === "ready" ? status.value : switching ? latest : null;

  let content;
  if (installations.view === null) {
    content = installations.error ? (
      <div className="state-message browser__state" role="alert">
        <p className="message message--error">
          The MAME installations could not be read: {describeError(installations.error)}
        </p>
        <button
          type="button"
          className="button button--secondary"
          onClick={() => void installations.reload()}
        >
          Retry
        </button>
      </div>
    ) : (
      <p className="message message--muted browser__state" role="status">
        Loading catalog…
      </p>
    );
  } else if (installationId === null) {
    content = (
      <div className="state-message browser__state">
        <h2 className="panel__title">No MAME installation</h2>
        <p className="message message--muted">
          Add a MAME installation and import its catalog to browse it.
        </p>
        <button type="button" className="button" onClick={onOpenMame}>
          Open MAME settings
        </button>
      </div>
    );
  } else if (status.status === "error") {
    content = (
      <div className="state-message browser__state" role="alert">
        <p className="message message--error">
          The catalog status could not be read: {describeError(status.error)}
        </p>
        <button type="button" className="button button--secondary" onClick={reload}>
          Retry
        </button>
        <InstallationSelect onOpenMame={onOpenMame} />
      </div>
    );
  } else if (current === null) {
    content = (
      <p className="message message--muted browser__state" role="status">
        Loading catalog…
      </p>
    );
  } else {
    content = (
      <InstallationScope value={installationId}>
        <CatalogContent
          installationId={installationId}
          status={current}
          stale={status.status !== "ready"}
          state={state}
          dispatch={dispatch}
          refreshKey={refreshKey}
          preferences={preferences}
          onPreferencesChange={onPreferencesChange}
          onOpenMame={onOpenMame}
          onOpenSupportFiles={onOpenSupportFiles}
          debounceMs={debounceMs}
        />
      </InstallationScope>
    );
  }

  return (
    <section className="browser" aria-labelledby="browser-title">
      <h1 id="browser-title" className="visually-hidden">
        Catalog
      </h1>
      {content}
    </section>
  );
}

interface CatalogContentProps {
  installationId: string;
  status: CatalogStatus;
  /** The status is the previous installation's, while switching. */
  stale: boolean;
  state: BrowserState;
  dispatch: Dispatch<BrowserAction>;
  refreshKey: number;
  preferences: BrowserPreferences;
  onPreferencesChange: (change: Partial<BrowserPreferences>) => void;
  onOpenMame: () => void;
  onOpenSupportFiles: () => void;
  debounceMs: number;
}

function CatalogContent({ status, stale, onOpenMame, ...props }: CatalogContentProps) {
  if (status.catalogError) {
    return (
      <div className="state-message browser__state" role="alert">
        <p className="message message--error">
          The catalog cannot be read: {status.catalogError}. Import the catalog again.
        </p>
        <button type="button" className="button" onClick={onOpenMame}>
          Open MAME settings
        </button>
        <InstallationSelect onOpenMame={onOpenMame} />
      </div>
    );
  }
  if (!status.catalog) {
    return (
      <div className="state-message browser__state">
        <h2 className="panel__title">No catalog yet</h2>
        <p className="message message--muted">
          No catalog has been imported for this MAME installation yet. Import its machine catalog
          from the MAME page to browse it, or choose another MAME version.
        </p>
        <button type="button" className="button" onClick={onOpenMame}>
          Open MAME settings
        </button>
        <InstallationSelect onOpenMame={onOpenMame} />
      </div>
    );
  }
  const freshness = status.freshness && !stale ? describeFreshness(status.freshness) : null;
  return (
    <Workspace
      {...props}
      // While switching, the software lists of the new catalog are not known
      // yet: the search reports them missing if needed.
      softwareImported={stale || status.catalog.software !== null}
      freshnessWarning={freshness?.warning ? freshness.message : null}
      onOpenMame={onOpenMame}
    />
  );
}

interface SearchResult {
  /** The installation whose catalog produced the results. */
  installationId: string;
  scope: Scope;
  items: ResultItem[];
  meta: ResultMeta;
}

interface FacetsResult {
  installationId: string;
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

async function runSearch(
  installationId: string,
  { scope, request }: BrowserRequest,
): Promise<SearchResult> {
  if (scope === "machines") {
    const response = await searchMachines(installationId, request);
    return {
      installationId,
      scope,
      items: response.items.map((machine) => ({ kind: "machine", key: machine.name, machine })),
      meta: response,
    };
  }
  const response = await searchSoftware(installationId, request);
  return {
    installationId,
    scope,
    items: response.items.map((software) => ({
      kind: "software",
      key: `${software.listName}/${software.name}`,
      software,
    })),
    meta: response,
  };
}

async function runFacetSearch(
  installationId: string,
  { scope, request }: BrowserRequest,
): Promise<FacetsResult> {
  const response =
    scope === "machines"
      ? await searchMachines(installationId, request)
      : await searchSoftware(installationId, request);
  return { installationId, scope, facets: response.facets };
}

function targetOf(item: ResultItem): DetailTarget {
  return item.kind === "machine"
    ? { kind: "machine", name: item.machine.name }
    : { kind: "software", list: item.software.listName, name: item.software.name };
}

interface WorkspaceProps extends Omit<CatalogContentProps, "status" | "stale"> {
  softwareImported: boolean;
  freshnessWarning: string | null;
}

function Workspace({
  installationId,
  state,
  dispatch,
  refreshKey,
  preferences,
  onPreferencesChange,
  onOpenMame,
  onOpenSupportFiles,
  debounceMs,
  softwareImported,
  freshnessWarning,
}: WorkspaceProps) {
  const { scope, appliedText, stack } = state;
  const query = state.queries[scope];
  const softwareUnavailable = scope === "software" && !softwareImported;
  const favorites = useFavorites();
  const { favoritesOnly } = preferences;
  // Favorites only restricts the search itself, so counts, facets and pages
  // cover the favorites of this catalog only.
  const list = favoritesOnly ? FAVORITES_LIST_ID : undefined;
  // Favorites-only results change with the list: they reload after every
  // saved change. Both keys only grow, so their sum changes with either.
  const searchKey = refreshKey + (favoritesOnly ? favorites.revision : 0);

  // New request objects only when the query changes, so rendering never searches again.
  const request = useMemo(
    () => buildResultsRequest(scope, appliedText, query, list),
    [scope, appliedText, query, list],
  );
  // The classifications of this catalog (support files), brought up to date
  // by the backend when they are read; tagged so that another catalog's
  // dimensions are never used.
  const loadClassifications = useCallback(
    async () => ({ installationId, status: await getClassificationStatus(installationId) }),
    [installationId],
  );
  const classificationLoad = useLoader(loadClassifications, refreshKey);
  const classificationStatus =
    classificationLoad.latest?.installationId === installationId
      ? classificationLoad.latest.status
      : null;
  const availableKey = classificationStatus?.dimensions.map((entry) => entry.dimension).join(",");
  const available = useMemo(
    () =>
      availableKey === undefined ? null : (availableKey.split(",").filter(Boolean) as Dimension[]),
    [availableKey],
  );
  const classifications: ClassificationsState = {
    available,
    syncError: classificationStatus?.syncError ?? null,
    error:
      classificationLoad.state.status === "error"
        ? describeError(classificationLoad.state.error)
        : null,
    onRetry: classificationLoad.reload,
  };

  // Facets do not depend on the page or the sort: changing them does not recount.
  const { filters, facetOptions } = query;
  // Keyed by text: "loading" and "none" request the same facets, so the
  // facets are not recounted when the status arrives without classifications.
  const facetDimensions = availableKey ?? "";
  const facetRequest = useMemo(
    () =>
      buildFacetRequest(
        scope,
        appliedText,
        filters,
        facetOptions,
        list,
        facetDimensions.split(",").filter(Boolean) as Dimension[],
      ),
    [scope, appliedText, filters, facetOptions, list, facetDimensions],
  );
  const loadFacetValues = useCallback(
    async (facet: string, valueSearch: string): Promise<FacetResult<string>> => {
      const { facets } = await runFacetSearch(
        installationId,
        buildFacetValuesRequest(scope, appliedText, filters, facet, valueSearch, list),
      );
      return facets[0] ?? { facet, values: [], hasMore: false };
    },
    [installationId, scope, appliedText, filters, list],
  );

  // Classification values that no machine of this catalog has (a saved
  // filter from another catalog, a removed source): they match nothing, and
  // the user is told so instead of seeing results silently change.
  const selectedClassifications = useMemo(() => {
    if (scope !== "machines") return [];
    return FACET_FILTERS.machines.flatMap((definition): DimensionValues[] => {
      const values = (filters.values[definition.facet] ?? []).filter(
        (value): value is string => value !== null,
      );
      return definition.classification && values.length > 0
        ? [{ dimension: definition.classification, values }]
        : [];
    });
  }, [scope, filters]);
  const loadMissing = useCallback(
    async () =>
      selectedClassifications.length === 0
        ? { installationId, missing: [] }
        : {
            installationId,
            missing: await findMissingClassificationValues(installationId, selectedClassifications),
          },
    [installationId, selectedClassifications],
  );
  const missingLoad = useLoader(loadMissing, refreshKey);
  const missing =
    missingLoad.state.status === "ready" &&
    missingLoad.state.value.installationId === installationId
      ? missingLoad.state.value.missing
      : [];
  const [presetNotice, setPresetNotice] = useState<string | null>(null);
  const criteria: BrowserCriteria = {
    scope,
    text: state.text.trim(),
    filters,
    favoritesOnly,
  };
  const load = useCallback(
    () => (softwareUnavailable ? Promise.resolve(null) : runSearch(installationId, request)),
    [installationId, request, softwareUnavailable],
  );
  const search = useLoader(load, searchKey);
  const current = search.state.status === "ready" ? search.state.value : null;
  // While searching, keep showing the previous results of the same catalog
  // and type; results of another installation are never shown.
  const sameCatalog = (result: { installationId: string; scope: Scope } | null) =>
    result?.installationId === installationId && result.scope === scope;
  const shown = current ?? (sameCatalog(search.latest) ? search.latest : null);
  const loadFacets = useCallback(
    () =>
      softwareUnavailable ? Promise.resolve(null) : runFacetSearch(installationId, facetRequest),
    [installationId, facetRequest, softwareUnavailable],
  );
  const facetSearch = useLoader(loadFacets, searchKey);
  const currentFacets = facetSearch.state.status === "ready" ? facetSearch.state.value : null;
  const shownFacets =
    currentFacets ?? (sameCatalog(facetSearch.latest) ? facetSearch.latest : null);

  const resultsRef = useRef<HTMLDivElement>(null);
  const savedScroll = useRef(0);
  const showingResults = stack.length === 0;

  useLayoutEffect(() => {
    if (showingResults && resultsRef.current) {
      resultsRef.current.scrollTop = savedScroll.current;
    }
  }, [showingResults]);

  // A new search, filter, sort or page starts at the top; changing the view
  // or returning from a detail keeps the position.
  useLayoutEffect(() => {
    savedScroll.current = 0;
    if (resultsRef.current) resultsRef.current.scrollTop = 0;
  }, [request]);

  // Removing the last favorite of the last page leaves it empty: show the new last page.
  const pastLastPage =
    favoritesOnly && current && current.meta.total > 0 && current.items.length === 0
      ? current.meta.pageCount - 1
      : null;
  useEffect(() => {
    if (pastLastPage !== null) dispatch({ type: "setPage", page: pastLastPage });
  }, [pastLastPage, dispatch]);

  const setFavoritesOnly = (value: boolean) => {
    onPreferencesChange({ favoritesOnly: value });
    dispatch({ type: "resetPages" });
  };

  const openFromResults = (index: number) => {
    const item = shown?.items[index];
    if (!item) return;
    savedScroll.current = resultsRef.current?.scrollTop ?? 0;
    dispatch({ type: "openDetail", target: targetOf(item), resultIndex: index });
  };

  const showFilters = preferences.filtersVisible && !softwareUnavailable;
  const showViewOptions = preferences.viewOptionsVisible;
  const bodyClass = [
    "browser__body",
    showFilters && "browser__body--filters",
    showViewOptions && "browser__body--view",
  ]
    .filter(Boolean)
    .join(" ");
  const filterCount = countActiveFilters(query.filters);
  const top = stack.at(-1);
  // One footer for every renderer, shown whenever there are results to page through.
  const pageMeta =
    !softwareUnavailable && search.state.status !== "error" && shown && shown.meta.total > 0
      ? shown.meta
      : null;

  return (
    <>
      {favorites.error && (
        <div className="browser__banner browser__notice" role="alert">
          <p className="message message--error">{favorites.error}</p>
          <button type="button" className="link-button" onClick={favorites.dismissError}>
            Dismiss
          </button>
        </div>
      )}
      {freshnessWarning && showingResults && (
        <p className="message message--warning browser__banner" role="status">
          {freshnessWarning}
        </p>
      )}
      {missing.length > 0 && showingResults && (
        <div className="browser__banner browser__notice" role="status">
          <p className="message message--warning">
            These classification criteria match nothing in this catalog:{" "}
            {missing
              .map(
                (entry) =>
                  `${DIMENSION_LABELS[entry.dimension]}: ${entry.values.join(", ")}${
                    available?.includes(entry.dimension) === false
                      ? " (no active support file)"
                      : ""
                  }`,
              )
              .join("; ")}
            .
          </p>
          <button type="button" className="link-button" onClick={onOpenSupportFiles}>
            Support Files settings
          </button>
        </div>
      )}
      {presetNotice && showingResults && (
        <div className="browser__banner browser__notice" role="status">
          <p className="message message--warning">{presetNotice}</p>
          <button
            type="button"
            className="link-button"
            onClick={() => {
              setPresetNotice(null);
            }}
          >
            Dismiss
          </button>
        </div>
      )}
      {state.filtersKept && showingResults && (
        <div className="browser__banner browser__notice" role="status">
          <p className="message message--muted">
            The filters were kept after changing the MAME version. Values that do not exist in this
            catalog match nothing.
          </p>
          <button
            type="button"
            className="link-button"
            onClick={() => {
              dispatch({ type: "clearFilters" });
            }}
          >
            Clear all filters
          </button>
          <button
            type="button"
            className="link-button"
            onClick={() => {
              dispatch({ type: "dismissFiltersKept" });
            }}
          >
            Dismiss
          </button>
        </div>
      )}

      <div className="browser__view" hidden={!showingResults}>
        <Toolbar
          scope={scope}
          text={state.text}
          query={query}
          appliedText={appliedText}
          filterCount={filterCount}
          preferences={preferences}
          dispatch={dispatch}
          onPreferencesChange={onPreferencesChange}
        />

        <div className={bodyClass}>
          {showFilters && (
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
              classifications={classifications}
              loadFacetValues={loadFacetValues}
              onOpenSupportFiles={onOpenSupportFiles}
            />
          )}

          {/* Only the results scroll: the pagination footer stays visible below them. */}
          <div className="browser__results" aria-busy={search.state.status === "loading"}>
            <div ref={resultsRef} className="browser__scroll" data-testid="results-scroll">
              <ActiveFilterBar
                scope={scope}
                state={state}
                dispatch={dispatch}
                favoritesOnly={favoritesOnly}
                onShowAll={() => {
                  setFavoritesOnly(false);
                }}
              />
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
                  onShowAll={() => {
                    setFavoritesOnly(false);
                  }}
                  onOpen={openFromResults}
                  onOpenMame={onOpenMame}
                />
              )}
            </div>
            {pageMeta && (
              <div className="browser__footer">
                <Pagination
                  page={pageMeta.page}
                  pageCount={pageMeta.pageCount}
                  pageSize={pageMeta.pageSize}
                  onPage={(page) => {
                    dispatch({ type: "setPage", page });
                  }}
                  onPageSize={(pageSize) => {
                    dispatch({ type: "setPageSize", pageSize });
                  }}
                />
              </div>
            )}
          </div>
          {showViewOptions && (
            <ViewOptionsPanel preferences={preferences} onPreferencesChange={onPreferencesChange}>
              <fieldset className="view-panel__section">
                <legend className="view-panel__title">MAME Version</legend>
                <InstallationSelect onOpenMame={onOpenMame} />
              </fieldset>
              <fieldset className="view-panel__section">
                <legend className="view-panel__title">Results</legend>
                <label className="view-panel__option view-panel__option--toggle">
                  <input
                    type="checkbox"
                    checked={favoritesOnly}
                    onChange={(event) => {
                      setFavoritesOnly(event.target.checked);
                    }}
                  />
                  <span>Favorites only</span>
                </label>
              </fieldset>
              <SavedFiltersSection
                criteria={criteria}
                activeId={state.presetId}
                onLoad={(presetId, loaded, ignored) => {
                  dispatch({ type: "applyCriteria", criteria: loaded, presetId });
                  if (loaded.favoritesOnly !== favoritesOnly) {
                    onPreferencesChange({ favoritesOnly: loaded.favoritesOnly });
                  }
                  setPresetNotice(
                    ignored.length > 0
                      ? `Some criteria of this saved filter are not supported by this version of YMDB and were not applied: ${ignored.join(", ")}.`
                      : null,
                  );
                }}
                onActiveChange={(presetId) => {
                  dispatch({ type: "setPreset", presetId });
                }}
              />
            </ViewOptionsPanel>
          )}
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
  favoritesOnly,
  onShowAll,
}: {
  scope: Scope;
  state: BrowserState;
  dispatch: Dispatch<BrowserAction>;
  favoritesOnly: boolean;
  onShowAll: () => void;
}) {
  const active = activeFilters(scope, state.queries[scope].filters);
  if (active.length === 0 && !favoritesOnly) return null;
  return (
    <div className="active-filters" aria-label="Active filters" role="group">
      {/* A view option, not a filter: "Clear all filters" keeps it. */}
      {favoritesOnly && (
        <span className="chip">
          Favorites only
          <button
            type="button"
            className="chip__remove"
            aria-label="Show all results, not only favorites"
            onClick={onShowAll}
          >
            ×
          </button>
        </span>
      )}
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
      {active.length > 0 && (
        <button
          type="button"
          className="link-button"
          onClick={() => {
            dispatch({ type: "clearFilters" });
          }}
        >
          Clear all filters
        </button>
      )}
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
  /** Turns Favorites only off. */
  onShowAll: () => void;
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
  onShowAll,
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
  const renderer = rendererById(preferences.renderer);
  const Renderer = renderer.Component;
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
          <p className="message message--muted">
            {preferences.favoritesOnly
              ? "No favorites match the search and filters in this catalog."
              : "No results match the search and filters."}
          </p>
          {preferences.favoritesOnly && (
            <button type="button" className="button button--secondary" onClick={onShowAll}>
              Show all results
            </button>
          )}
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
        <Renderer
          items={items}
          options={preferences.rendererOptions[renderer.id] ?? {}}
          onOpen={onOpen}
        />
      </div>
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

  const navigation: DetailNavigation = {
    start: (
      <>
        <button
          type="button"
          className="button button--secondary button--small"
          onClick={() => {
            dispatch({ type: "back" });
          }}
        >
          {depth > 1 ? "Back" : "Back to results"}
        </button>
        {depth > 1 && (
          <button
            type="button"
            className="button button--secondary button--small"
            onClick={() => {
              dispatch({ type: "backToResults" });
            }}
          >
            Back to results
          </button>
        )}
      </>
    ),
    end: resultIndex !== null && (
      <>
        <button
          type="button"
          className="button button--secondary button--small"
          disabled={!previous}
          onClick={() => {
            if (previous) dispatch({ type: "replaceDetail", ...previous });
          }}
        >
          Previous
        </button>
        <span className="detail-header__position">
          {resultIndex + 1} / {items.length}
        </span>
        <button
          type="button"
          className="button button--secondary button--small"
          disabled={!next}
          onClick={() => {
            if (next) dispatch({ type: "replaceDetail", ...next });
          }}
        >
          Next
        </button>
      </>
    ),
  };

  return (
    <div className="browser__detail">
      {target.kind === "machine" ? (
        <MachineDetailView
          key={key}
          name={target.name}
          tab={tab}
          refreshKey={refreshKey}
          onTab={onTab}
          onOpen={onOpen}
          onShowClassification={(facet, value) => {
            dispatch({ type: "showClassification", facet, value });
          }}
          navigation={navigation}
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
          navigation={navigation}
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
          navigation={navigation}
        />
      )}
    </div>
  );
}
