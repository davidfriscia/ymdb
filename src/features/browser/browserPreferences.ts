import { normalizeOptions } from "./renderers/options";
import { DEFAULT_RENDERER, RENDERERS, isRendererId, type RendererId } from "./renderers/registry";
import type { RendererOptionValues } from "./renderers/types";

/**
 * Display preferences of the browser. Only `favoritesOnly` restricts the
 * search; the others never change it.
 */
export interface BrowserPreferences {
  filtersVisible: boolean;
  viewOptionsVisible: boolean;
  renderer: RendererId;
  /** Options of every renderer, by renderer id, kept when switching renderer. */
  rendererOptions: Readonly<Record<string, RendererOptionValues>>;
  /** Shows only the entries of the global Favorites list (not the list itself). */
  favoritesOnly: boolean;
}

/** Valid options for every registered renderer, from stored values or defaults. */
export function normalizeRendererOptions(raw: unknown): Record<string, RendererOptionValues> {
  const stored = typeof raw === "object" && raw !== null ? (raw as Record<string, unknown>) : {};
  return Object.fromEntries(
    RENDERERS.map((renderer) => [
      renderer.id,
      normalizeOptions(renderer.options, stored[renderer.id]),
    ]),
  );
}

export const DEFAULT_BROWSER_PREFERENCES: BrowserPreferences = {
  filtersVisible: true,
  viewOptionsVisible: false,
  renderer: DEFAULT_RENDERER,
  rendererOptions: normalizeRendererOptions(undefined),
  favoritesOnly: false,
};

/** Reads browser preferences from stored values; invalid values use the defaults. */
export function parseBrowserPreferences(values: Record<string, unknown>): BrowserPreferences {
  return {
    filtersVisible:
      typeof values.filtersVisible === "boolean"
        ? values.filtersVisible
        : DEFAULT_BROWSER_PREFERENCES.filtersVisible,
    viewOptionsVisible:
      typeof values.viewOptionsVisible === "boolean"
        ? values.viewOptionsVisible
        : DEFAULT_BROWSER_PREFERENCES.viewOptionsVisible,
    renderer: isRendererId(values.renderer)
      ? values.renderer
      : DEFAULT_BROWSER_PREFERENCES.renderer,
    rendererOptions: normalizeRendererOptions(values.rendererOptions),
    favoritesOnly:
      typeof values.favoritesOnly === "boolean"
        ? values.favoritesOnly
        : DEFAULT_BROWSER_PREFERENCES.favoritesOnly,
  };
}
