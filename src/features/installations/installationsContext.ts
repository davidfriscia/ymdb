import { createContext, useContext } from "react";
import type { InstallationSummary, InstallationsView } from "./installationsApi";

export interface InstallationsContextValue {
  /** `null` while loading or when the list could not be loaded (see `error`). */
  view: InstallationsView | null;
  error: unknown;
  /** In registration order. */
  installations: InstallationSummary[];
  /**
   * The installation browsed in the catalog and used by new launches. It
   * changes as soon as the user selects another one, before the selection is
   * saved.
   */
  activeId: string | null;
  active: InstallationSummary | null;
  /** Distinct display names by installation id. */
  labels: Map<string, string>;
  /** Selects the active installation. MAME is never started. */
  select: (id: string) => void;
  /** Why the last selection could not be saved. */
  selectError: string | null;
  /** Reads the list again, for example after a change on the MAME page. */
  reload: () => Promise<void>;
}

export const InstallationsContext = createContext<InstallationsContextValue | null>(null);

export function useInstallations(): InstallationsContextValue {
  const value = useContext(InstallationsContext);
  if (!value) throw new Error("useInstallations must be used inside InstallationsProvider");
  return value;
}

/**
 * The installation whose catalog a component reads. Catalog views get it
 * from the browser, so every request they send names one installation and a
 * response for another one is never shown.
 */
export const InstallationScope = createContext<string | null>(null);

export function useInstallationScope(): string {
  const id = useContext(InstallationScope);
  if (id === null) throw new Error("useInstallationScope must be used inside InstallationScope");
  return id;
}
