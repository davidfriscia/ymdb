import { invokeCommand } from "../../shared/ipc";

// These types mirror the Rust contracts in `src-tauri/src/mame/` and
// `src-tauri/src/commands/`. Keep both sides in sync.

/** Mirrors `MameVersion` in `src-tauri/src/mame/version.rs`. */
export interface MameVersion {
  /** Release number, for example `0.280`. */
  release: string;
  /** Source revision of the build, for example `mame0280`. */
  revision: string;
}

/** Mirrors `ProcessOutput` in `src-tauri/src/mame/process.rs`. */
export interface ProcessOutput {
  /** `null` when the process was terminated by a signal. */
  exitCode: number | null;
  stdout: string;
  stderr: string;
}

/** Mirrors `ValidationStatus` in `src-tauri/src/mame/validation.rs`. */
export type ValidationStatus =
  | { kind: "notFound" }
  | { kind: "notAFile" }
  | { kind: "notExecutable" }
  | { kind: "launchFailed"; message: string }
  | { kind: "timedOut"; timeoutMs: number }
  | { kind: "invalidExitCode"; exitCode: number | null }
  | { kind: "unrecognizedOutput" }
  | { kind: "valid" };

/** Mirrors `ValidationReport` in `src-tauri/src/mame/validation.rs`. */
export interface ValidationReport {
  executablePath: string;
  status: ValidationStatus;
  version: MameVersion | null;
  output: ProcessOutput | null;
  /** Milliseconds since the Unix epoch. */
  checkedAt: number;
}

/** Mirrors `CandidateSource` in `src-tauri/src/mame/detection.rs`. */
export type CandidateSource = "configured" | "path" | "conventional";

/** Mirrors `DetectedInstallation` in `src-tauri/src/mame/detection.rs`. */
export interface DetectedInstallation {
  source: CandidateSource;
  report: ValidationReport;
}

/** Mirrors `MameConfiguration` in `src-tauri/src/commands/mame.rs`. */
export interface MameConfiguration {
  executablePath: string | null;
  executableExists: boolean;
}

const call = invokeCommand;

export function getMameConfiguration(): Promise<MameConfiguration> {
  return call("get_mame_configuration");
}

/** Opens the native file picker. Resolves to `null` when the user cancels. */
export function pickMameExecutable(): Promise<string | null> {
  return call("pick_mame_executable");
}

/** Validates the executable and saves it only if it is a valid MAME installation. */
export function setMameExecutable(path: string): Promise<ValidationReport> {
  return call("set_mame_executable", { path });
}

/** Validates the configured executable without changing the configuration. */
export function verifyMameConfiguration(): Promise<ValidationReport> {
  return call("verify_mame_configuration");
}

export function clearMameConfiguration(): Promise<void> {
  return call("clear_mame_configuration");
}

/** Searches well-known locations. Never changes the configuration. */
export function detectMameInstallations(): Promise<DetectedInstallation[]> {
  return call("detect_mame_installations");
}
