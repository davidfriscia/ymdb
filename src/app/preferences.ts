import { DEFAULT_THEME, isThemeId, type ThemeId } from "./theme/themes";
import {
  DEFAULT_BROWSER_PREFERENCES,
  parseBrowserPreferences,
  type BrowserPreferences,
} from "../features/browser/browserPreferences";

/**
 * Interface preferences, stored in the webview's local storage. They are
 * separate from the MAME, launcher and media configuration and from the
 * catalog: losing them only restores the defaults.
 */
export interface UiPreferences extends BrowserPreferences {
  theme: ThemeId;
  /** Whether launch panels show the MAME command line. */
  showCommandPreview: boolean;
}

export const DEFAULT_PREFERENCES: UiPreferences = {
  theme: DEFAULT_THEME,
  ...DEFAULT_BROWSER_PREFERENCES,
  showCommandPreview: false,
};

export const PREFERENCES_KEY = "ymdb.preferences.v2";
/** Format of sessions 7-8: one card size and the `cards` renderer. */
export const LEGACY_PREFERENCES_KEY = "ymdb.preferences.v1";

/** Converts version 1 preferences to the version 2 shape. */
function migrateV1(values: Record<string, unknown>): Record<string, unknown> {
  return {
    theme: values.theme,
    filtersVisible: values.filtersVisible,
    showCommandPreview: values.showCommandPreview,
    renderer: values.renderer === "cards" ? "classicCards" : values.renderer,
    rendererOptions: { classicCards: { cardSize: values.cardSize } },
  };
}

function read(storage: Storage | null, key: string): Record<string, unknown> | null {
  try {
    const raw = storage?.getItem(key);
    const parsed: unknown = raw ? JSON.parse(raw) : null;
    return typeof parsed === "object" && parsed !== null && !Array.isArray(parsed)
      ? (parsed as Record<string, unknown>)
      : null;
  } catch {
    // Unreadable storage or malformed JSON: use the defaults.
    return null;
  }
}

/**
 * Reads the stored preferences, migrating version 1 when there is no version
 * 2 yet. Invalid or unknown values fall back to the defaults, field by field.
 */
export function loadPreferences(storage: Storage | null = safeStorage()): UiPreferences {
  const current = read(storage, PREFERENCES_KEY);
  const legacy = current ? null : read(storage, LEGACY_PREFERENCES_KEY);
  const values = current ?? (legacy ? migrateV1(legacy) : null);
  if (!values) return DEFAULT_PREFERENCES;
  return {
    theme: isThemeId(values.theme) ? values.theme : DEFAULT_PREFERENCES.theme,
    ...parseBrowserPreferences(values),
    showCommandPreview:
      typeof values.showCommandPreview === "boolean"
        ? values.showCommandPreview
        : DEFAULT_PREFERENCES.showCommandPreview,
  };
}

/** Saves the preferences in the current format and drops the legacy entry. */
export function savePreferences(
  preferences: UiPreferences,
  storage: Storage | null = safeStorage(),
): void {
  try {
    storage?.setItem(PREFERENCES_KEY, JSON.stringify(preferences));
    storage?.removeItem(LEGACY_PREFERENCES_KEY);
  } catch {
    // Storage unavailable or full: the preferences only last for this session.
  }
}

function safeStorage(): Storage | null {
  try {
    return window.localStorage;
  } catch {
    return null;
  }
}

/**
 * The stored interface preferences as raw entries, for application data
 * backups (the webview storage is not a file YMDB can copy).
 */
export function exportStoredPreferences(
  storage: Storage | null = safeStorage(),
): Record<string, string> {
  const entries: Record<string, string> = {};
  for (const key of [PREFERENCES_KEY, LEGACY_PREFERENCES_KEY]) {
    try {
      const value = storage?.getItem(key);
      if (typeof value === "string") entries[key] = value;
    } catch {
      // Unreadable storage: the backup has no interface preferences.
    }
  }
  return entries;
}

/**
 * Replaces the stored interface preferences with entries from a restored
 * backup. Unknown keys and non-string values are ignored; the values are
 * validated when read, like any stored preferences.
 */
export function importStoredPreferences(
  entries: Record<string, unknown>,
  storage: Storage | null = safeStorage(),
): void {
  try {
    for (const key of [PREFERENCES_KEY, LEGACY_PREFERENCES_KEY]) {
      const value = entries[key];
      if (typeof value === "string") storage?.setItem(key, value);
      else storage?.removeItem(key);
    }
  } catch {
    // Storage unavailable: the defaults apply.
  }
}
