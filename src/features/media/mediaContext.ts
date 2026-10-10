import { createContext, useContext } from "react";
import type { MediaConfig, MediaSourcesView } from "./mediaApi";

export interface MediaContextValue {
  /** `null` while loading or when the sources could not be read (see `error`). */
  view: MediaSourcesView | null;
  error: unknown;
  reload: () => void;
  /** Incremented whenever the sources are saved; part of image URLs. */
  version: number;
  save: (config: MediaConfig) => Promise<void>;
}

export const MediaContext = createContext<MediaContextValue | null>(null);

export function useMedia(): MediaContextValue {
  const value = useContext(MediaContext);
  if (!value) throw new Error("useMedia must be used inside MediaProvider");
  return value;
}

/** Whether at least one directory is configured for a category. */
export function hasSources(view: MediaSourcesView | null, category: string): boolean {
  return (view?.config.sources[category]?.length ?? 0) > 0;
}
