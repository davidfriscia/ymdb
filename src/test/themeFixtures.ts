import { BUILTIN_THEME_TOKENS } from "../app/theme/builtinThemeTokens";
import { TOKENS_VERSION, copyTokens, type ThemeTokens } from "../app/theme/themeTokens";
import type { CustomTheme } from "../features/themes/themesApi";

export const THEME_ID = "0123456789abcdef0123456789abcdef";
export const OTHER_THEME_ID = "fedcba9876543210fedcba9876543210";

/** Neon 80s with an orange accent. */
export function sunsetTokens(): ThemeTokens {
  const tokens = copyTokens(BUILTIN_THEME_TOKENS.neon80s);
  tokens.colors.accent = "#ff8800";
  return tokens;
}

export function customTheme(overrides: Partial<CustomTheme> = {}): CustomTheme {
  return {
    id: THEME_ID,
    name: "Sunset",
    sourceThemeId: "neon80s",
    tokensVersion: TOKENS_VERSION,
    tokens: sunsetTokens(),
    createdAt: 1,
    updatedAt: 1,
    ...overrides,
  };
}
