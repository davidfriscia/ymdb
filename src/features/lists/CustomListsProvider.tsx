import { useCallback, useMemo, useState, type ReactNode } from "react";
import { useLoader } from "../../shared/useLoader";
import { CustomListsContext, type CustomListsContextValue } from "./customListsContext";
import { listUserLists, type ListSummary } from "./listsApi";

/**
 * Shares the custom lists (names and counts, never their entries) with every
 * view: the Lists page, the Lists filter of the catalog, the Manage Lists
 * window and saved filters. They are global, so switching installation never
 * reloads them. Views that change a list call `changed`, which reloads the
 * summaries and increments `revision` for the views that show entries.
 * Favorites is not a custom list and is shared by `FavoritesProvider`.
 */
export function CustomListsProvider({ children }: { children: ReactNode }) {
  const [revision, setRevision] = useState(0);
  const loaded = useLoader(listUserLists, revision);
  // The last lists read, kept while reloading so pickers do not flicker.
  const lists = loaded.state.status === "ready" ? loaded.state.value : loaded.latest;
  const loadError = loaded.state.status === "error" ? loaded.state.error : null;

  const changed = useCallback(() => {
    setRevision((current) => current + 1);
  }, []);

  const index = useMemo(
    () => new Map<string, ListSummary>((lists ?? []).map((list) => [list.id, list])),
    [lists],
  );

  const value = useMemo<CustomListsContextValue>(
    () => ({
      lists,
      loadError,
      byId: (id) => index.get(id),
      revision,
      changed,
    }),
    [lists, loadError, index, revision, changed],
  );
  return <CustomListsContext value={value}>{children}</CustomListsContext>;
}
