import { invoke } from "@tauri-apps/api/core";

/** Mirrors `CommandError` in `src-tauri/src/commands/mod.rs`. */
export type CommandError =
  | { kind: "settings"; message: string }
  | { kind: "notConfigured" }
  | { kind: "importInProgress" }
  | { kind: "invalidRequest"; message: string }
  | { kind: "catalogNotImported" }
  | { kind: "softwareListsNotImported" }
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
    case "internal":
      return `Unexpected error: ${error.message}`;
  }
}

/** Describes any error thrown by a command wrapper. */
export function describeError(error: unknown): string {
  return error instanceof CommandFailedError ? error.message : String(error);
}
