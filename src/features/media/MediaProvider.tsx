import { useCallback, useMemo, useState, type ReactNode } from "react";
import { useLoader } from "../../shared/useLoader";
import {
  getMediaSources,
  saveMediaSources,
  type MediaConfig,
  type MediaSourcesView,
} from "./mediaApi";
import { MediaContext, type MediaContextValue } from "./mediaContext";

/** Shares the media sources with the browser and the settings. */
export function MediaProvider({ children }: { children: ReactNode }) {
  const [version, setVersion] = useState(0);
  const [saved, setSaved] = useState<MediaSourcesView | null>(null);
  const loaded = useLoader(getMediaSources, version);

  const save = useCallback(async (config: MediaConfig) => {
    setSaved(await saveMediaSources(config));
    setVersion((current) => current + 1);
  }, []);
  const { reload } = loaded;

  // After a save, the saved view is shown until the reload (with statuses) completes.
  const view = loaded.state.status === "ready" ? loaded.state.value : saved;
  const error = loaded.state.status === "error" ? loaded.state.error : null;
  const value = useMemo<MediaContextValue>(
    () => ({ view, error, reload, version, save }),
    [view, error, reload, version, save],
  );
  return <MediaContext value={value}>{children}</MediaContext>;
}
