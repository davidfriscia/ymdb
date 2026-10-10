import { invoke } from "@tauri-apps/api/core";
import type { CatalogFreshness } from "../features/catalog/catalogApi";

/** Mirrors `CommandError` in `src-tauri/src/commands/mod.rs`. */
export type CommandError =
  | { kind: "settings"; message: string }
  | { kind: "installationNotFound"; installationId: string }
  | { kind: "duplicateInstallation"; existingInstallationId: string }
  | { kind: "removalNotAllowed"; message: string }
  | { kind: "migrationFailed"; message: string }
  | { kind: "importInProgress" }
  | { kind: "invalidRequest"; message: string }
  | { kind: "catalogNotImported" }
  | { kind: "catalogUnreadable"; message: string }
  | { kind: "softwareListsNotImported" }
  | { kind: "catalogMismatch"; freshness: CatalogFreshness }
  | { kind: "notFound"; message: string }
  | { kind: "alreadyRunning"; sessionId: number }
  | { kind: "launchFailed"; message: string }
  | { kind: "internal"; message: string };

/** Thrown by {@link invokeCommand} when a Tauri command fails. */
export class CommandFailedError extends Error {
  readonly detail: CommandError;

  constructor(detail: CommandError) {
    super(describeCommandError(detail));
    this.name = "CommandFailedError";
    this.detail = detail;
  }
}

function isCommandError(value: unknown): value is CommandError {
  if (typeof value !== "object" || value === null || !("kind" in value)) {
    return false;
  }
  const { kind } = value;
  return (
    kind === "settings" ||
    kind === "installationNotFound" ||
    kind === "duplicateInstallation" ||
    kind === "removalNotAllowed" ||
    kind === "migrationFailed" ||
    kind === "importInProgress" ||
    kind === "invalidRequest" ||
    kind === "catalogNotImported" ||
    kind === "catalogUnreadable" ||
    kind === "softwareListsNotImported" ||
    kind === "catalogMismatch" ||
    kind === "notFound" ||
    kind === "alreadyRunning" ||
    kind === "launchFailed" ||
    kind === "internal"
  );
}

/**
 * Invokes a Tauri command. Feature API modules wrap this function so that
 * components never call `invoke` directly.
 */
export async function invokeCommand<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    throw new CommandFailedError(
      isCommandError(error) ? error : { kind: "internal", message: String(error) },
    );
  }
}

export function describeCommandError(error: CommandError): string {
  switch (error.kind) {
    case "settings":
      return `Settings error: ${error.message}`;
    case "installationNotFound":
      return "This MAME installation is no longer registered.";
    case "duplicateInstallation":
      return "This MAME executable is already registered.";
    case "removalNotAllowed":
      return `The installation cannot be removed now: ${error.message}`;
    case "migrationFailed":
      return `The previous YMDB configuration could not be upgraded. Nothing was deleted; YMDB retries on the next attempt. ${error.message}`;
    case "importInProgress":
      return "A catalog import is already running.";
    case "invalidRequest":
      return `Invalid request: ${error.message}`;
    case "catalogNotImported":
      return "No catalog has been imported yet for this MAME installation.";
    case "catalogUnreadable":
      return `The catalog cannot be read: ${error.message}`;
    case "softwareListsNotImported":
      return "The software lists have not been imported yet.";
    case "catalogMismatch":
      return "The catalog does not match the MAME executable of this installation. Import the catalog again before launching.";
    case "notFound":
      return `Not found: ${error.message}`;
    case "alreadyRunning":
      return "This machine or software is already running.";
    case "launchFailed":
      return `MAME could not be started: ${error.message}`;
    case "internal":
      return `Unexpected error: ${error.message}`;
  }
}

/** Describes any error thrown by a command wrapper. */
export function describeError(error: unknown): string {
  return error instanceof CommandFailedError ? error.message : String(error);
}
