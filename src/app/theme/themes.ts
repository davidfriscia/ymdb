/**
 * Theme registry. Each theme is a complete set of the semantic tokens defined
 * in `themes.css` under `:root[data-theme="<id>"], [data-theme-preview="<id>"]`.
 * To add a theme, define its tokens there and register it here: Settings →
 * Appearance lists registered themes, with a preview drawn from their tokens.
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

export function isThemeId(value: unknown): value is ThemeId {
  return THEMES.some((theme) => theme.id === value);
}

/** Applies a theme to the whole document. */
export function applyTheme(theme: ThemeId, root: HTMLElement = document.documentElement): void {
  root.dataset.theme = theme;
}
