import type {
  FacetRequest,
  MachineFacet,
  MachineFilters,
  MachineSearchRequest,
  MachineSort,
  SoftwareFacet,
  SoftwareFilters,
  SoftwareSearchRequest,
  SoftwareSort,
  SortDirection,
  SupportStatus,
  CloneFilter,
} from "../search/searchApi";
import { FACET_FILTERS, FLAG_FILTERS, formatFacetValue, type Scope } from "./filterDefinitions";

/**
 * State of the catalog browser. It only describes the query and the
 * navigation: searching, filtering, sorting, faceting and pagination are done
 * by the backend search engine.
 */

export const PAGE_SIZES = [25, 50, 100, 200] as const;
export const DEFAULT_PAGE_SIZE = 50;
export const FACET_LIMIT_STEP = 50;
export const FACET_LIMIT_MAX = 200;

export interface ScopeFilters {
  /** Selected values per facet; values of one facet are alternatives. */
  values: Partial<Record<string, (string | null)[]>>;
  /** Selected option per flag filter; missing means "any". */
  flags: Partial<Record<string, string>>;
}

export interface FacetOptions {
  search: string;
  limit: number;
}

export interface ScopeQuery {
  filters: ScopeFilters;
  /** `null` uses the default: relevance with search text, name otherwise. */
  sort: MachineSort | SoftwareSort | null;
  direction: SortDirection;
  /** Zero-based page index. */
  page: number;
  pageSize: number;
  facetOptions: Partial<Record<string, FacetOptions>>;
}

export type DetailTarget =
  | { kind: "machine"; name: string }
  | { kind: "software"; list: string; name: string }
  | { kind: "softwareList"; name: string };

export interface DetailEntry {
  target: DetailTarget;
  /** Position in the current result page, for previous/next navigation. */
  resultIndex: number | null;
  tab: string;
}

export interface BrowserState {
  scope: Scope;
  /** Text in the search box. */
  text: string;
  /** Text used by the search (applied after the debounce or on submit). */
  appliedText: string;
  queries: Record<Scope, ScopeQuery>;
  /** Detail views opened on top of the results; empty shows the results. */
  stack: DetailEntry[];
  /**
   * Set when the MAME version changed while filters were active: they are
   * kept, but values missing from the new catalog match nothing.
   */
  filtersKept: boolean;
}

const emptyQuery = (): ScopeQuery => ({
  filters: { values: {}, flags: {} },
  sort: null,
  direction: "ascending",
  page: 0,
  pageSize: DEFAULT_PAGE_SIZE,
  facetOptions: {},
});

export const initialBrowserState: BrowserState = {
  scope: "machines",
  text: "",
  appliedText: "",
  queries: { machines: emptyQuery(), software: emptyQuery() },
  stack: [],
  filtersKept: false,
};

export type BrowserAction =
  | { type: "setScope"; scope: Scope }
  | { type: "setText"; text: string }
  | { type: "applyText" }
  | { type: "toggleValue"; facet: string; value: string | null }
  | { type: "setFlag"; flag: string; value: string | null }
  | { type: "clearFacet"; facet: string }
  | { type: "clearFilters" }
  | { type: "setSort"; sort: MachineSort | SoftwareSort | null }
  | { type: "setDirection"; direction: SortDirection }
  | { type: "setPage"; page: number }
  | { type: "setPageSize"; pageSize: number }
  | { type: "setFacetSearch"; facet: string; search: string }
  | { type: "showMoreFacetValues"; facet: string }
  | { type: "openDetail"; target: DetailTarget; resultIndex: number | null }
  | { type: "replaceDetail"; target: DetailTarget; resultIndex: number | null }
  | { type: "setDetailTab"; tab: string }
  | { type: "back" }
  | { type: "backToResults" }
  | { type: "resetPages" }
  | { type: "installationChanged" }
  | { type: "dismissFiltersKept" };

function updateQuery(state: BrowserState, update: (query: ScopeQuery) => ScopeQuery): BrowserState {
  const query = state.queries[state.scope];
  return { ...state, queries: { ...state.queries, [state.scope]: update(query) } };
}

