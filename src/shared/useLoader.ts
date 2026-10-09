import { useCallback, useEffect, useState } from "react";

export type LoadState<T> =
  { status: "loading" } | { status: "ready"; value: T } | { status: "error"; error: unknown };

interface LoadResult<T> {
  load: () => Promise<T>;
  refreshKey: number;
  attempt: number;
  state: LoadState<T>;
}

export interface Loader<T> {
  /** State of the current load. */
  state: LoadState<T>;
  /** Last successfully loaded value, possibly from a previous load. */
  latest: T | null;
  reload: () => void;
}

/**
 * Runs `load` whenever it or `refreshKey` changes. `load` must be stable
 * (`useCallback`). Responses of superseded loads are ignored, so a slow
 * response can never replace a newer one.
 */
export function useLoader<T>(load: () => Promise<T>, refreshKey = 0): Loader<T> {
  const [result, setResult] = useState<LoadResult<T> | null>(null);
  const [latest, setLatest] = useState<T | null>(null);
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    let active = true;
    load().then(
      (value) => {
        if (!active) return;
        setResult({ load, refreshKey, attempt, state: { status: "ready", value } });
        setLatest(value);
      },
      (error: unknown) => {
        if (active) setResult({ load, refreshKey, attempt, state: { status: "error", error } });
      },
    );
    return () => {
      active = false;
    };
  }, [load, refreshKey, attempt]);

  const reload = useCallback(() => {
    setAttempt((value) => value + 1);
  }, []);

  const current =
    result?.load === load && result.refreshKey === refreshKey && result.attempt === attempt
      ? result.state
      : null;
  return { state: current ?? { status: "loading" }, latest, reload };
}
