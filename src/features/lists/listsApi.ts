import { invokeCommand } from "../../shared/ipc";

// These types mirror the Rust contracts in `src-tauri/src/lists/` and
// `src-tauri/src/commands/lists.rs`. Keep both sides in sync.

/** Id of the Favorites system list (`FAVORITES` in `lists/model.rs`). */
export const FAVORITES_LIST_ID = "favorites";

/**
 * A machine or a software, by MAME names only: the same entry matches the
 * same title in the catalog of every MAME installation.
 */
export type ListEntry =
  { kind: "machine"; name: string } | { kind: "software"; listName: string; name: string };

export type ListKind = "system" | "user";

/** Mirrors `ListView` in `commands/lists.rs`. */
export interface ListView {
  id: string;
  kind: ListKind;
  name: string;
  entries: ListEntry[];
}

export type ImportMode = "merge" | "replace";

/** Mirrors `ListImportSummary` in `lists/exchange.rs`. */
export interface ListImportSummary {
  listId: string;
  name: string;
  entries: number;
  newEntries: number;
  existingEntries: number;
  duplicates: number;
  removedByReplace: number;
}

/** Mirrors `ListsImportPreview` in `commands/lists.rs`. */
export interface ListsImportPreview {
  importId: number;
  lists: ListImportSummary[];
  ignoredLists: string[];
}

/** Mirrors `ImportOutcome` in `lists/model.rs`. */
export interface ImportOutcome {
  listId: string;
  added: number;
  removed: number;
  entries: number;
}

/** A unique textual key (MAME names never contain `:`). */
export function entryKey(entry: ListEntry): string {
  return entry.kind === "machine"
    ? `machine:${entry.name}`
    : `software:${entry.listName}:${entry.name}`;
}

export function getList(listId: string): Promise<ListView> {
  return invokeCommand("get_list", { listId });
}

/** Adds the entry to the list or removes it; resolves to the new membership. */
export function setListMembership(
  listId: string,
  entry: ListEntry,
  member: boolean,
): Promise<boolean> {
  return invokeCommand("set_list_membership", { listId, entry, member });
}

/** Opens the native save dialog; resolves to the file written, or `null` when cancelled. */
export function exportLists(listIds: string[]): Promise<string | null> {
  return invokeCommand("export_lists", { listIds });
}

/** Opens the native file picker and checks the file; `null` when cancelled. */
export function previewListsImport(): Promise<ListsImportPreview | null> {
  return invokeCommand("preview_lists_import");
}

export function applyListsImport(importId: number, mode: ImportMode): Promise<ImportOutcome[]> {
  return invokeCommand("apply_lists_import", { importId, mode });
}
