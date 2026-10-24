import { invokeCommand } from "../../shared/ipc";

// These types mirror the Rust contracts in `src-tauri/src/app_data/` and
// `src-tauri/src/commands/app_data.rs`. Keep both sides in sync.

/** Mirrors `DataClass` in `app_data/inventory.rs`. */
export type DataClass = "owned" | "regenerable" | "transient" | "legacy";

/** Mirrors `DataEntry` in `app_data/inventory.rs`. */
export interface DataEntry {
  id: string;
  name: string;
  description: string;
  path: string;
  kind: "file" | "directory";
  class: DataClass;
  exists: boolean;
  size: number | null;
  files: number | null;
  inBackup: boolean;
  exclusionReason: string | null;
}

/** Mirrors `ExternalKind` in `app_data/inventory.rs`. */
export type ExternalKind = "mameExecutable" | "mediaDirectory" | "romDestination" | "privateKey";

/** Mirrors `ExternalReference` in `app_data/inventory.rs`. */
export interface ExternalReference {
  kind: ExternalKind;
  owner: string;
  path: string;
  exists: boolean;
}

/** Mirrors `ApplicationDataView` in `commands/app_data.rs`. */
export interface ApplicationDataView {
  configDir: string;
  dataDir: string;
  customDataDir: boolean;
  entries: DataEntry[];
  externalReferences: ExternalReference[];
  restorePending: boolean;
}

/** Mirrors `CategorySummary` in `app_data/restore.rs`. */
export interface CategorySummary {
  category: string;
  label: string;
  files: number;
  size: number;
}

/** Mirrors `RestorePreview` in `app_data/restore.rs`. */
export interface RestorePreview {
  archivePath: string;
  formatVersion: number;
  appVersion: string;
  createdAt: number;
  platform: { os: string; arch: string };
  samePlatform: boolean;
  totalSize: number;
  categories: CategorySummary[];
  replaced: CategorySummary[];
  externalReferences: ExternalReference[];
  notes: string[];
}

export type BackupJobKind = "export" | "importCheck";

/** Mirrors `Phase` in `app_data/mod.rs`. */
export type BackupPhase = "snapshot" | "writing" | "verifying" | "extracting" | "checking";

/** Mirrors `BackupJobStatus` in `commands/app_data.rs`. */
export type BackupJobStatus =
  | { state: "idle" }
  | {
      state: "running";
      jobId: number;
      kind: BackupJobKind;
      phase: BackupPhase;
      bytesDone: number;
      bytesTotal: number;
      cancelRequested: boolean;
    }
  | { state: "exported"; jobId: number; path: string; size: number }
  | { state: "prepared"; jobId: number; preview: RestorePreview }
  | { state: "failed"; jobId: number; kind: BackupJobKind; message: string }
  | { state: "cancelled"; jobId: number; kind: BackupJobKind };

/** Mirrors `RestoreOutcome` in `app_data/restore.rs`. */
export type RestoreOutcome =
  | {
      kind: "restored";
      createdAt: number;
      appVersion: string;
      interfacePreferences: Record<string, unknown> | null;
    }
  | { kind: "failed"; message: string; previousDataKept: boolean };

export function getApplicationData(): Promise<ApplicationDataView> {
  return invokeCommand("get_application_data");
}

/** Shows an inventory entry in the system file manager. */
export function openApplicationDataLocation(entryId: string): Promise<void> {
  return invokeCommand("open_application_data_location", { entryId });
}

/**
 * Opens the native save dialog and starts the export; `false` when the
 * dialog is cancelled. The interface preferences travel with the backup.
 */
export function startBackupExport(interfacePreferences: Record<string, string>): Promise<boolean> {
  return invokeCommand("start_backup_export", { interfacePreferences });
}

/** Opens the native file picker and starts checking the backup; `false` when cancelled. */
export function startBackupImportCheck(): Promise<boolean> {
  return invokeCommand("start_backup_import_check");
}

export function getBackupStatus(): Promise<BackupJobStatus> {
  return invokeCommand("get_backup_status");
}

export function cancelBackupJob(): Promise<void> {
  return invokeCommand("cancel_backup_job");
}

/** Forgets a finished job and removes a checked backup that is not restored. */
export function discardBackupJob(): Promise<void> {
  return invokeCommand("discard_backup_job");
}

/** Confirms the restore; YMDB restarts and does not answer on success. */
export function applyBackupImport(jobId: number): Promise<void> {
  return invokeCommand("apply_backup_import", { jobId });
}

/** The result of a restore applied when YMDB started, once. */
export function takeRestoreOutcome(): Promise<RestoreOutcome | null> {
  return invokeCommand("take_restore_outcome");
}
