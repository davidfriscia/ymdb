import { CardRenderer } from "./CardRenderer";
import type { ResultRendererDefinition } from "./types";

const REGISTERED = [
  { id: "cards", label: "Cards", sizable: true, Component: CardRenderer },
] as const satisfies readonly ResultRendererDefinition[];

/**
 * Available result renderers. A new renderer (for example a table or a
 * compact list) implements `ResultRendererProps` and is registered here; the
 * renderer selector appears when more than one is registered.
 */
export const RENDERERS: readonly ResultRendererDefinition[] = REGISTERED;

export type RendererId = (typeof REGISTERED)[number]["id"];

export const DEFAULT_RENDERER: RendererId = "cards";

export function isRendererId(value: unknown): value is RendererId {
  return RENDERERS.some((renderer) => renderer.id === value);
}

export function rendererById(id: RendererId): ResultRendererDefinition {
  return RENDERERS.find((renderer) => renderer.id === id) ?? REGISTERED[0];
}
