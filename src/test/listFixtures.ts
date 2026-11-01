import type { ListEntry, ListSummary, ListView } from "../features/lists/listsApi";

/** The Favorites list with the given entries. */
export function favoritesList(entries: ListEntry[] = []): ListView {
  return { id: "favorites", kind: "system", name: "Favorites", entries };
}

/** A custom list summary. */
export function listSummary(
  id: string,
  name: string,
  machines = 0,
  overrides: Partial<ListSummary> = {},
): ListSummary {
  return {
    id,
    name,
    machines,
    entries: machines,
    createdAt: Date.UTC(2026, 9, 1),
    updatedAt: Date.UTC(2026, 9, 2),
    ...overrides,
  };
}