function without<T>(record: Partial<Record<string, T>>, key: string): Partial<Record<string, T>> {
  return Object.fromEntries(Object.entries(record).filter(([name]) => name !== key));
}

/** Any change to the query returns to the first page. */
function updateFilters(state: BrowserState, update: (filters: ScopeFilters) => ScopeFilters) {
  return updateQuery(state, (query) => ({ ...query, filters: update(query.filters), page: 0 }));
}

export function facetOptionsOf(
  query: Pick<ScopeQuery, "facetOptions">,
  facet: string,
): FacetOptions {
  return query.facetOptions[facet] ?? { search: "", limit: FACET_LIMIT_STEP };
}

export function browserReducer(state: BrowserState, action: BrowserAction): BrowserState {
  switch (action.type) {
    case "setScope":
      return state.scope === action.scope ? state : { ...state, scope: action.scope, stack: [] };
    case "setText":
      return { ...state, text: action.text };
    case "applyText": {
      if (state.text === state.appliedText) return state;
      const applied = { ...state, appliedText: state.text };
      return {
        ...applied,
        queries: {
          machines: { ...applied.queries.machines, page: 0 },
          software: { ...applied.queries.software, page: 0 },
        },
      };
    }
    case "toggleValue":
      return updateFilters(state, (filters) => {
        const selected = filters.values[action.facet] ?? [];
        const next = selected.includes(action.value)
          ? selected.filter((value) => value !== action.value)
          : [...selected, action.value];
        return { ...filters, values: { ...filters.values, [action.facet]: next } };
      });
    case "setFlag":
      return updateFilters(state, (filters) => {
        const flags = without(filters.flags, action.flag);
        return {
          ...filters,
          flags: action.value === null ? flags : { ...flags, [action.flag]: action.value },
        };
      });
    case "clearFacet":
      return updateFilters(state, (filters) => ({
        ...filters,
        values: without(filters.values, action.facet),
      }));
    case "clearFilters":
      return {
        ...updateFilters(state, () => ({ values: {}, flags: {} })),
        filtersKept: false,
      };
    case "setSort":
      return updateQuery(state, (query) => ({ ...query, sort: action.sort, page: 0 }));
    case "setDirection":
      return updateQuery(state, (query) => ({ ...query, direction: action.direction, page: 0 }));
    case "setPage":
      return updateQuery(state, (query) => ({ ...query, page: Math.max(0, action.page) }));
    case "setPageSize":
      return updateQuery(state, (query) => ({ ...query, pageSize: action.pageSize, page: 0 }));
    case "setFacetSearch":
      return updateQuery(state, (query) => ({
        ...query,
        facetOptions: {
          ...query.facetOptions,
          [action.facet]: { search: action.search, limit: FACET_LIMIT_STEP },
        },
      }));
    case "showMoreFacetValues":
      return updateQuery(state, (query) => {
        const options = facetOptionsOf(query, action.facet);
        return {
          ...query,
          facetOptions: {
            ...query.facetOptions,
            [action.facet]: {
              ...options,
              limit: Math.min(FACET_LIMIT_MAX, options.limit + FACET_LIMIT_STEP),
            },
          },
        };
      });
    case "openDetail":
      return {
        ...state,
        stack: [
          ...state.stack,
          { target: action.target, resultIndex: action.resultIndex, tab: "overview" },
        ],
      };
    case "replaceDetail": {
      const current = state.stack.at(-1);
      if (!current) return state;
      return {
        ...state,
        stack: [
          ...state.stack.slice(0, -1),
          { target: action.target, resultIndex: action.resultIndex, tab: current.tab },
        ],
      };
    }
    case "setDetailTab": {
      const current = state.stack.at(-1);
      if (!current) return state;
      return { ...state, stack: [...state.stack.slice(0, -1), { ...current, tab: action.tab }] };
    }
    case "back":
      return { ...state, stack: state.stack.slice(0, -1) };
    case "backToResults":
      return { ...state, stack: [] };
    case "resetPages":
      // The results of both scopes changed (for example Favorites only): first pages.
      return {
        ...state,
        queries: {
          machines: { ...state.queries.machines, page: 0 },
          software: { ...state.queries.software, page: 0 },
        },
      };
    case "installationChanged":
      // Detail views and pages belong to the previous catalog. The search
      // text, filters and sorting are kept.
      return {
        ...state,
        stack: [],
        queries: {
          machines: { ...state.queries.machines, page: 0 },
          software: { ...state.queries.software, page: 0 },
        },
        filtersKept:
          countActiveFilters(state.queries.machines.filters) +
            countActiveFilters(state.queries.software.filters) >
          0,
      };
    case "dismissFiltersKept":
      return { ...state, filtersKept: false };
  }
}

