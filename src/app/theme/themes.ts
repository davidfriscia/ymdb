import { THEME_PROPERTY_NAMES, themeProperties, type ThemeTokens } from "./themeTokens";

/**
 * Theme registry. Each built-in theme is a complete set of the semantic
 * tokens defined in `themes.css` under
 * `:root[data-theme="<id>"], [data-theme-preview="<id>"]`, and is described
 * in the editable token model in `builtinThemeTokens.ts`. To add a theme,
 * define it in both places and register it here: Settings → Appearance lists
 * registered themes, with a preview drawn from their tokens.
 */
export const THEMES = [
  { id: "light", label: "Light", description: "Bright neutral surfaces with a blue accent." },
  { id: "dark", label: "Dark", description: "Dark neutral surfaces with a light blue accent." },
  {
    id: "neon80s",
    label: "Neon 80s",
    description: "Synthwave arcade: dark violet surfaces, fuchsia and cyan neon accents.",
  },
] as const;

export type ThemeId = (typeof THEMES)[number]["id"];

export const DEFAULT_THEME: ThemeId = "light";

/** A custom theme, by its stable id (renaming never changes it). */
export type CustomThemeSelection = `custom:${string}`;

/** The theme chosen in Settings → Appearance: built-in or custom. */
export type ThemeSelection = ThemeId | CustomThemeSelection;

const CUSTOM_PREFIX = "custom:";
const CUSTOM_ID = /^[0-9a-f]{32}$/;

export function isThemeId(value: unknown): value is ThemeId {
  return THEMES.some((theme) => theme.id === value);
}

export function customSelection(id: string): CustomThemeSelection {
  return `${CUSTOM_PREFIX}${id}`;
}

/** The custom theme id of a selection, or `null` for a built-in theme. */
export function customThemeIdOf(selection: ThemeSelection): string | null {
  return selection.startsWith(CUSTOM_PREFIX) ? selection.slice(CUSTOM_PREFIX.length) : null;
}

/**
 * Whether a stored value is a theme selection. A custom selection is only
 * checked for its form here: whether the theme exists is known once the
 * custom themes are loaded.
 */
export function isThemeSelection(value: unknown): value is ThemeSelection {
  return (
    isThemeId(value) ||
    (typeof value === "string" &&
      value.startsWith(CUSTOM_PREFIX) &&
      CUSTOM_ID.test(value.slice(CUSTOM_PREFIX.length)))
  );
}

/** `data-theme` of a document showing a custom theme (no rule of `themes.css` matches it). */
export const CUSTOM_THEME_ATTRIBUTE = "custom";

/** Applies a built-in theme to the whole document, removing any custom values. */
export function applyTheme(theme: ThemeId, root: HTMLElement = document.documentElement): void {
  for (const name of THEME_PROPERTY_NAMES) root.style.removeProperty(name);
  root.dataset.theme = theme;
}

/**
 * Applies a custom theme to the whole document: its generated properties,
 * set inline on the root, override the default rule of `themes.css`. Every
 * token is set, so nothing of a previous theme remains.
 */
export function applyThemeTokens(
  tokens: ThemeTokens,
  root: HTMLElement = document.documentElement,
): void {
  for (const [name, value] of Object.entries(themeProperties(tokens))) {
    root.style.setProperty(name, value);
  }
  root.dataset.theme = CUSTOM_THEME_ATTRIBUTE;
}
