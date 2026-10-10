import { describeStatus } from "../mame-installation/messages";
import type { CatalogFreshness, ImportError, ImportKind, ImportPhase } from "./catalogApi";

export function describeImportError(error: ImportError): string {
  switch (error.kind) {
    case "cancelled":
      return "The import was cancelled.";
    case "invalidMame":
      return `The MAME executable of this installation cannot be used: ${describeStatus(error.status)}`;
    case "machineCatalogRequired":
      return "Import the machine catalog before importing software lists.";
    case "catalogMismatch":
      return error.catalogRelease === error.executableRelease &&
        error.catalogRevision === error.executableRevision
        ? "The MAME executable has changed since the machine catalog was imported. Reimport the machine catalog first."
        : `The machine catalog comes from MAME ${error.catalogRelease} (${error.catalogRevision}) but the executable is MAME ${error.executableRelease} (${error.executableRevision}). Reimport the machine catalog first.`;
    case "launchFailed":
      return `MAME could not be started: ${error.message}`;
    case "mameFailed": {
      const code =
        error.exitCode === null ? "without an exit code" : `with code ${String(error.exitCode)}`;
      const details = error.stderr.trim() ? ` ${error.stderr.trim()}` : "";
      return `MAME exited ${code}.${details}`;
    }
    case "inactive":
      return `MAME produced no output for ${String(error.timeoutSeconds)} seconds and was stopped.`;
    case "parse":
      return `The list produced by MAME could not be read: ${error.message}`;
    case "database":
      return `The catalog could not be saved: ${error.message}`;
  }
}

export function describePhase(kind: ImportKind, phase: ImportPhase): string {
  switch (phase) {
    case "validating":
      return "Checking the MAME executable…";
    case "reading":
      return kind === "machines" ? "Reading the machine list…" : "Reading the software lists…";
    case "finalizing":
      return "Saving the catalog…";
  }
}

/** Names the items counted by an import of the given kind. */
export function itemsLabel(kind: ImportKind): string {
  return kind === "machines" ? "machines" : "software";
}

/** Names what an import of the given kind produces. */
export function importSubject(kind: ImportKind): string {
  return kind === "machines" ? "Machine catalog" : "Software lists";
}

/** Returns a message and whether it is a warning, or `null` when up to date. */
export function describeFreshness(
  freshness: CatalogFreshness,
): { message: string; warning: boolean } | null {
  switch (freshness) {
    case "current":
      return null;
    case "executableChanged":
      return {
        message:
          "The MAME executable has changed since this catalog was imported. Import the catalog again to update it.",
        warning: true,
      };
    case "differentExecutable":
      return {
        message:
          "This catalog was imported from a different MAME executable than the one of this installation. Import the catalog again to match it.",
        warning: true,
      };
    case "executableMissing":
      return {
        message: "The MAME executable used for this catalog no longer exists.",
        warning: true,
      };
  }
}

const numberFormat = new Intl.NumberFormat("en-US");

export function formatCount(value: number): string {
  return numberFormat.format(value);
}

export function formatBytes(bytes: number): string {
  const megabytes = bytes / (1024 * 1024);
  return `${megabytes.toFixed(1)} MB`;
}

export function formatDuration(milliseconds: number): string {
  return `${(milliseconds / 1000).toFixed(1)} s`;
}
