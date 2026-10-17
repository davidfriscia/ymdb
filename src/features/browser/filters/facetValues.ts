import type { FacetValue } from "../../search/searchApi";
import { CATEGORY_SEPARATOR } from "../../support-files/supportFilesApi";
import type { FacetFilterDefinition } from "../filterDefinitions";

/** A value to show: `count` is `null` for selected values the response did not return. */
export interface FacetRow {
  value: string | null;
  count: number | null;
}

/**
 * The rows to show: selected values missing from the returned values first
 * (they stay selectable to be removed, with "—" as count), then the values.
 */
export function facetRows(
  definition: FacetFilterDefinition,
  values: FacetValue[],
  selected: (string | null)[],
): FacetRow[] {
  const missing = selected.filter((value) => !values.some((entry) => entry.value === value));
  return [
    ...missing.map((value) => ({ value, count: null })),
    ...values.filter((entry) => definition.nullable || entry.value !== null),
  ];
}

/** Case-insensitive substring match on the displayed label. */
export function matchesSearch(label: string, search: string): boolean {
  const needle = search.trim().toLowerCase();
  return needle === "" || label.toLowerCase().includes(needle);
}

/**
 * A category node. The backend returns every node path (`Shooter`,
 * `Shooter / Flying Vertical`) with its count; the levels come from the
 * data, so nodes may have any depth.
 */
export interface CategoryNode {
  path: string;
  label: string;
  /** `null` for nodes the response did not return (selected or ancestors). */
  count: number | null;
  children: CategoryNode[];
}

/** Builds the tree of `values` and of the `selected` paths, sorted by label. */
export function buildCategoryTree(
  values: FacetValue[],
  selected: (string | null)[],
): CategoryNode[] {
  const nodes = new Map<string, CategoryNode>();
  const roots: CategoryNode[] = [];
  const ensure = (path: string): CategoryNode => {
    const existing = nodes.get(path);
    if (existing) return existing;
    const levels = path.split(CATEGORY_SEPARATOR);
    const node: CategoryNode = {
      path,
      label: levels.at(-1) ?? path,
      count: null,
      children: [],
    };
    nodes.set(path, node);
    if (levels.length > 1) {
      ensure(levels.slice(0, -1).join(CATEGORY_SEPARATOR)).children.push(node);
    } else {
      roots.push(node);
    }
    return node;
  };
  for (const { value, count } of values) {
    if (value !== null) ensure(value).count = count;
  }
  for (const path of selected) {
    if (path !== null) ensure(path);
  }
  const sort = (list: CategoryNode[]) => {
    list.sort((a, b) => a.label.localeCompare(b.label));
    list.forEach((node) => {
      sort(node.children);
    });
  };
  sort(roots);
  return roots;
}

/** Whether the node or one of its descendants contains `needle` (lowercase). */
export function nodeMatches(node: CategoryNode, needle: string): boolean {
  return (
    node.path.toLowerCase().includes(needle) ||
    node.children.some((child) => nodeMatches(child, needle))
  );
}
