import { invokeCommand } from "../../shared/ipc";
import type {
  DetectedInstallation,
  MameVersion,
  ValidationReport,
} from "../mame-installation/mameApi";

// These types mirror the Rust contracts in `src-tauri/src/installations/` and
// `src-tauri/src/commands/installations.rs`. Keep both sides in sync.

/** Mirrors `ExecutableStatus` in `installations/registry.rs`. */
export type ExecutableStatus = "verified" | "notVerified" | "changed" | "missing";

/** Mirrors `InstallationSummary` in `commands/installations.rs`. */
export interface InstallationSummary {
  /** Opaque, stable id; never the version. */
  id: string;
  /** Display name, or the version when none is set. */
  label: string;
  displayName: string | null;
  executablePath: string;
  version: MameVersion | null;
  /** Milliseconds since the Unix epoch. */
  addedAt: number;
  verifiedAt: number | null;
  executableStatus: ExecutableStatus;
  /** Whether a catalog has been imported for this installation. */
  catalogImported: boolean;
  active: boolean;
}

/** Mirrors `InstallationsView` in `commands/installations.rs`. */
export interface InstallationsView {
  /** In registration order. */
  installations: InstallationSummary[];
  activeInstallationId: string | null;
}

/** Mirrors `InstallationValidation` in `commands/installations.rs`. */
export interface InstallationValidation {
  report: ValidationReport;
  /** The registered installation; `null` when an executable to add is invalid. */
  installation: InstallationSummary | null;
}

/** Mirrors `RemovalResult` in `commands/installations.rs`. */
export interface RemovalResult {
  removedInstallationId: string;
  activeInstallationId: string | null;
  /** Directories of the removed installation kept by choice. */
  keptPaths: string[];
}

/** Mirrors `DetectedCandidate` in `commands/mame.rs`. */
export interface DetectedCandidate extends DetectedInstallation {
  registeredInstallationId: string | null;
}

const call = invokeCommand;

export function listInstallations(): Promise<InstallationsView> {
  return call("list_installations");
}

/** Validates the executable (runs `-version`) and registers it when valid. */
export function addInstallation(path: string): Promise<InstallationValidation> {
  return call("add_installation", { path });
}

/** Runs `-version` again; a valid result updates the recorded version. */
export function verifyInstallation(installationId: string): Promise<InstallationValidation> {
  return call("verify_installation", { installationId });
}

/** Sets the display name; `null` or a blank name shows the version again. */
export function renameInstallation(
  installationId: string,
  displayName: string | null,
): Promise<InstallationSummary> {
  return call("rename_installation", { installationId, displayName });
}

/** Selects the installation browsed in the catalog and used by new launches. */
export function setActiveInstallation(installationId: string): Promise<InstallationsView> {
  return call("set_active_installation", { installationId });
}

/**
 * Unregisters an installation. Its catalog and launch options are deleted
 * only when requested; MAME's own files are never touched.
 */
export function removeInstallation(
  installationId: string,
  deleteCatalog: boolean,
  deleteLaunchConfig: boolean,
): Promise<RemovalResult> {
  return call("remove_installation", { installationId, deleteCatalog, deleteLaunchConfig });
}

/** Opens the native file picker. Resolves to `null` when the user cancels. */
export function pickMameExecutable(): Promise<string | null> {
  return call("pick_mame_executable");
}

/** Searches well-known locations. Never changes the registered installations. */
export function detectMameInstallations(): Promise<DetectedCandidate[]> {
  return call("detect_mame_installations");
}

/** The label, with the version when the user named the installation. */
export function describeInstallation(installation: InstallationSummary): string {
  return installation.displayName !== null && installation.version !== null
    ? `${installation.displayName} (MAME ${installation.version.release})`
    : installation.label;
}

/**
 * Names shown in lists: {@link describeInstallation}, with the executable's
 * folder when several installations share it (for example two builds of the
 * same version).
 */
export function distinctLabels(installations: InstallationSummary[]): Map<string, string> {
  const counts = new Map<string, number>();
  for (const installation of installations) {
    const name = describeInstallation(installation);
    counts.set(name, (counts.get(name) ?? 0) + 1);
  }
  return new Map(
    installations.map((installation) => {
      const name = describeInstallation(installation);
      return [
        installation.id,
        (counts.get(name) ?? 0) > 1 ? `${name} · ${folderOf(installation.executablePath)}` : name,
      ];
    }),
  );
}

function folderOf(path: string): string {
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts.length > 1 ? (parts.at(-2) ?? path) : path;
}
