import { isThemeId, type ThemeId } from "../../app/theme/themes";
import {
  TOKENS_VERSION,
  copyTokens,
  parseThemeTokens,
  type ThemeTokens,
} from "../../app/theme/themeTokens";
import { MAX_THEME_NAME_LENGTH } from "./themeNames";

/**
 * The portable theme file (`.ymdb-theme.json`). It holds a name, the tokens
 * and a few metadata: never local paths, application data, CSS or code.
 * An imported theme always becomes a new, independent custom theme.
 */
export const THEME_FILE_FORMAT = "ymdb-theme";
export const THEME_FILE_VERSION = 1;

export interface ThemeDocument {
  format: typeof THEME_FILE_FORMAT;
  formatVersion: number;
  name: string;
  /** The built-in theme it derives from, when there is one. */
  basedOn?: ThemeId;
  tokensVersion: number;
  tokens: ThemeTokens;
  /** ISO 8601 date of the export. */
  exportedAt: string;
  /** Version of the YMDB that exported it. */
  appVersion: string;
}

export function buildThemeDocument(
  name: string,
  tokens: ThemeTokens,
  basedOn: string | undefined,
  now: Date = new Date(),
  appVersion: string = __APP_VERSION__,
): ThemeDocument {
  return {
    format: THEME_FILE_FORMAT,
    formatVersion: THEME_FILE_VERSION,
    name,
    ...(isThemeId(basedOn) ? { basedOn } : {}),
    tokensVersion: TOKENS_VERSION,
    tokens: copyTokens(tokens),
    exportedAt: now.toISOString(),
    appVersion,
  };
}

export type ParsedThemeFile =
  | { ok: true; name: string; basedOn: ThemeId | null; tokens: ThemeTokens }
  | { ok: false; error: string };

/**
 * Checks an imported document. Its metadata may grow in later versions of
 * the same format; its tokens must be exactly valid.
 */
export function parseThemeDocument(document: unknown): ParsedThemeFile {
  if (typeof document !== "object" || document === null || Array.isArray(document)) {
    return { ok: false, error: "The file is not a YMDB theme." };
  }
  const value = document as Record<string, unknown>;
  if (value.format !== THEME_FILE_FORMAT) {
    return { ok: false, error: "The file is not a YMDB theme." };
  }
  const { formatVersion, tokensVersion } = value;
  if (typeof formatVersion !== "number" || !Number.isInteger(formatVersion) || formatVersion < 1) {
    return { ok: false, error: "The theme file has no valid format version." };
  }
  if (
    formatVersion > THEME_FILE_VERSION ||
    (typeof tokensVersion === "number" && tokensVersion > TOKENS_VERSION)
  ) {
    return {
      ok: false,
      error: "The theme was exported by a newer version of YMDB and cannot be imported.",
    };
  }
  const tokens = parseThemeTokens(tokensVersion, value.tokens);
  if (!tokens) {
    return {
      ok: false,
      error: "The theme file contains missing or invalid colors or effects.",
    };
  }
  const rawName = typeof value.name === "string" ? value.name.trim() : "";
  const name =
    rawName && !/\p{Cc}/u.test(rawName)
      ? Array.from(rawName).slice(0, MAX_THEME_NAME_LENGTH).join("").trim()
      : "Imported theme";
  return { ok: true, name, basedOn: isThemeId(value.basedOn) ? value.basedOn : null, tokens };
}
