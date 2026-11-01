import { createContext, useContext } from "react";
import { BUILTIN_THEME_TOKENS } from "../../app/theme/builtinThemeTokens";
import {
  DEFAULT_THEME,
  THEMES,
  customThemeIdOf,
  isThemeId,
  type ThemeId,
  type ThemeSelection,
} from "../../app/theme/themes";
import type { ThemeTokens } from "../../app/theme/themeTokens";
import type { CustomTheme } from "./themesApi";

/** A stored custom theme with its validated tokens. */
export interface CustomThemeEntry {
  theme: CustomTheme;
  /**
   * `null` when the tokens are of another format version (a newer YMDB) or
   * invalid: the theme is kept as it is, but cannot be applied or edited.
   */
  tokens: ThemeTokens | null;
}

export interface CustomThemesContextValue {
  /** The custom themes in creation order; `null` until loaded (or on error). */
  themes: readonly CustomThemeEntry[] | null;
  /** Set when the custom themes could not be read. */
  loadError: unknown;
  byId: (id: string) => CustomThemeEntry | undefined;
  /** Keeps the themes returned by a saved change. */
  replace: (themes: CustomTheme[]) => void;
  /** Reads the custom themes again. */
  reload: () => void;
}

export const CustomThemesContext = createContext<CustomThemesContextValue | null>(null);

export function useCustomThemes(): CustomThemesContextValue {
  const value = useContext(CustomThemesContext);
  if (!value) throw new Error("useCustomThemes must be used inside CustomThemesProvider");
  return value;
}

/** What the document shows for a theme selection. */
export type ResolvedTheme =
  | { kind: "builtin"; id: ThemeId; unavailable: boolean }
  | { kind: "custom"; id: string; tokens: ThemeTokens }
  | { kind: "pending" };

/**
 * Resolves a selection. A custom theme that does not exist, cannot be used
 * or cannot be read shows the default theme (`unavailable`), without
 * changing the selection; while the custom themes load nothing changes.
 */
export function resolveTheme(
  selection: ThemeSelection,
  themes: readonly CustomThemeEntry[] | null,
  loadFailed: boolean,
): ResolvedTheme {
  const id = customThemeIdOf(selection);
  if (id === null) {
    return {
      kind: "builtin",
      id: isThemeId(selection) ? selection : DEFAULT_THEME,
      unavailable: false,
    };
  }
  if (themes === null && !loadFailed) return { kind: "pending" };
  const tokens = themes?.find((entry) => entry.theme.id === id)?.tokens;
  return tokens
    ? { kind: "custom", id, tokens }
    : { kind: "builtin", id: DEFAULT_THEME, unavailable: true };
}

/** A theme as a source of clones and exports, built-in or custom. */
export interface ThemeSource {
  /** `light`, `dark`, `neon80s` or `custom:<id>`. */
  selection: ThemeSelection;
  name: string;
  builtin: boolean;
  tokens: ThemeTokens | null;
}

export function builtinSources(): ThemeSource[] {
  return THEMES.map((theme) => ({
    selection: theme.id,
    name: theme.label,
    builtin: true,
    tokens: BUILTIN_THEME_TOKENS[theme.id],
  }));
}
