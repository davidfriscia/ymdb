import { useCallback, useEffect, useMemo, useState, type ReactNode } from "react";
import { useLoader } from "../../shared/useLoader";
import {
  getLaunchConfig,
  getLaunchSessions,
  saveLaunchConfig,
  startLaunch,
  stopLaunchSession,
  type LaunchConfig,
  type LaunchConfigView,
  type LaunchRequest,
  type SessionInfo,
} from "./launcherApi";
import { LauncherContext, type LauncherContextValue } from "./launcherContext";

interface LauncherProviderProps {
  children: ReactNode;
  /** The active installation, whose launch configuration is loaded. */
  installationId: string | null;
  showCommandPreview: boolean;
  onShowCommandPreviewChange: (show: boolean) => void;
  /** Interval between session status queries while a session runs. */
  pollIntervalMs?: number;
}

/**
 * Shares the launch configuration of the active installation and the running
 * sessions of every installation with the whole application.
 */
export function LauncherProvider({
  children,
  installationId,
  showCommandPreview,
  onShowCommandPreviewChange,
  pollIntervalMs = 1000,
}: LauncherProviderProps) {
  const load = useCallback(
    () => (installationId === null ? Promise.resolve(null) : getLaunchConfig(installationId)),
    [installationId],
  );
  const loaded = useLoader(load);
  const [saved, setSaved] = useState<LaunchConfigView | null>(null);
  const [configVersion, setConfigVersion] = useState(0);
  const [sessions, setSessions] = useState<SessionInfo[]>([]);
  const [sessionsVersion, setSessionsVersion] = useState(0);
  const running = sessions.some((session) => session.state.state === "running");

  useEffect(() => {
    let active = true;
    getLaunchSessions().then(
      (current) => {
        if (active) setSessions(current);
      },
      () => {
        // Statuses are refreshed on the next poll or action.
      },
    );
    return () => {
      active = false;
    };
  }, [sessionsVersion]);

  // Poll only while a session runs; the list is held in memory by the backend.
  useEffect(() => {
    if (!running) return;
    const timer = window.setInterval(() => {
      setSessionsVersion((version) => version + 1);
    }, pollIntervalMs);
    return () => {
      window.clearInterval(timer);
    };
  }, [running, pollIntervalMs]);

  const saveConfig = useCallback(async (id: string, config: LaunchConfig) => {
    const view = await saveLaunchConfig(id, config);
    setSaved(view);
    setConfigVersion((version) => version + 1);
    return view;
  }, []);
  const start = useCallback(async (request: LaunchRequest) => {
    try {
      return await startLaunch(request);
    } finally {
      setSessionsVersion((version) => version + 1);
    }
  }, []);
  const refreshSessions = useCallback(() => {
    setSessionsVersion((version) => version + 1);
  }, []);
  const stop = useCallback(async (id: number) => {
    try {
      await stopLaunchSession(id);
    } finally {
      setSessionsVersion((version) => version + 1);
    }
  }, []);

  const { reload } = loaded;
  const reloadConfig = useCallback(() => {
    setSaved(null);
    reload();
  }, [reload]);

  // A saved configuration replaces the loaded one only for its installation.
  const current = saved?.installationId === installationId ? saved : null;
  const view = current ?? (loaded.state.status === "ready" ? loaded.state.value : null);
  const error = loaded.state.status === "error" && !current ? loaded.state.error : null;
  const value = useMemo<LauncherContextValue>(
    () => ({
      view,
      error,
      reloadConfig,
      configVersion,
      saveConfig,
      sessions,
      start,
      refreshSessions,
      stop,
      showCommandPreview,
      setShowCommandPreview: onShowCommandPreviewChange,
    }),
    [
      view,
      error,
      reloadConfig,
      configVersion,
      saveConfig,
      sessions,
      start,
      refreshSessions,
      stop,
      showCommandPreview,
      onShowCommandPreviewChange,
    ],
  );
  return <LauncherContext value={value}>{children}</LauncherContext>;
}
