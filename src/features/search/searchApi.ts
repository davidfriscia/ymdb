import { invokeCommand } from "../../shared/ipc";

// These types mirror the Rust contracts in `src-tauri/src/catalog/search/`.
// Keep both sides in sync.

export type SortDirection = "ascending" | "descending";
export type CloneFilter = "parents" | "clones";

/** Mirrors `FacetRequest` in `request.rs`. */
export interface FacetRequest<F> {
  facet: F;
  /** Case-insensitive substring that values must contain. */
  search?: string;
  /** Maximum number of values (default 50, at most 1000). */
  limit?: number;
}

/** Mirrors `FacetValue` in `mod.rs`; `null` is the "no value" entry. */
export interface FacetValue {
  value: string | null;
  count: number;
}

export interface FacetResult<F> {
  facet: F;
  values: FacetValue[];
  hasMore: boolean;
}

/** Mirrors `SearchResponse` in `mod.rs`. */
export interface SearchResponse<T, F> {
  items: T[];
  total: number;
  /** Zero-based page index. */
  page: number;
  pageSize: number;
  pageCount: number;
  facets: FacetResult<F>[];
  elapsedMs: number;
}

// Machines

export type MachineSort = "relevance" | "name" | "description" | "year" | "manufacturer";
export type MachineFacet =
  | "manufacturer"
  | "year"
  | "driverStatus"
  | "emulationStatus"
  | "softwareList"
  | "genre"
  | "category"
  | "series"
  | "versionAdded";

/** Mirrors `VersionRange`: both bounds included, in MAME version order. */
export interface VersionRange {
  from?: string;
  to?: string;
}

/** Mirrors `MachineFilters`; `null` in a value list selects entries without a value. */
export interface MachineFilters {
  manufacturers?: (string | null)[];
  years?: (string | null)[];
  clones?: CloneFilter;
  /** Clones of this parent (short name). */
  cloneOf?: string;
  bios?: boolean;
  device?: boolean;
  mechanical?: boolean;
  runnable?: boolean;
  driverStatuses?: (string | null)[];
  emulationStatuses?: (string | null)[];
  hasRoms?: boolean;
  hasDisks?: boolean;
  softwareLists?: string[];
  /** Classifications of the active support file sources. */
  genres?: string[];
  /** Category nodes; a node selects its descendants. */
  categories?: string[];
  series?: string[];
  /** Versions added; alternatives of `versionAddedRange`. */
  versionsAdded?: string[];
  versionAddedRange?: VersionRange;
  /** Only machines of every one of these user lists (Favorites, a custom list). */
  userLists?: string[];
}

export interface MachineSearchRequest {
  text?: string;
  filters?: MachineFilters;
  sort?: MachineSort;
  direction?: SortDirection;
  page?: number;
  pageSize?: number;
  facets?: FacetRequest<MachineFacet>[];
}

/** Mirrors `MachineSummary` in `machines.rs`. */
export interface MachineSummary {
  name: string;
  description: string;
  year: string | null;
  manufacturer: string | null;
  cloneOf: string | null;
  isBios: boolean;
  isDevice: boolean;
  isMechanical: boolean;
  isRunnable: boolean;
  driverStatus: string | null;
  emulationStatus: string | null;
}

export type MachineSearchResponse = SearchResponse<MachineSummary, MachineFacet>;

// Software

export type SoftwareSort = "relevance" | "name" | "description" | "year" | "publisher" | "list";
export type SoftwareFacet = "list" | "publisher" | "year" | "supported" | "interface";
export type SupportStatus = "yes" | "partial" | "no";

export interface SoftwareFilters {
  lists?: string[];
  publishers?: (string | null)[];
  years?: (string | null)[];
  clones?: CloneFilter;
  /** Clones of this parent (short name). */
  cloneOf?: string;
  supported?: SupportStatus[];
  hasRoms?: boolean;
  hasDisks?: boolean;
  interfaces?: string[];
  /** Only software of every one of these user lists (for example Favorites). */
  userLists?: string[];
}

export interface SoftwareSearchRequest {
  text?: string;
  filters?: SoftwareFilters;
  sort?: SoftwareSort;
  direction?: SortDirection;
  page?: number;
  pageSize?: number;
  facets?: FacetRequest<SoftwareFacet>[];
}

/** Mirrors `SoftwareSummary` in `software.rs`. */
export interface SoftwareSummary {
  listName: string;
  listDescription: string | null;
  name: string;
  description: string;
  year: string | null;
  publisher: string | null;
  cloneOf: string | null;
  supported: SupportStatus;
}

export type SoftwareSearchResponse = SearchResponse<SoftwareSummary, SoftwareFacet>;

/** Searches the machines of an installation's catalog. */
export function searchMachines(
  installationId: string,
  request: MachineSearchRequest,
): Promise<MachineSearchResponse> {
  return invokeCommand("search_catalog_machines", { installationId, request });
}

/** Searches the software of an installation's catalog. */
export function searchSoftware(
  installationId: string,
  request: SoftwareSearchRequest,
): Promise<SoftwareSearchResponse> {
  return invokeCommand("search_catalog_software", { installationId, request });
}
