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
}

/** Mirrors `CatalogFreshness` in `src-tauri/src/catalog/freshness.rs`. */
export type CatalogFreshness =
  | "current"
  | "executableChanged"
  | "differentExecutable"
  | "executableMissing"
  | "mameNotConfigured";

/** Mirrors `ImportPhase` in `src-tauri/src/catalog/import.rs`. */
export type ImportPhase = "validating" | "reading" | "finalizing";

/** Mirrors `ImportError` in `src-tauri/src/catalog/import.rs`. */
export type ImportError =
  | { kind: "cancelled" }
  | { kind: "invalidMame"; status: ValidationStatus }
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
      phase: ImportPhase;
      machinesProcessed: number;
      startedAt: number;
      cancelRequested: boolean;
    }
  | { state: "completed"; machines: number; durationMs: number; finishedAt: number }
  | { state: "failed"; error: ImportError; finishedAt: number }
  | { state: "cancelled"; finishedAt: number };

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

export function startCatalogImport(): Promise<ImportStatus> {
  return call("start_catalog_import");
}

export function cancelCatalogImport(): Promise<ImportStatus> {
  return call("cancel_catalog_import");
}
