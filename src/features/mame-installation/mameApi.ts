// These types mirror the Rust contracts in `src-tauri/src/mame/`. Keep both
// sides in sync. Installation commands are in `features/installations`.

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
