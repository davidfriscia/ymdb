import { invokeCommand } from "../../shared/ipc";

// These types mirror `src-tauri/src/saved_filters.rs` and
// `src-tauri/src/commands/saved_filters.rs`. Keep both sides in sync.

/**
 * A saved filter preset. `criteria` is stored verbatim by the backend; its
 * format is defined by the browser (`savedFilterCriteria.ts`) and versioned
 * by `criteriaVersion`.
 */
export interface SavedFilter {
  id: string;
  name: string;
  criteriaVersion: number;
  criteria: unknown;
  /** Milliseconds since the Unix epoch. */
  createdAt: number;
  updatedAt: number;
}

/** Mirrors `SavedFilterCreated`. */
export interface SavedFilterCreated {
  id: string;
  presets: SavedFilter[];
}

export function getSavedFilters(): Promise<SavedFilter[]> {
  return invokeCommand("get_saved_filters");
}

export function createSavedFilter(
  name: string,
  criteriaVersion: number,
  criteria: unknown,
): Promise<SavedFilterCreated> {
  return invokeCommand("create_saved_filter", { name, criteriaVersion, criteria });
}

/** Replaces the criteria of a preset with new ones. */
export function updateSavedFilter(
  id: string,
  criteriaVersion: number,
  criteria: unknown,
): Promise<SavedFilter[]> {
  return invokeCommand("update_saved_filter", { id, criteriaVersion, criteria });
}

export function renameSavedFilter(id: string, name: string): Promise<SavedFilter[]> {
  return invokeCommand("rename_saved_filter", { id, name });
}

export function deleteSavedFilter(id: string): Promise<SavedFilter[]> {
  return invokeCommand("delete_saved_filter", { id });
}
