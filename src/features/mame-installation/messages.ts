import type { CandidateSource, CommandError, MameVersion, ValidationStatus } from "./mameApi";

export function describeStatus(status: ValidationStatus): string {
  switch (status.kind) {
    case "valid":
      return "Valid MAME installation.";
    case "notFound":
      return "The file does not exist.";
    case "notAFile":
      return "The path is not a file.";
    case "notExecutable":
      return "The file is not an executable program.";
    case "launchFailed":
      return `The program could not be started: ${status.message}`;
    case "timedOut":
      return `The program did not respond within ${String(status.timeoutMs / 1000)} seconds and was stopped.`;
    case "invalidExitCode":
      return status.exitCode === null
        ? "The program was terminated before completing the version check."
        : `The program exited with code ${String(status.exitCode)}. It may not be MAME, or it may be a MAME version too old to report its version.`;
    case "unrecognizedOutput":
      return "The program did not report a MAME version. It does not appear to be MAME.";
  }
}

export function describeCommandError(error: CommandError): string {
  switch (error.kind) {
    case "settings":
      return `Settings error: ${error.message}`;
    case "notConfigured":
      return "No MAME executable is configured.";
    case "internal":
      return `Unexpected error: ${error.message}`;
  }
}

export function formatVersion(version: MameVersion): string {
  return `MAME ${version.release} (${version.revision})`;
}

export function describeSource(source: CandidateSource): string {
  switch (source) {
    case "configured":
      return "Current configuration";
    case "path":
      return "Found in PATH";
    case "conventional":
      return "Standard location";
  }
}

export function formatTimestamp(millis: number): string {
  return new Date(millis).toLocaleString();
}
