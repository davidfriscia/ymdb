import { NEW_ID } from "./installationFixtures";
import { NAS_ID } from "./remoteFixtures";
import type {
  DownloadJobStatus,
  DownloadPlan,
  PlanArtifact,
  TransferState,
} from "../features/roms/romsApi";

export function artifact(
  relativePath: string,
  overrides: Partial<PlanArtifact> = {},
): PlanArtifact {
  return {
    id: `machineRoms:${relativePath}`,
    category: "machineRoms",
    set: relativePath.replace(/\.zip$/, ""),
    reason: "selected",
    fileName: relativePath,
    relativePath,
    connectionId: NAS_ID,
    remotePath: `/volume1/mame/roms/${relativePath}`,
    destination: `E:\\Downloads\\ROMs\\${relativePath}`,
    local: null,
    availability: "remoteAvailable",
    remoteSize: 2048,
    remoteError: null,
    download: true,
    warning: null,
    ...overrides,
  };
}

/** A plan for `pacman`: the clone to download, the parent present, a disk not found. */
export function downloadPlan(
  overrides: Partial<DownloadPlan> = {},
  installationId: string = NEW_ID,
  machine = "pacman",
): DownloadPlan {
  return {
    id: "plan-1",
    installationId,
    installationLabel: "MAME 0.280",
    request: { installationId, machine, software: null, profileId: null },
    target: { machine, software: null },
    title: "Pac-Man (Midway)",
    createdAt: 1,
    artifacts: [
      artifact("pacman.zip"),
      artifact("puckman.zip", {
        reason: "parent",
        availability: "present",
        download: false,
        remoteSize: null,
        local: { path: "C:\\MAME\\roms\\puckman.zip", size: 4096, issue: null },
      }),
      artifact("pacman/pacman.chd", {
        id: "machineChds:pacman/pacman.chd",
        category: "machineChds",
        availability: "remoteMissing",
        download: false,
        remoteSize: null,
        warning: {
          kind: "availability",
          message: "/volume1/mame/chds/pacman/pacman.chd was not found in the remote repository.",
        },
      }),
    ],
    warnings: [],
    connectionIssues: [],
    summary: {
      present: 1,
      toDownload: 1,
      notFound: 1,
      unresolved: 0,
      warnings: 1,
      downloadBytes: 2048,
      unknownSizes: 0,
    },
    ...overrides,
  };
}

export function transfer(
  artifactId: string,
  overrides: Partial<TransferState> = {},
): TransferState {
  return {
    artifactId,
    status: "pending",
    bytesTransferred: 0,
    size: 2048,
    error: null,
    ...overrides,
  };
}

export function jobStatus(overrides: Partial<DownloadJobStatus> = {}): DownloadJobStatus {
  return {
    planId: "plan-1",
    installationId: NEW_ID,
    installationLabel: "MAME 0.280",
    target: { machine: "pacman", software: null },
    title: "Pac-Man (Midway)",
    play: false,
    state: "running",
    cancelRequested: false,
    transfers: [transfer("machineRoms:pacman.zip")],
    bytesTransferred: 0,
    bytesTotal: 2048,
    startedAt: 1,
    finishedAt: null,
    launch: null,
    ...overrides,
  };
}
