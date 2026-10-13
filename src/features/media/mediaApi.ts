import { convertFileSrc } from "@tauri-apps/api/core";
import { invokeCommand } from "../../shared/ipc";

// These types mirror the Rust contracts in `src-tauri/src/media/` and
// `src-tauri/src/commands/media.rs`. Keep both sides in sync.

/** Mirrors `CATEGORIES` in `media/config.rs`. */
export const MEDIA_CATEGORIES = [
  { id: "snap", label: "Snapshots" },
  { id: "titles", label: "Title screens" },
  { id: "flyers", label: "Flyers" },
  { id: "cabinets", label: "Cabinets" },
  { id: "marquees", label: "Marquees" },
] as const;

export type MediaCategoryId = (typeof MEDIA_CATEGORIES)[number]["id"];

/** Mirrors `MediaConfig` in `media/config.rs`. */
export interface MediaConfig {
  schemaVersion: number;
  /** Directories per category, in order of precedence. */
  sources: Partial<Record<string, string[]>>;
}

export type SourceStatus = "available" | "missing" | "notADirectory" | "unreadable";

/** Mirrors `MediaSourcesView` in `commands/media.rs`. */
export interface MediaSourcesView {
  categories: { id: string; label: string }[];
  config: MediaConfig;
  sources: { category: string; sources: { path: string; status: SourceStatus }[] }[];
  loadError: string | null;
}

export function getMediaSources(): Promise<MediaSourcesView> {
  return invokeCommand("get_media_sources");
}

export function saveMediaSources(config: MediaConfig): Promise<MediaSourcesView> {
  return invokeCommand("save_media_sources", { config });
}

/** Native directory picker; `null` when cancelled. */
export function pickMediaDirectory(): Promise<string | null> {
  return invokeCommand("pick_media_directory");
}

/** What an image is requested for. */
export type MediaTarget =
  { kind: "machine"; name: string } | { kind: "software"; list: string; name: string };

/** Mirrors `CategoryImages` in `media/protocol.rs`. */
export interface CategoryImages {
  category: MediaCategoryId;
  /** Number of images, requested by index from 0 to `count - 1`. */
  count: number;
}

/**
 * The categories with at least one image of `target`, in category order. Only
 * the files of this title are looked up in the configured directories.
 */
export function listMedia(target: MediaTarget): Promise<CategoryImages[]> {
  return invokeCommand("list_media", { target });
}

/**
 * URL of an image served by the `ymdb-media` protocol. It contains only
 * identifiers; the backend finds the file in the configured directories.
 * `index` selects one of the images of the title in the category (the first
 * by default). `version` changes when the sources change, so the webview does
 * not reuse images of the previous configuration.
 */
export function mediaUrl(
  category: string,
  target: MediaTarget,
  version: number,
  index = 0,
): string {
  const record =
    target.kind === "machine"
      ? `${category}/machine/${target.name}`
      : `${category}/software/${target.list}/${target.name}`;
  const path = index === 0 ? record : `${record}/${String(index)}`;
  return `${convertFileSrc(path, "ymdb-media")}?v=${String(version)}`;
}
