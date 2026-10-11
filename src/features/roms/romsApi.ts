import { invokeCommand, type CommandError } from "../../shared/ipc";
import type { LaunchRequest, LaunchTarget } from "../launcher/launcherApi";
import type { RemoteError, SourceCategory } from "../remote/remoteApi";

// These types mirror the Rust contracts in `src-tauri/src/roms/` and
// `src-tauri/src/commands/roms.rs`. Keep both sides in sync.

/** Mirrors `Reason` in `roms/mod.rs`. */
export type Reason =
  "selected" | "system" | "parent" | "bios" | "device" | "software" | "softwareParent" | "sample";

/** Mirrors `Availability` in `roms/plan.rs`. */
export type Availability =
  "missing" | "present" | "remoteAvailable" | "remoteMissing" | "unresolved";

/** Mirrors `WarningKind` in `roms/plan.rs`. */
export type WarningKind =
  "availability" | "informational" | "configuration" | "unresolved" | "conflict";

export interface PlanWarning {
  kind: WarningKind;
  message: string;
}

/** Mirrors `LocalFile` in `roms/local.rs`. */
export interface LocalFile {
  path: string;
  size: number;
  issue: string | null;
}

/** Mirrors `PlanArtifact` in `roms/plan.rs`. */
export interface PlanArtifact {
  id: string;
  category: SourceCategory;
  set: string;
  reason: Reason;
  fileName: string;
  relativePath: string;
  connectionId: string | null;
  remotePath: string | null;
  destination: string | null;
  local: LocalFile | null;
  availability: Availability;
  remoteSize: number | null;
  remoteError: RemoteError | null;
  download: boolean;
  warning: PlanWarning | null;
}

/** Mirrors `ConnectionIssue` in `roms/plan.rs`. */
export interface ConnectionIssue {
  connectionId: string;
  connectionName: string;
  error: RemoteError;
}

/** Mirrors `PlanSummary` in `roms/plan.rs`. */
export interface PlanSummary {
  present: number;
  toDownload: number;
  notFound: number;
  unresolved: number;
  warnings: number;
  downloadBytes: number;
  unknownSizes: number;
}

/** Mirrors `DownloadPlan` in `roms/plan.rs`. */
export interface DownloadPlan {
  id: string;
  installationId: string;
  installationLabel: string;
  /** As serialized by the backend: the default profile is `null`. */
  request: Omit<LaunchRequest, "software" | "profileId"> & {
    software: LaunchRequest["software"] | null;
    profileId: string | null;
  };
  target: LaunchTarget;
  title: string;
  createdAt: number;
  artifacts: PlanArtifact[];
  warnings: PlanWarning[];
  connectionIssues: ConnectionIssue[];
  summary: PlanSummary;
}

/** Mirrors `TransferStatus` in `roms/download.rs`. */
export type TransferStatus =
  "pending" | "downloading" | "downloaded" | "skipped" | "failed" | "cancelled";

/** Mirrors `TransferError` in `roms/download.rs`. */
export type TransferError =
  | { kind: "remote"; error: RemoteError }
  | { kind: "sizeMismatch"; expected: number; received: number }
  | { kind: "local"; message: string };

/** Mirrors `TransferState` in `roms/download.rs`. */
export interface TransferState {
  artifactId: string;
  status: TransferStatus;
  bytesTransferred: number;
  size: number | null;
  error: TransferError | null;
}

/** Mirrors `JobState` in `commands/roms.rs`. */
export type JobState = "running" | "completed" | "completedWithWarnings" | "failed" | "cancelled";

/** Mirrors `LaunchOutcome` in `commands/roms.rs`. */
export type LaunchOutcome =
  | { status: "started"; sessionId: number }
  | { status: "notStarted" }
  | { status: "failed"; error: CommandError };

/** Mirrors `DownloadJobStatus` in `commands/roms.rs`. */
export interface DownloadJobStatus {
  planId: string;
  installationId: string;
  installationLabel: string;
  target: LaunchTarget;
  title: string;
  play: boolean;
  state: JobState;
  cancelRequested: boolean;
  transfers: TransferState[];
  bytesTransferred: number;
  bytesTotal: number | null;
  startedAt: number;
  finishedAt: number | null;
  launch: LaunchOutcome | null;
}

/** Passphrases by connection id: sent with one command, never stored. */
export type Passphrases = Record<string, string>;

/** Builds the download plan of a launch; nothing is downloaded. */
export function prepareDownloadPlan(
  request: LaunchRequest,
  passphrases: Passphrases,
): Promise<DownloadPlan> {
  return invokeCommand("prepare_download_plan", { request, passphrases });
}

/**
 * Downloads the available missing files of a plan (again for a retry). With
 * `play`, the backend starts MAME with the plan's request once every
 * transfer succeeded.
 */
export function startDownload(
  planId: string,
  passphrases: Passphrases,
  play: boolean,
): Promise<DownloadJobStatus> {
  return invokeCommand("start_download", { planId, passphrases, play });
}

/** The running or last download; in memory, suitable for polling. */
export function getDownloadStatus(): Promise<DownloadJobStatus | null> {
  return invokeCommand("get_download_status");
}

export function cancelDownload(): Promise<DownloadJobStatus | null> {
  return invokeCommand("cancel_download");
}

/** Whether a plan was prepared for exactly this launch request. */
export function planMatches(plan: DownloadPlan, request: LaunchRequest): boolean {
  const media = (value: LaunchRequest["software"] | null | undefined) =>
    value
      ? JSON.stringify([value.list, value.name, value.media.map((m) => [m.device, m.part])])
      : "";
  return (
    plan.request.installationId === request.installationId &&
    plan.request.machine === request.machine &&
    (plan.request.profileId ?? null) === (request.profileId ?? null) &&
    media(plan.request.software) === media(request.software)
  );
}

/** A plan's request in the shape the launcher sends. */
export function launchRequestOf(plan: DownloadPlan): LaunchRequest {
  const { software, profileId, ...rest } = plan.request;
  return {
    ...rest,
    ...(software ? { software } : {}),
    ...(profileId ? { profileId } : {}),
  };
}
