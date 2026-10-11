import { createContext, useContext } from "react";
import type {
  LaunchConfig,
  LaunchConfigView,
  LaunchRequest,
  LaunchTarget,
  SessionInfo,
} from "./launcherApi";
import { sameTarget } from "./launcherApi";

export interface LauncherContextValue {
  /**
   * The launch configuration of the active installation; `null` while
   * loading, without an active installation, or when it could not be loaded
   * (see `error`). Check `view.installationId` before using it.
   */
  view: LaunchConfigView | null;
  /** Why the launch configuration could not be loaded. */
  error: unknown;
  reloadConfig: () => void;
  /** Incremented whenever a configuration is saved. */
  configVersion: number;
  /** Saves the whole configuration of an installation; rejects with the backend error. */
  saveConfig: (installationId: string, config: LaunchConfig) => Promise<LaunchConfigView>;
  /** Sessions of this run, from every installation, most recent first. */
  sessions: SessionInfo[];
  start: (request: LaunchRequest) => Promise<SessionInfo>;
  /** Reads the sessions again, for example after Download & Play started one. */
  refreshSessions: () => void;
  stop: (id: number) => Promise<void>;
  showCommandPreview: boolean;
  setShowCommandPreview: (show: boolean) => void;
}

export const LauncherContext = createContext<LauncherContextValue | null>(null);

export function useLauncher(): LauncherContextValue {
  const value = useContext(LauncherContext);
  if (!value) throw new Error("useLauncher must be used inside LauncherProvider");
  return value;
}

/**
 * The most recent session of a machine or software started from an
 * installation, if any. The same target may run once per installation.
 */
export function latestSession(
  sessions: SessionInfo[],
  target: LaunchTarget,
  installationId: string,
): SessionInfo | undefined {
  return sessions.find(
    (session) => session.installationId === installationId && sameTarget(session.target, target),
  );
}
