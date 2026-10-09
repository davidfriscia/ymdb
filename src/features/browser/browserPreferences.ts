import type { RendererId } from "./renderers/registry";

/** Display preferences of the browser; they never change the search. */
export interface BrowserPreferences {
  /** Minimum card width in pixels. */
  cardSize: number;
  filtersVisible: boolean;
  renderer: RendererId;
}

export const CARD_SIZE = { min: 160, max: 420, step: 10, default: 220 } as const;
