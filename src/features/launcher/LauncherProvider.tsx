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
  showCommandPreview: boolean;
  onShowCommandPreviewChange: (show: boolean) => void;
  /** Interval between session status queries while a session runs. */
  pollIntervalMs?: number;
}

/** Shares the launch configuration and the running sessions with the whole application. */
export function LauncherProvider({
  children,
  showCommandPreview,
  onShowCommandPreviewChange,
  pollIntervalMs = 1000,
}: LauncherProviderProps) {
  const loaded = useLoader(getLaunchConfig);
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

  const saveConfig = useCallback(async (config: LaunchConfig) => {
    setSaved(await saveLaunchConfig(config));
    setConfigVersion((version) => version + 1);
  }, []);
  const start = useCallback(async (request: LaunchRequest) => {
    try {
      return await startLaunch(request);
    } finally {
      setSessionsVersion((version) => version + 1);
    }
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

  const view = saved ?? (loaded.state.status === "ready" ? loaded.state.value : null);
  const error = loaded.state.status === "error" && !saved ? loaded.state.error : null;
  const value = useMemo<LauncherContextValue>(
    () => ({
      view,
      error,
      reloadConfig,
      configVersion,
      saveConfig,
      sessions,
      start,
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
      stop,
      showCommandPreview,
      onShowCommandPreviewChange,
    ],
  );
  return <LauncherContext value={value}>{children}</LauncherContext>;
}
