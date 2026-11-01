import { useCallback, useMemo, useState, type ReactNode } from "react";
import { parseThemeTokens } from "../../app/theme/themeTokens";
import { useLoader } from "../../shared/useLoader";
import {
  CustomThemesContext,
  type CustomThemeEntry,
  type CustomThemesContextValue,
} from "./customThemesContext";
import { getCustomThemes, type CustomTheme } from "./themesApi";

/**
 * Shares the custom themes with the shell, which applies the selected one,
 * and with Settings → Appearance. Tokens are validated once per load or
 * change; a theme whose tokens cannot be used is listed but never applied.
 */
export function CustomThemesProvider({ children }: { children: ReactNode }) {
  const loaded = useLoader(getCustomThemes);
  // Themes returned by the last saved change, newer than the last load.
  const [changed, setChanged] = useState<CustomTheme[] | null>(null);
  const stored = changed ?? (loaded.state.status === "ready" ? loaded.state.value : null);
  const loadError = changed === null && loaded.state.status === "error" ? loaded.state.error : null;

  const themes = useMemo<CustomThemeEntry[] | null>(
    () =>
      stored?.map((theme) => ({
        theme,
        tokens: parseThemeTokens(theme.tokensVersion, theme.tokens),
      })) ?? null,
    [stored],
  );
  const index = useMemo(
    () => new Map((themes ?? []).map((entry) => [entry.theme.id, entry])),
    [themes],
  );

  const { reload: reloadThemes } = loaded;
  const reload = useCallback(() => {
    setChanged(null);
    reloadThemes();
  }, [reloadThemes]);

  const value = useMemo<CustomThemesContextValue>(
    () => ({
      themes,
      loadError,
      byId: (id) => index.get(id),
      replace: setChanged,
      reload,
    }),
    [themes, loadError, index, reload],
  );
  return <CustomThemesContext value={value}>{children}</CustomThemesContext>;
}
