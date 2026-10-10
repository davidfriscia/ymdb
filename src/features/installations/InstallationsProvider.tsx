import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { describeError } from "../../shared/ipc";
import {
  distinctLabels,
  listInstallations,
  setActiveInstallation,
  type InstallationsView,
} from "./installationsApi";
import { InstallationsContext, type InstallationsContextValue } from "./installationsContext";

/** Shares the registered installations and the active one with the whole application. */
export function InstallationsProvider({ children }: { children: ReactNode }) {
  const [view, setView] = useState<InstallationsView | null>(null);
  const [error, setError] = useState<unknown>(null);
  // Selected but not saved yet: the interface switches immediately.
  const [pending, setPending] = useState<string | null>(null);
  const [selectError, setSelectError] = useState<string | null>(null);
  // Selections are saved one after the other, so the last one always wins.
  const queue = useRef<Promise<void>>(Promise.resolve());
  const mounted = useRef(true);

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  const reload = useCallback(async () => {
    try {
      const loaded = await listInstallations();
      if (!mounted.current) return;
      setView(loaded);
      setError(null);
    } catch (loadError) {
      if (mounted.current) setError(loadError);
    }
  }, []);

  useEffect(() => {
    listInstallations().then(
      (loaded) => {
        if (!mounted.current) return;
        setView(loaded);
        setError(null);
      },
      (loadError: unknown) => {
        if (mounted.current) setError(loadError);
      },
    );
  }, []);

  const select = useCallback((id: string) => {
    setPending(id);
    setSelectError(null);
    const settle = () => {
      setPending((current) => (current === id ? null : current));
    };
    queue.current = queue.current.then(async () => {
      try {
        const saved = await setActiveInstallation(id);
        if (!mounted.current) return;
        setView(saved);
        settle();
      } catch (saveError) {
        if (!mounted.current) return;
        setSelectError(describeError(saveError));
        // Back to the saved selection.
        settle();
      }
    });
  }, []);

  const value = useMemo<InstallationsContextValue>(() => {
    const installations = view?.installations ?? [];
    const requested = pending ?? view?.activeInstallationId ?? null;
    const active = installations.find((installation) => installation.id === requested) ?? null;
    return {
      view,
      error,
      installations,
      activeId: active?.id ?? null,
      active,
      labels: distinctLabels(installations),
      select,
      selectError,
      reload,
    };
  }, [view, error, pending, select, selectError, reload]);

  return <InstallationsContext value={value}>{children}</InstallationsContext>;
}
