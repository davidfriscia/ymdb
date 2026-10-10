import { MEDIA_CATEGORIES } from "../../media/mediaApi";
import { ClassicCardRenderer } from "./ClassicCardRenderer";
import { MediaGridRenderer } from "./MediaGridRenderer";
import type { ResultRendererDefinition } from "./types";

/**
 * Available result renderers. A renderer (for example a future table or list)
 * implements `ResultRendererProps`, declares its options and is registered
 * here; the View Options panel and the preferences need no other change.
 */
export const RENDERERS = [
  {
    id: "classicCards",
    label: "Classic Cards",
    icon: "M4 4h7v7H4zM13 4h7v7h-7zM4 13h7v7H4zM13 13h7v7h-7z",
    Component: ClassicCardRenderer,
    options: [
      {
        key: "cardSize",
        type: "range",
        label: "Card size",
        min: 160,
        max: 420,
        step: 10,
        default: 220,
        unit: "px",
      },
    ],
  },
  {
    id: "mediaGrid",
    label: "Media Grid",
    icon: "M3 5h18v14H3zM3 15l5-5 4 4 3-3 6 6M15.5 9a1.5 1.5 0 1 0 0-.01",
    Component: MediaGridRenderer,
    options: [
      {
        key: "tileSize",
        type: "range",
        label: "Tile size",
        min: 120,
        max: 480,
        step: 10,
        default: 220,
        unit: "px",
      },
      {
        key: "category",
        type: "choice",
        label: "Artwork",
        choices: MEDIA_CATEGORIES.map((category) => ({
          value: category.id,
          label: category.label,
        })),
        default: "snap",
      },
      {
        key: "fit",
        type: "choice",
        label: "Image fit",
        choices: [
          { value: "contain", label: "Whole image" },
          { value: "cover", label: "Fill the tile" },
        ],
        default: "contain",
      },
      { key: "showTitle", type: "toggle", label: "Show title", default: true },
      { key: "showName", type: "toggle", label: "Show technical name", default: false },
      { key: "showBadges", type: "toggle", label: "Show status badges", default: true },
    ],
  },
] as const satisfies readonly ResultRendererDefinition[];

export type RendererId = (typeof RENDERERS)[number]["id"];

export const DEFAULT_RENDERER: RendererId = "classicCards";

export function isRendererId(value: unknown): value is RendererId {
  return RENDERERS.some((renderer) => renderer.id === value);
}

export function rendererById(id: RendererId): ResultRendererDefinition {
  return RENDERERS.find((renderer) => renderer.id === id) ?? RENDERERS[0];
}
