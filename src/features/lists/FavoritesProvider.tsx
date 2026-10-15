import { useCallback, useMemo, useState, type ReactNode } from "react";
import { describeError } from "../../shared/ipc";
import { useLoader } from "../../shared/useLoader";
import { FavoritesContext, type FavoritesContextValue } from "./favoritesContext";
import {
  FAVORITES_LIST_ID,
  entryKey,
  getList,
  setListMembership,
  type ListEntry,
} from "./listsApi";

const loadFavorites = () => getList(FAVORITES_LIST_ID);

function withKey(keys: ReadonlySet<string>, key: string, member: boolean): Set<string> {
  const next = new Set(keys);
  if (member) next.add(key);
  else next.delete(key);
  return next;
}

/**
 * Shares the Favorites list with every view. It is loaded once and global:
 * the same entries apply to the catalog of every MAME installation, so
 * switching installation never reloads it. Membership is checked in memory,
 * never with one request per card.
 */
export function FavoritesProvider({ children }: { children: ReactNode }) {
  const [version, setVersion] = useState(0);
  const loaded = useLoader(loadFavorites, version);
  const [keys, setKeys] = useState<ReadonlySet<string> | null>(null);
  const [pending, setPending] = useState<ReadonlySet<string>>(new Set());
  const [error, setError] = useState<string | null>(null);
  const [revision, setRevision] = useState(0);

  // Each loaded list replaces the keys, while rendering; local changes are
  // already saved by then.
  const list = loaded.state.status === "ready" ? loaded.state.value : null;
  const [shownList, setShownList] = useState(list);
  if (list !== null && list !== shownList) {
    setShownList(list);
    setKeys(new Set(list.entries.map(entryKey)));
  }

  const toggle = useCallback(
    (entry: ListEntry) => {
      const key = entryKey(entry);
      if (keys === null || pending.has(key)) return;
      const member = !keys.has(key);
      setKeys((current) => current && withKey(current, key, member));
      setPending((current) => withKey(current, key, true));
      setError(null);
      setListMembership(FAVORITES_LIST_ID, entry, member)
        .then(
          () => {
            setRevision((current) => current + 1);
          },
          (failure: unknown) => {
            // Back to the saved state.
            setKeys((current) => current && withKey(current, key, !member));
            setError(
              `${member ? "Could not add to favorites" : "Could not remove from favorites"}: ${describeError(failure)}`,
            );
          },
        )
        .finally(() => {
          setPending((current) => withKey(current, key, false));
        });
    },
    [keys, pending],
  );

  const reload = useCallback(() => {
    setVersion((current) => current + 1);
    setRevision((current) => current + 1);
  }, []);
  const dismissError = useCallback(() => {
    setError(null);
  }, []);
  const loadError = loaded.state.status === "error" ? loaded.state.error : null;

  const value = useMemo<FavoritesContextValue>(
    () => ({
      ready: keys !== null,
      loadError,
      count: keys?.size ?? 0,
      isFavorite: (entry) => keys?.has(entryKey(entry)) ?? false,
      isPending: (entry) => pending.has(entryKey(entry)),
      toggle,
      error,
      dismissError,
      revision,
      reload,
    }),
    [keys, loadError, pending, toggle, error, dismissError, revision, reload],
  );
  return <FavoritesContext value={value}>{children}</FavoritesContext>;
}
