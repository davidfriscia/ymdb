import { parseThemeTokens } from "../../app/theme/themeTokens";
import {
  DEFAULT_THEME,
  applyTheme,
  applyThemeTokens,
  customThemeIdOf,
  isThemeId,
  type ThemeSelection,
} from "../../app/theme/themes";
import { getCustomThemes } from "./themesApi";

/** How long the first render waits for the custom themes. */
const STARTUP_TIMEOUT_MS = 1500;

/**
 * Applies the saved theme before the first render. A built-in theme applies
 * at once; a custom theme waits briefly for the custom themes, and falls back
 * to the default theme (without changing the preference) when it cannot be
 * read in time or no longer exists. The shell applies it again once loaded.
 */
export async function applyStartupTheme(
  selection: ThemeSelection,
  root: HTMLElement = document.documentElement,
  load: () => Promise<{ id: string; tokensVersion: number; tokens: unknown }[]> = getCustomThemes,
): Promise<void> {
  const id = customThemeIdOf(selection);
  if (id === null) {
    applyTheme(isThemeId(selection) ? selection : DEFAULT_THEME, root);
    return;
  }
  try {
    const themes = await Promise.race([
      load(),
      new Promise<never>((_, reject) =>
        setTimeout(() => {
          reject(new Error("timeout"));
        }, STARTUP_TIMEOUT_MS),
      ),
    ]);
    const theme = themes.find((entry) => entry.id === id);
    const tokens = theme ? parseThemeTokens(theme.tokensVersion, theme.tokens) : null;
    if (tokens) {
      applyThemeTokens(tokens, root);
      return;
    }
  } catch {
    // Unreadable custom themes: the default theme applies, as below.
  }
  applyTheme(DEFAULT_THEME, root);
}
