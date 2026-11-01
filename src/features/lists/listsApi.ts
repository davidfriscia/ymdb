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

// Custom lists

/** Mirrors `ListSummary` in `commands/lists.rs`: a custom list without its entries. */
export interface ListSummary {
  id: string;
  name: string;
  /** Machine entries. */
  machines: number;
  /** All entries (machines and software). */
  entries: number;
  /** Milliseconds since the Unix epoch. */
  createdAt: number | null;
  updatedAt: number | null;
}

/** Mirrors `ListMachine` in `catalog/search/members.rs`. */
export interface ListMachine {
  name: string;
  /** From the catalog; `null` when the catalog does not have the machine. */
  description: string | null;
  /** `null` without a catalog. */
  inCatalog: boolean | null;
}

/** Mirrors `ListEntriesPage` in `commands/lists.rs`. */
export interface ListEntriesPage {
  list: ListSummary;
  /** The installation whose catalog gave the descriptions, if any. */
  installationId: string | null;
  items: ListMachine[];
  matched: number;
  page: number;
  pageSize: number;
}

/** Mirrors `Naming` in `lists/folder_import.rs`. */
export type FolderNaming = "subfolder" | "prefixed";

/** Mirrors `IniWarningKind` in `lists/ini.rs`. */
export type IniWarningKind =
  | "notUtf8"
  | "missingRootFolder"
  | "outsideSection"
  | "invalidLine"
  | "duplicateEntry"
  | "duplicateFolder"
  | "unnamedSection";

export interface IniWarning {
  kind: IniWarningKind;
  count: number;
  examples: string[];
}

/** Mirrors `FolderFileSummary` in `commands/lists.rs`. */
export interface FolderFileSummary {
  importId: number;
  fileName: string;
  suggestedName: string;
  rootEntries: number;
  folders: number;
  emptyFolders: number;
  entries: number;
  duplicates: number;
  warnings: IniWarning[];
}

/** Mirrors `PlannedListView` in `commands/lists.rs`. */
export interface PlannedList {
  name: string;
  /** `merge`: an existing list with the same name (ignoring case). */
  action: "create" | "merge";
  entries: number;
  newEntries: number;
}

/** Mirrors `FolderImportPreview` in `commands/lists.rs`. */
export interface FolderImportPreview {
  importId: number;
  fileName: string;
  name: string;
  naming: FolderNaming;
  hasSubfolders: boolean;
  toCreate: number;
  toMerge: number;
  emptyFolders: number;
  newEntries: number;
  existingEntries: number;
  duplicates: number;
  warnings: IniWarning[];
  lists: PlannedList[];
}

/** Mirrors `FolderImportOutcome` in `lists/folder_import.rs`. */
export interface FolderImportOutcome {
  created: number;
  merged: number;
  entriesAdded: number;
}

/** The wanted membership of an entry in one custom list. */
export interface MembershipChange {
  listId: string;
  member: boolean;
}

/** Names are unique ignoring case, as in `name_key` (`lists/model.rs`). */
export function listNameKey(name: string): string {
  return name.trim().toLowerCase();
}

export function listUserLists(): Promise<ListSummary[]> {
  return invokeCommand("list_user_lists");
}

export function createList(name: string): Promise<ListSummary> {
  return invokeCommand("create_list", { name });
}

export function renameList(listId: string, name: string): Promise<ListSummary> {
  return invokeCommand("rename_list", { listId, name });
}

export function deleteList(listId: string): Promise<void> {
  return invokeCommand("delete_list", { listId });
}

export function getListEntries(
  listId: string,
  installationId: string | null,
  search: string,
  page: number,
  pageSize: number,
): Promise<ListEntriesPage> {
  return invokeCommand("get_list_entries", {
    listId,
    installationId,
    search: search.trim() === "" ? null : search.trim(),
    page,
    pageSize,
  });
}

/** Ids of the custom lists containing the entry. */
export function getEntryLists(entry: ListEntry): Promise<string[]> {
  return invokeCommand("get_entry_lists", { entry });
}

/**
 * Sets the membership of the entry in the named lists in one saved change;
 * resolves to the ids of the custom lists containing it afterwards.
 */
export function setEntryMemberships(
  entry: ListEntry,
  changes: MembershipChange[],
): Promise<string[]> {
  return invokeCommand("set_entry_memberships", { entry, changes });
}

/** Opens the native file picker and reads the file; `null` when cancelled. */
export function chooseFolderImportFile(): Promise<FolderFileSummary | null> {
  return invokeCommand("choose_folder_import_file");
}

export function previewFolderImport(
  importId: number,
  name: string,
  naming: FolderNaming,
): Promise<FolderImportPreview> {
  return invokeCommand("preview_folder_import", { importId, name, naming });
}

export function applyFolderImport(
  importId: number,
  name: string,
  naming: FolderNaming,
): Promise<FolderImportOutcome> {
  return invokeCommand("apply_folder_import", { importId, name, naming });
}
