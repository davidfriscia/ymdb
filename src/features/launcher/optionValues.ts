import type { OptionCategory, OptionKind, OptionValue } from "./launcherApi";

/**
 * Text editing of option values. The backend validates every value again; these
 * checks only give immediate feedback.
 */

export const CATEGORY_LABELS: Record<OptionCategory, string> = {
  display: "Display",
  performance: "Performance",
  sound: "Sound",
  paths: "Paths",
  machine: "Machine",
  session: "Session",
};

/** Value used when an option is added. */
export function initialValue(kind: OptionKind): OptionValue {
  switch (kind.type) {
    case "boolean":
      return true;
    case "integer":
    case "float":
      return Math.min(Math.max(kind.type === "float" ? 1 : 0, kind.min), kind.max);
    case "identifier":
    case "path":
      return "";
    case "pathList":
      return [];
  }
}

/** The text shown in an input for a value. */
export function valueToText(value: OptionValue | undefined): string {
  if (value === undefined) return "";
  if (Array.isArray(value)) return value.join("\n");
  return String(value);
}

/** Parses edited text; returns an error message when it is not valid. */
export function parseValue(
  kind: OptionKind,
  text: string,
): { value: OptionValue } | { error: string } {
  switch (kind.type) {
    case "boolean":
      return { value: text === "true" };
    case "integer":
    case "float": {
      const number = Number(text.trim());
      const integer = kind.type === "integer";
      if (
        text.trim() === "" ||
        !Number.isFinite(number) ||
        (integer && !Number.isInteger(number)) ||
        number < kind.min ||
        number > kind.max
      ) {
        return {
          error: `Enter ${integer ? "an integer" : "a number"} from ${String(kind.min)} to ${String(kind.max)}.`,
        };
      }
      return { value: number };
    }
    case "identifier":
      return /^[A-Za-z0-9_.][A-Za-z0-9_.-]{0,63}$/.test(text)
        ? { value: text }
        : { error: "Use letters, digits, '_', '.' or '-' (not first)." };
    case "path": {
      const error = pathError(text);
      return error ? { error } : { value: text };
    }
    case "pathList": {
      const paths = text
        .split("\n")
        .map((line) => line.trim())
        .filter((line) => line !== "");
      if (paths.length === 0) return { error: "Enter at least one path, one per line." };
      const error = paths.map(pathError).find((message) => message !== null);
      return error ? { error } : { value: paths };
    }
  }
}

function pathError(path: string): string | null {
  if (path.trim() === "") return "Enter a path.";
  if (path.startsWith("-")) return "Paths must not start with '-'.";
  if (path.includes(";")) return "Paths must not contain ';'.";
  return null;
}

/** A short readable form of a value, for summaries. */
export function describeValue(value: OptionValue): string {
  if (typeof value === "boolean") return value ? "On" : "Off";
  if (Array.isArray(value)) return value.join("; ");
  return String(value);
}

/** A copy of `record` without `key`. */
export function omit<T>(
  record: Partial<Record<string, T>>,
  key: string,
): Partial<Record<string, T>> {
  return Object.fromEntries(Object.entries(record).filter(([name]) => name !== key));
}
