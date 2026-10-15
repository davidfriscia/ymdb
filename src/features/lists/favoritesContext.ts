import { createContext, useContext } from "react";
import type { ListEntry } from "./listsApi";

export interface FavoritesContextValue {
  /** Whether the Favorites list has been loaded. */
  ready: boolean;
  /** Set when the list could not be read; bookmarks are then disabled. */
  loadError: unknown;
  count: number;
  isFavorite: (entry: ListEntry) => boolean;
  /** Whether a change of this entry is being saved. */
  isPending: (entry: ListEntry) => boolean;
  /**
   * Adds the entry to Favorites or removes it. The change shows at once and
   * is undone, with an error, if it cannot be saved.
   */
  toggle: (entry: ListEntry) => void;
  /** The last change that could not be saved, until dismissed. */
  error: string | null;
  dismissError: () => void;
  /** Incremented after every saved change, so that favorites-only searches reload. */
  revision: number;
  /** Reads the list again (for example after an import). */
  reload: () => void;
}

export const FavoritesContext = createContext<FavoritesContextValue | null>(null);

export function useFavorites(): FavoritesContextValue {
  const value = useContext(FavoritesContext);
  if (!value) throw new Error("useFavorites must be used inside FavoritesProvider");
  return value;
}
