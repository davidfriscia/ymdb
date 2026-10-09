import { invokeCommand } from "../../shared/ipc";
import type { ValidationStatus } from "../mame-installation/mameApi";

// These types mirror the Rust contracts in `src-tauri/src/catalog/` and
// `src-tauri/src/commands/catalog.rs`. Keep both sides in sync.

/** Mirrors `CatalogStatistics` in `src-tauri/src/catalog/store.rs`. */
export interface CatalogStatistics {
  /** All `<machine>` entries, including devices. */
  machines: number;
  devices: number;
  bios: number;
  mechanical: number;
  /** Non-device entries that are clones of another machine. */
  clones: number;
  /** Runnable entries that are not devices or BIOS sets. */
  runnableSystems: number;
  roms: number;
  disks: number;
  /** Distinct software lists referenced by machines. */
  softwareLists: number;
}

/** Mirrors `SoftwareStatistics` in `src-tauri/src/catalog/store.rs`. */
export interface SoftwareStatistics {
  lists: number;
  software: number;
  clones: number;
  partiallySupported: number;
  unsupported: number;
  parts: number;
  dataAreas: number;
  roms: number;
  diskAreas: number;
  disks: number;
  /** Distinct list names referenced by machines but not provided by MAME. */
  missingReferencedLists: number;
}

/** Mirrors `SoftwareCatalogInfo` in `src-tauri/src/catalog/store.rs`. */
export interface SoftwareCatalogInfo {
  mameRelease: string;
  mameRevision: string;
  /** Milliseconds since the Unix epoch. */
  importedAt: number;
  importDurationMs: number;
  matchesMachineCatalog: boolean;
  statistics: SoftwareStatistics;
}

/** Mirrors `CatalogInfo` in `src-tauri/src/catalog/store.rs`. */
export interface CatalogInfo {
  schemaVersion: number;
  mameRelease: string;
  mameRevision: string;
  mameBuild: string | null;
  executablePath: string;
  /** Milliseconds since the Unix epoch. */
  importedAt: number;
  importDurationMs: number;
  /** Size of the catalog database file in bytes. */
  databaseSize: number;
  statistics: CatalogStatistics;
  /** `null` until software lists are imported for this catalog. */
  software: SoftwareCatalogInfo | null;
}

/** Mirrors `CatalogFreshness` in `src-tauri/src/catalog/freshness.rs`. */
export type CatalogFreshness =
  | "current"
  | "executableChanged"
  | "differentExecutable"
  | "executableMissing"
  | "mameNotConfigured";

/** Mirrors `ImportKind` in `src-tauri/src/commands/catalog.rs`. */
export type ImportKind = "machines" | "softwareLists";

/** Mirrors `ImportPhase` in `src-tauri/src/catalog/import.rs`. */
export type ImportPhase = "validating" | "reading" | "finalizing";

/** Mirrors `ImportError` in `src-tauri/src/catalog/import.rs`. */
export type ImportError =
  | { kind: "cancelled" }
  | { kind: "invalidMame"; status: ValidationStatus }
  | { kind: "machineCatalogRequired" }
  | {
      kind: "catalogMismatch";
      catalogRelease: string;
      catalogRevision: string;
      executableRelease: string;
      executableRevision: string;
    }
  | { kind: "launchFailed"; message: string }
  | { kind: "mameFailed"; exitCode: number | null; stderr: string }
  | { kind: "inactive"; timeoutSeconds: number }
  | { kind: "parse"; message: string }
  | { kind: "database"; message: string };

/** Mirrors `ImportStatus` in `src-tauri/src/commands/catalog.rs`. */
export type ImportStatus =
  | { state: "idle" }
  | {
      state: "running";
      kind: ImportKind;
      phase: ImportPhase;
      /** Machines or software stored so far, depending on `kind`. */
      itemsProcessed: number;
      startedAt: number;
      cancelRequested: boolean;
    }
  | {
      state: "completed";
      kind: ImportKind;
      items: number;
      durationMs: number;
      finishedAt: number;
    }
  | { state: "failed"; kind: ImportKind; error: ImportError; finishedAt: number }
  | { state: "cancelled"; kind: ImportKind; finishedAt: number };

/** Mirrors `CatalogStatus` in `src-tauri/src/commands/catalog.rs`. */
export interface CatalogStatus {
  catalog: CatalogInfo | null;
  catalogError: string | null;
  freshness: CatalogFreshness | null;
  mameConfigured: boolean;
  import: ImportStatus;
}

const call = invokeCommand;

/** Reads the catalog summary. Reads the catalog database: avoid polling it. */
export function getCatalogStatus(): Promise<CatalogStatus> {
  return call("get_catalog_status");
}

/** Cheap in-memory status of the current or last import, suitable for polling. */
export function getCatalogImportStatus(): Promise<ImportStatus> {
  return call("get_catalog_import_status");
}

/** Imports the machine catalog; replaces the whole catalog, software lists included. */
export function startCatalogImport(): Promise<ImportStatus> {
  return call("start_catalog_import");
}

/** Imports the software lists into the current machine catalog. */
export function startSoftwareListImport(): Promise<ImportStatus> {
  return call("start_software_list_import");
}

export function cancelCatalogImport(): Promise<ImportStatus> {
  return call("cancel_catalog_import");
}
