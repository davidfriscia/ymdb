import {
  MEDIA_CATEGORIES,
  type MediaSourcesView,
  type SourceStatus,
} from "../features/media/mediaApi";

/** A media sources view with the given directories, all with the same status. */
export function mediaSourcesView(
  sources: Partial<Record<string, string[]>> = {},
  status: SourceStatus = "available",
): MediaSourcesView {
  return {
    categories: MEDIA_CATEGORIES.map((category) => ({ id: category.id, label: category.label })),
    config: { schemaVersion: 1, sources },
    sources: MEDIA_CATEGORIES.map((category) => ({
      category: category.id,
      sources: (sources[category.id] ?? []).map((path) => ({ path, status })),
    })),
    loadError: null,
  };
}
