import { formatCount } from "../catalog/catalogMessages";
import { describeRemoteError } from "../remote/remoteMessages";
import type { Availability, JobState, Reason, TransferError, TransferStatus } from "./romsApi";

export const REASON_LABELS: Record<Reason, string> = {
  selected: "Selected machine",
  system: "Machine running the software",
  parent: "Parent",
  bios: "BIOS",
  device: "Device",
  software: "Selected software",
  softwareParent: "Parent software",
  sample: "Samples",
};

export const AVAILABILITY_LABELS: Record<Availability, string> = {
  present: "Present locally",
  missing: "Missing, not looked up",
  remoteAvailable: "To download",
  remoteMissing: "Not found remotely",
  unresolved: "Unresolved",
};

export const TRANSFER_LABELS: Record<TransferStatus, string> = {
  pending: "Pending",
  downloading: "Downloading",
  downloaded: "Downloaded",
  skipped: "Kept existing file",
  failed: "Failed",
  cancelled: "Cancelled",
};

export const JOB_MESSAGES: Record<Exclude<JobState, "running">, string> = {
  completed: "Download completed.",
  completedWithWarnings:
    "Download completed with warnings: some candidate files were not available. MAME decides whether the files present are enough.",
  failed: "Download failed: some files could not be downloaded.",
  cancelled: "Download cancelled. Completed files were kept; partial files were removed.",
};

export function describeTransferError(error: TransferError): string {
  switch (error.kind) {
    case "remote":
      return describeRemoteError(error.error);
    case "sizeMismatch":
      return `Received ${formatCount(error.received)} bytes instead of ${formatCount(error.expected)}; the file was discarded.`;
    case "local":
      return `Local file error: ${error.message}.`;
  }
}

/** A short size, for progress and tables. */
export function shortSize(bytes: number | null): string {
  if (bytes === null) return "—";
  if (bytes < 1024) return `${String(bytes)} B`;
  const units = ["KiB", "MiB", "GiB", "TiB"];
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit++;
  }
  return `${value.toFixed(value < 10 ? 1 : 0)} ${units[unit] ?? ""}`;
}
