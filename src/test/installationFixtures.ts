import type {
  InstallationSummary,
  InstallationsView,
} from "../features/installations/installationsApi";

/** Ids of the test installations: MAME 0.280 (active by default) and 0.252. */
export const NEW_ID = "0123456789abcdef0123456789abcdef";
export const OLD_ID = "fedcba9876543210fedcba9876543210";

export function installation(
  id: string,
  release: string,
  overrides: Partial<InstallationSummary> = {},
): InstallationSummary {
  return {
    id,
    label: `MAME ${release}`,
    displayName: null,
    executablePath: `C:\\Emulators\\MAME ${release}\\mame.exe`,
    version: { release, revision: `mame${release.replace(".", "")}` },
    addedAt: Date.UTC(2026, 9, 1),
    verifiedAt: Date.UTC(2026, 9, 2),
    executableStatus: "verified",
    catalogImported: true,
    active: false,
    ...overrides,
  };
}

/** The installations of a view, with `active` set from `activeId`. */
export function installationsView(
  installations: InstallationSummary[] = [
    installation(NEW_ID, "0.280"),
    installation(OLD_ID, "0.252"),
  ],
  activeId: string | null = installations[0]?.id ?? null,
): InstallationsView {
  return {
    installations: installations.map((entry) => ({ ...entry, active: entry.id === activeId })),
    activeInstallationId: activeId,
  };
}
