import { createContext, useContext } from "react";
import type { ListSummary } from "./listsApi";

export interface CustomListsContextValue {
  /** The custom lists in creation order; `null` until loaded (or on error). */
  lists: readonly ListSummary[] | null;
  /** Set when the lists could not be read. */
  loadError: unknown;
  /** The list with this id, if it exists (`undefined` while loading too). */
  byId: (id: string) => ListSummary | undefined;
  /**
   * Incremented after every saved change of a custom list (name, entries,
   * creation, deletion, import), so that views showing lists reload.
   */
  revision: number;
  /** Reads the lists again after a change saved by the caller. */
  changed: () => void;
}

export const CustomListsContext = createContext<CustomListsContextValue | null>(null);

export function useCustomLists(): CustomListsContextValue {
  const value = useContext(CustomListsContext);
  if (!value) throw new Error("useCustomLists must be used inside CustomListsProvider");
  return value;
}
