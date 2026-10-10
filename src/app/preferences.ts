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
