import type { ListEntry, ListView } from "../features/lists/listsApi";

/** The Favorites list with the given entries. */
export function favoritesList(entries: ListEntry[] = []): ListView {
  return { id: "favorites", kind: "system", name: "Favorites", entries };
}
