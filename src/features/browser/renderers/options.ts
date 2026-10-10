import type { RendererOptionDefinition, RendererOptionValues } from "./types";

/** Default values of a renderer's options. */
export function defaultOptions(
  definitions: readonly RendererOptionDefinition[],
): RendererOptionValues {
  return Object.fromEntries(definitions.map((definition) => [definition.key, definition.default]));
}

/**
 * Valid values for every option: stored values are kept when valid (ranges
 * are clamped to their bounds and steps), anything else falls back to the
 * default. Unknown keys are dropped.
 */
export function normalizeOptions(
  definitions: readonly RendererOptionDefinition[],
  raw: unknown,
): RendererOptionValues {
  const stored = typeof raw === "object" && raw !== null ? (raw as Record<string, unknown>) : {};
  return Object.fromEntries(
    definitions.map((definition): [string, number | boolean | string] => {
      const value = stored[definition.key];
      switch (definition.type) {
        case "range": {
          if (typeof value !== "number" || !Number.isFinite(value)) {
            return [definition.key, definition.default];
          }
          const stepped =
            definition.min +
            Math.round((value - definition.min) / definition.step) * definition.step;
          return [definition.key, Math.min(definition.max, Math.max(definition.min, stepped))];
        }
        case "toggle":
          return [definition.key, typeof value === "boolean" ? value : definition.default];
        case "choice":
          return [
            definition.key,
            definition.choices.some((choice) => choice.value === value)
              ? (value as string)
              : definition.default,
          ];
      }
    }),
  );
}

export function numberOption(options: RendererOptionValues, key: string): number {
  const value = options[key];
  return typeof value === "number" ? value : 0;
}

export function booleanOption(options: RendererOptionValues, key: string): boolean {
  return options[key] === true;
}

export function stringOption(options: RendererOptionValues, key: string): string {
  const value = options[key];
  return typeof value === "string" ? value : "";
}