// Active filters

export interface ActiveFilter {
  key: string;
  label: string;
  remove: BrowserAction;
}

export function countActiveFilters(filters: ScopeFilters): number {
  const values = Object.values(filters.values).reduce(
    (count, selected) => count + (selected?.length ?? 0),
    0,
  );
  return values + Object.keys(filters.flags).length;
}

export function activeFilters(scope: Scope, filters: ScopeFilters): ActiveFilter[] {
  const active: ActiveFilter[] = [];
  for (const definition of FACET_FILTERS[scope]) {
    for (const value of filters.values[definition.facet] ?? []) {
      const valueLabel = formatFacetValue(definition, value);
      active.push({
        key: `${definition.facet}:${value ?? "\u0000"}`,
        label: `${definition.label}: ${valueLabel}`,
        remove: { type: "toggleValue", facet: definition.facet, value },
      });
    }
  }
  for (const definition of FLAG_FILTERS[scope]) {
    const value = filters.flags[definition.id];
    const option = definition.options.find((candidate) => candidate.value === value);
    if (option) {
      active.push({
        key: `flag:${definition.id}`,
        label: `${definition.label}: ${option.label}`,
        remove: { type: "setFlag", flag: definition.id, value: null },
      });
    }
  }
  return active;
}

// Search requests

const MACHINE_SORTS: readonly MachineSort[] = [
  "relevance",
  "name",
  "description",
  "year",
  "manufacturer",
];
const SOFTWARE_SORTS: readonly SoftwareSort[] = [
  "relevance",
  "name",
  "description",
  "year",
  "publisher",
  "list",
];

export function sortOptions(scope: Scope): readonly (MachineSort | SoftwareSort)[] {
  return scope === "machines" ? MACHINE_SORTS : SOFTWARE_SORTS;
}

/** The sort actually used: relevance needs search text. */
export function effectiveSort(
  query: ScopeQuery,
  text: string,
): MachineSort | SoftwareSort | undefined {
  if (query.sort === null) return undefined;
  if (query.sort === "relevance" && text.trim() === "") return undefined;
  return query.sort;
}

const bool = (value: string | undefined) => (value === undefined ? undefined : value === "yes");

function selected(filters: ScopeFilters, facet: string): (string | null)[] {
  return filters.values[facet] ?? [];
}

function present(filters: ScopeFilters, facet: string): string[] {
  return selected(filters, facet).filter((value): value is string => value !== null);
}

/** Removes empty and unset fields so requests stay small and readable. */
function compact<T extends object>(value: T): T {
  return Object.fromEntries(
    Object.entries(value).filter(
      ([, field]) => field !== undefined && !(Array.isArray(field) && field.length === 0),
    ),
  ) as T;
}

function facetRequests<F extends string>(
  facets: readonly F[],
  query: Pick<ScopeQuery, "facetOptions">,
): FacetRequest<F>[] {
  return facets.map((facet) => {
    const options = facetOptionsOf(query, facet);
    return compact({
      facet,
      search: options.search.trim() === "" ? undefined : options.search.trim(),
      limit: options.limit,
    });
  });
}

function textOf(text: string): string | undefined {
  return text.trim() === "" ? undefined : text.trim();
}

