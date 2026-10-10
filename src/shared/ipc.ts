import { invoke } from "@tauri-apps/api/core";
import type { CatalogFreshness } from "../features/catalog/catalogApi";

/** Mirrors `CommandError` in `src-tauri/src/commands/mod.rs`. */
export type CommandError =
  | { kind: "settings"; message: string }
  | { kind: "notConfigured" }
  | { kind: "importInProgress" }
  | { kind: "invalidRequest"; message: string }
  | { kind: "catalogNotImported" }
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
    kind === "notConfigured" ||
    kind === "importInProgress" ||
    kind === "invalidRequest" ||
    kind === "catalogNotImported" ||
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
    case "notConfigured":
      return "No MAME executable is configured.";
    case "importInProgress":
      return "A catalog import is already running.";
    case "invalidRequest":
      return `Invalid request: ${error.message}`;
    case "catalogNotImported":
      return "No catalog has been imported yet.";
    case "softwareListsNotImported":
      return "The software lists have not been imported yet.";
    case "catalogMismatch":
      return "The catalog does not match the configured MAME executable. Import the catalog again before launching.";
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
