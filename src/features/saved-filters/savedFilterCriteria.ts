import { emptyFilters, type BrowserCriteria, type ScopeFilters } from "../browser/browserState";
import { FACET_FILTERS, FLAG_FILTERS, type Scope } from "../browser/filterDefinitions";
import type { VersionRange } from "../search/searchApi";

/**
 * The criteria of a saved filter, as stored in `saved-filters.json`: the
 * catalog type, the search text, the filters of that type (facet values,
 * single-choice filters, version ranges) and Favorites only. Presentation
 * (renderer, sizes, panels, theme), sorting and paging are not part of it.
 *
 * Criteria reference classifications and other values by name only, never
 * by installation or catalog row, so they apply to any catalog. A new kind
 * of filter gets a new {@link CRITERIA_VERSION} and a conversion here; a
 * preset of an unknown version is kept as it is and cannot be loaded.
 */
export const CRITERIA_VERSION = 1;

/** The stored shape of version 1. */
interface StoredCriteriaV1 {
  scope: Scope;
  text: string;
  values: Record<string, (string | null)[]>;
  flags: Record<string, string>;
  ranges: Record<string, VersionRange>;
  favoritesOnly: boolean;
}

export type ParsedCriteria =
  | {
      kind: "ok";
      criteria: BrowserCriteria;
      /** Criteria of the preset this version does not know (ignored). */
      ignored: string[];
    }
  | { kind: "unsupported"; version: number }
  | { kind: "invalid"; message: string };

const sorted = (values: (string | null)[]) =>
  [...new Set(values)].sort((a, b) => (a ?? "").localeCompare(b ?? "") || (a === null ? -1 : 1));

/** Criteria in their stored form, normalized so that equal criteria compare equal. */
export function encodeCriteria(criteria: BrowserCriteria): StoredCriteriaV1 {
  const { filters } = criteria;
  const values: Record<string, (string | null)[]> = {};
  for (const key of Object.keys(filters.values).sort()) {
    const selected = filters.values[key] ?? [];
    if (selected.length > 0) values[key] = sorted(selected);
  }
  const flags: Record<string, string> = {};
  for (const key of Object.keys(filters.flags).sort()) {
    const value = filters.flags[key];
    if (value !== undefined) flags[key] = value;
  }
  const ranges: Record<string, VersionRange> = {};
  for (const key of Object.keys(filters.ranges).sort()) {
    const range = filters.ranges[key];
    if (range && (range.from !== undefined || range.to !== undefined)) {
      ranges[key] = {
        ...(range.from !== undefined && { from: range.from }),
        ...(range.to !== undefined && { to: range.to }),
      };
    }
  }
  return {
    scope: criteria.scope,
    text: criteria.text.trim(),
    values,
    flags,
    ranges,
    favoritesOnly: criteria.favoritesOnly,
  };
}

export function sameCriteria(a: BrowserCriteria, b: BrowserCriteria): boolean {
  return JSON.stringify(encodeCriteria(a)) === JSON.stringify(encodeCriteria(b));
}

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === "object" && value !== null && !Array.isArray(value);

/**
 * Reads stored criteria. Unknown filters and values (for example of a filter
 * removed from YMDB) are reported and left out, so loading never applies
 * something else instead; the stored preset itself is not changed.
 */
export function parseCriteria(version: number, stored: unknown): ParsedCriteria {
  if (version !== CRITERIA_VERSION) return { kind: "unsupported", version };
  if (!isRecord(stored)) return { kind: "invalid", message: "the criteria are not an object" };
  const { scope, text, values, flags, ranges, favoritesOnly } = stored;
  if (scope !== "machines" && scope !== "software") {
    return { kind: "invalid", message: "unknown catalog type" };
  }
  if (typeof text !== "string" || typeof favoritesOnly !== "boolean") {
    return { kind: "invalid", message: "the search text or Favorites only is missing" };
  }
  if (!isRecord(values) || !isRecord(flags) || !isRecord(ranges)) {
    return { kind: "invalid", message: "the filters are missing" };
  }
  const ignored: string[] = [];
  const filters: ScopeFilters = emptyFilters();
  for (const [facet, selected] of Object.entries(values)) {
    const definition = FACET_FILTERS[scope].find((entry) => entry.facet === facet);
    const valid =
      Array.isArray(selected) &&
      selected.every((value) => value === null || typeof value === "string");
    if (!definition || !valid) {
      ignored.push(facet);
      continue;
    }
    const kept = (selected as (string | null)[]).filter(
      (value) => value !== null || definition.nullable,
    );
    if (kept.length > 0) filters.values[facet] = kept;
  }
  for (const [flag, value] of Object.entries(flags)) {
    const definition = FLAG_FILTERS[scope].find((entry) => entry.id === flag);
    if (!definition?.options.some((option) => option.value === value)) {
      ignored.push(flag);
      continue;
    }
    filters.flags[flag] = value as string;
  }
  for (const [facet, range] of Object.entries(ranges)) {
    const definition = FACET_FILTERS[scope].find((entry) => entry.facet === facet);
    const bound = (value: unknown) => value === undefined || typeof value === "string";
    if (
      definition?.presentation !== "version" ||
      !isRecord(range) ||
      !bound(range.from) ||
      !bound(range.to)
    ) {
      ignored.push(facet);
      continue;
    }
    filters.ranges[facet] = {
      ...(typeof range.from === "string" && { from: range.from }),
      ...(typeof range.to === "string" && { to: range.to }),
    };
  }
  return { kind: "ok", criteria: { scope, text, filters, favoritesOnly }, ignored };
}
