import { describeStatus } from "../mame-installation/messages";
import type { CatalogFreshness, ImportError, ImportPhase } from "./catalogApi";

export function describeImportError(error: ImportError): string {
  switch (error.kind) {
    case "cancelled":
      return "The import was cancelled.";
    case "invalidMame":
      return `The configured MAME executable cannot be used: ${describeStatus(error.status)}`;
    case "launchFailed":
      return `MAME could not be started: ${error.message}`;
    case "mameFailed": {
      const code =
        error.exitCode === null ? "without an exit code" : `with code ${String(error.exitCode)}`;
      const details = error.stderr.trim() ? ` ${error.stderr.trim()}` : "";
      return `MAME exited ${code} while listing machines.${details}`;
    }
    case "inactive":
      return `MAME produced no output for ${String(error.timeoutSeconds)} seconds and was stopped.`;
    case "parse":
      return `The machine list produced by MAME could not be read: ${error.message}`;
    case "database":
      return `The catalog could not be saved: ${error.message}`;
  }
}

export function describePhase(phase: ImportPhase): string {
  switch (phase) {
    case "validating":
      return "Checking the MAME executable…";
    case "reading":
      return "Reading the machine list…";
    case "finalizing":
      return "Saving the catalog…";
  }
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
          "This catalog was imported from a different MAME executable than the one configured. Import the catalog again to match it.",
        warning: true,
      };
    case "executableMissing":
      return {
        message: "The MAME executable used for this catalog no longer exists.",
        warning: true,
      };
    case "mameNotConfigured":
      return {
        message: "Configure a MAME executable to import or update the catalog.",
        warning: false,
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
