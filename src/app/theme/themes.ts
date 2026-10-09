/**
 * Theme registry. Each theme is a set of CSS custom properties defined in
 * `themes.css` under `:root[data-theme="<id>"]`. To add a theme, define its
 * tokens there and register it here.
 */
export const THEMES = [
  { id: "light", label: "Light" },
  { id: "dark", label: "Dark" },
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
