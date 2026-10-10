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
  /** `null` while loading or when it could not be loaded (see `error`). */
  view: LaunchConfigView | null;
  /** Why the launch configuration could not be loaded. */
  error: unknown;
  reloadConfig: () => void;
  /** Incremented whenever the configuration is saved. */
  configVersion: number;
  /** Saves the whole configuration; rejects with the backend error. */
  saveConfig: (config: LaunchConfig) => Promise<void>;
  /** Sessions of this run, most recent first. */
  sessions: SessionInfo[];
  start: (request: LaunchRequest) => Promise<SessionInfo>;
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

/** The most recent session of a machine or software, if any. */
export function latestSession(
  sessions: SessionInfo[],
  target: LaunchTarget,
): SessionInfo | undefined {
  return sessions.find((session) => sameTarget(session.target, target));
}
