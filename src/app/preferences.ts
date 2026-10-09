import { DEFAULT_THEME, isThemeId, type ThemeId } from "./theme/themes";
import { DEFAULT_RENDERER, isRendererId } from "../features/browser/renderers/registry";
import { CARD_SIZE, type BrowserPreferences } from "../features/browser/browserPreferences";

/**
 * Interface preferences, stored in the webview's local storage. They are
 * separate from the MAME configuration and from the catalog: losing them only
 * restores the defaults.
 */
export interface UiPreferences extends BrowserPreferences {
  theme: ThemeId;
}

export const DEFAULT_PREFERENCES: UiPreferences = {
  theme: DEFAULT_THEME,
  cardSize: CARD_SIZE.default,
  filtersVisible: true,
  renderer: DEFAULT_RENDERER,
};

export const PREFERENCES_KEY = "ymdb.preferences.v1";

function validCardSize(value: unknown): number {
  return typeof value === "number" && Number.isFinite(value)
    ? Math.min(CARD_SIZE.max, Math.max(CARD_SIZE.min, Math.round(value)))
    : CARD_SIZE.default;
}

/** Reads the stored preferences; invalid or unknown values fall back to the defaults. */
export function loadPreferences(storage: Storage | null = safeStorage()): UiPreferences {
  let stored: unknown;
  try {
    const raw = storage?.getItem(PREFERENCES_KEY);
    stored = raw ? JSON.parse(raw) : null;
  } catch {
    // Unreadable storage or malformed JSON: use the defaults.
    stored = null;
  }
  if (typeof stored !== "object" || stored === null) {
    return DEFAULT_PREFERENCES;
  }
  const values = stored as Record<string, unknown>;
  return {
    theme: isThemeId(values.theme) ? values.theme : DEFAULT_PREFERENCES.theme,
    cardSize: validCardSize(values.cardSize),
    filtersVisible:
      typeof values.filtersVisible === "boolean"
        ? values.filtersVisible
        : DEFAULT_PREFERENCES.filtersVisible,
    renderer: isRendererId(values.renderer) ? values.renderer : DEFAULT_PREFERENCES.renderer,
  };
}

export function savePreferences(
  preferences: UiPreferences,
  storage: Storage | null = safeStorage(),
): void {
  try {
    storage?.setItem(PREFERENCES_KEY, JSON.stringify(preferences));
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