export function machineFilters(filters: ScopeFilters, list?: string): MachineFilters {
  return compact<MachineFilters>({
    manufacturers: selected(filters, "manufacturer"),
    years: selected(filters, "year"),
    driverStatuses: selected(filters, "driverStatus"),
    emulationStatuses: selected(filters, "emulationStatus"),
    softwareLists: present(filters, "softwareList"),
    clones: filters.flags.clones as CloneFilter | undefined,
    bios: bool(filters.flags.bios),
    device: bool(filters.flags.device),
    mechanical: bool(filters.flags.mechanical),
    runnable: bool(filters.flags.runnable),
    hasRoms: bool(filters.flags.hasRoms),
    hasDisks: bool(filters.flags.hasDisks),
    list,
  });
}

export function softwareFilters(filters: ScopeFilters, list?: string): SoftwareFilters {
  return compact<SoftwareFilters>({
    lists: present(filters, "list"),
    publishers: selected(filters, "publisher"),
    years: selected(filters, "year"),
    supported: present(filters, "supported") as SupportStatus[],
    interfaces: present(filters, "interface"),
    clones: filters.flags.clones as CloneFilter | undefined,
    hasRoms: bool(filters.flags.hasRoms),
    hasDisks: bool(filters.flags.hasDisks),
    list,
  });
}

export type BrowserRequest =
  | { scope: "machines"; request: MachineSearchRequest }
  | { scope: "software"; request: SoftwareSearchRequest };

/**
 * The request for the result page. Facets are requested separately
 * ({@link buildFacetRequest}) because they do not depend on the page or the
 * sort, and computing them over the whole catalog is the expensive part.
 */
export function buildResultsRequest(
  scope: Scope,
  text: string,
  query: ScopeQuery,
  list?: string,
): BrowserRequest {
  const common = compact({
    text: textOf(text),
    direction: query.direction,
    page: query.page,
    pageSize: query.pageSize,
  });
  if (scope === "machines") {
    return {
      scope,
      request: {
        ...common,
        ...compact({ sort: effectiveSort(query, text) as MachineSort | undefined }),
        filters: machineFilters(query.filters, list),
      },
    };
  }
  return {
    scope,
    request: {
      ...common,
      ...compact({ sort: effectiveSort(query, text) as SoftwareSort | undefined }),
      filters: softwareFilters(query.filters, list),
    },
  };
}

/** The request for the facets: same text and filters, a single item. */
export function buildFacetRequest(
  scope: Scope,
  text: string,
  filters: ScopeFilters,
  facetOptions: ScopeQuery["facetOptions"],
  list?: string,
): BrowserRequest {
  const query = { facetOptions };
  const common = compact({ text: textOf(text), pageSize: 1 });
  if (scope === "machines") {
    return {
      scope,
      request: {
        ...common,
        filters: machineFilters(filters, list),
        facets: facetRequests(
          FACET_FILTERS.machines.map((definition) => definition.facet as MachineFacet),
          query,
        ),
      },
    };
  }
  return {
    scope,
    request: {
      ...common,
      filters: softwareFilters(filters, list),
      facets: facetRequests(
        FACET_FILTERS.software.map((definition) => definition.facet as SoftwareFacet),
        query,
      ),
    },
  };
}

/** Page numbers (zero-based) around the current page, with `null` for gaps. */
export function pageWindow(page: number, pageCount: number, radius = 2): (number | null)[] {
  if (pageCount <= 0) return [];
  const pages = new Set<number>([0, pageCount - 1]);
  for (let candidate = page - radius; candidate <= page + radius; candidate++) {
    if (candidate >= 0 && candidate < pageCount) pages.add(candidate);
  }
  const sorted = [...pages].sort((a, b) => a - b);
  const result: (number | null)[] = [];
  sorted.forEach((value, index) => {
    const previous = sorted[index - 1];
    if (previous !== undefined && value - previous > 1) result.push(null);
    result.push(value);
  });
  return result;
}
