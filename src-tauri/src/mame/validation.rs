//! Validation of a MAME executable chosen by the user or found by detection.
//!
//! Validation checks the file system entry, then runs the executable with the
//! single argument `-version` (no shell, bounded by a timeout) and recognizes
//! the output with [`parse_version_output`].

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;

use super::platform;
use super::process::{self, ProcessError, ProcessOutput};
use super::version::{MameVersion, parse_version_output};

/// Maximum time `mame -version` may take before the process is terminated.
/// Generous because the first start of a large MAME binary can be slow (for
/// example while an antivirus scans it).
pub const VALIDATION_TIMEOUT: Duration = Duration::from_secs(20);

/// Outcome of a validation, ordered by the step at which it was decided.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ValidationStatus {
    /// The path is relative or no file exists at it.
    NotFound,
    /// The path exists but is not a regular file (for example a directory).
    NotAFile,
    /// The file cannot be executed directly as a program.
    NotExecutable,
    /// The file could not be accessed or the process could not be started.
    LaunchFailed { message: String },
    /// The process did not exit within the timeout and was terminated.
    TimedOut { timeout_ms: u64 },
    /// The process exited with a non-zero code, or was terminated by a signal (`None`).
    InvalidExitCode { exit_code: Option<i32> },
    /// The process exited successfully but did not print a MAME version.
    UnrecognizedOutput,
    /// The executable is a usable MAME installation.
    Valid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationReport {
    pub executable_path: PathBuf,
    pub status: ValidationStatus,
    /// Present only when the status is [`ValidationStatus::Valid`].
    pub version: Option<MameVersion>,
    /// Raw output of `-version`, present whenever the process ran to completion.
    pub output: Option<ProcessOutput>,
    /// Time of the check, in milliseconds since the Unix epoch.
    pub checked_at: u64,
}

impl ValidationReport {
    pub fn is_valid(&self) -> bool {
        self.status == ValidationStatus::Valid
    }
}

pub fn validate_executable(path: &Path) -> ValidationReport {
    validate_with_timeout(path, VALIDATION_TIMEOUT)
}

pub fn validate_with_timeout(path: &Path, timeout: Duration) -> ValidationReport {
    let checked_at = now_millis();
    let (status, version, output) = match check_file(path) {
        Err(status) => (status, None, None),
        Ok(()) => match process::run_with_timeout(version_command(path), timeout) {
            Ok(output) => {
                let (status, version) = interpret_output(&output);
                (status, version, Some(output))
            }
            Err(error) => (status_for_process_error(error, timeout), None, None),
        },
    };

    ValidationReport {
        executable_path: path.to_path_buf(),
        status,
        version,
        output,
        checked_at,
    }
}

fn check_file(path: &Path) -> Result<(), ValidationStatus> {
    // Relative paths are rejected so that the result never depends on the
    // working directory or on a PATH lookup performed by the OS.
    if !path.is_absolute() {
        return Err(ValidationStatus::NotFound);
    }
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(ValidationStatus::NotFound);
        }
        Err(error) => {
            return Err(ValidationStatus::LaunchFailed {
                message: format!("cannot access file: {error}"),
            });
        }
    };
    if !metadata.is_file() {
        return Err(ValidationStatus::NotAFile);
    }
    if !platform::is_executable(path, &metadata) {
        return Err(ValidationStatus::NotExecutable);
    }
    Ok(())
}

fn version_command(path: &Path) -> Command {
    let mut command = Command::new(path);
    command.arg("-version");
    // MAME resolves relative paths in its configuration against the working
    // directory, so it runs from its own directory.
    if let Some(dir) = path.parent() {
        command.current_dir(dir);
    }
    command
}

/// Interprets the output of a `-version` run that completed.
pub fn interpret_output(output: &ProcessOutput) -> (ValidationStatus, Option<MameVersion>) {
    if output.exit_code != Some(0) {
        return (
            ValidationStatus::InvalidExitCode {
                exit_code: output.exit_code,
            },
            None,
        );
    }
    match parse_version_output(&output.stdout) {
        Some(version) => (ValidationStatus::Valid, Some(version)),
        None => (ValidationStatus::UnrecognizedOutput, None),
    }
}

fn status_for_process_error(error: ProcessError, timeout: Duration) -> ValidationStatus {
    match error {
        ProcessError::TimedOut => ValidationStatus::TimedOut {
            timeout_ms: u64::try_from(timeout.as_millis()).unwrap_or(u64::MAX),
        },
        ProcessError::Spawn(error) if platform::is_not_executable_error(&error) => {
            ValidationStatus::NotExecutable
        }
        ProcessError::Spawn(error) => ValidationStatus::LaunchFailed {
            message: error.to_string(),
        },
        ProcessError::Wait(error) => ValidationStatus::LaunchFailed {
            message: format!("failed to wait for the process: {error}"),
        },
    }
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn output(exit_code: Option<i32>, stdout: &str, stderr: &str) -> ProcessOutput {
        ProcessOutput {
            exit_code,
            stdout: stdout.to_owned(),
            stderr: stderr.to_owned(),
        }
    }

    #[test]
    fn recognizes_valid_mame_output() {
        let (status, version) = interpret_output(&output(Some(0), "0.280 (mame0280)\r\n", ""));

        assert_eq!(status, ValidationStatus::Valid);
        assert_eq!(version.map(|v| v.release), Some("0.280".to_owned()));
    }

    #[test]
    fn rejects_successful_program_with_unrecognized_output() {
        let (status, version) = interpret_output(&output(Some(0), "Python 3.12.1\n", ""));

        assert_eq!(status, ValidationStatus::UnrecognizedOutput);
        assert_eq!(version, None);
    }

    #[test]
    fn rejects_non_zero_exit_code_even_with_version_output() {
        // MAME 0.162 does not support -version and exits with code 6.
        let (status, _) =
            interpret_output(&output(Some(6), "", "Error: unknown option: -version\n"));
        assert_eq!(
            status,
            ValidationStatus::InvalidExitCode { exit_code: Some(6) }
        );

        let (status, _) = interpret_output(&output(Some(1), "0.280 (mame0280)", ""));
        assert_eq!(
            status,
            ValidationStatus::InvalidExitCode { exit_code: Some(1) }
        );
    }

    #[test]
    fn reports_termination_by_signal_as_invalid_exit_code() {
        let (status, _) = interpret_output(&output(None, "", ""));

        assert_eq!(
            status,
            ValidationStatus::InvalidExitCode { exit_code: None }
        );
    }

    #[test]
    fn maps_process_errors_to_statuses() {
        let timeout = Duration::from_secs(10);

        assert_eq!(
            status_for_process_error(ProcessError::TimedOut, timeout),
            ValidationStatus::TimedOut { timeout_ms: 10_000 }
        );
        assert_eq!(
            status_for_process_error(
                ProcessError::Spawn(io::Error::from(io::ErrorKind::PermissionDenied)),
                timeout
            ),
            ValidationStatus::NotExecutable
        );
        assert!(matches!(
            status_for_process_error(
                ProcessError::Spawn(io::Error::other("boom")),
                timeout
            ),
            ValidationStatus::LaunchFailed { message } if message == "boom"
        ));
    }

    #[test]
    fn reports_missing_file() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("missing mame");

        let report = validate_executable(&path);

        assert_eq!(report.status, ValidationStatus::NotFound);
        assert_eq!(report.executable_path, path);
        assert_eq!(report.output, None);
        assert!(report.checked_at > 0);
    }

    #[test]
    fn rejects_relative_path() {
        let report = validate_executable(Path::new(platform::EXECUTABLE_NAMES[0]));

        assert_eq!(report.status, ValidationStatus::NotFound);
    }

    #[test]
    fn reports_directory_as_not_a_file() {
        let dir = tempfile::tempdir().expect("temp dir");

        let report = validate_executable(dir.path());

        assert_eq!(report.status, ValidationStatus::NotAFile);
    }

    #[test]
    fn reports_non_executable_file() {
        // Neither an `.exe` on Windows nor marked executable on Unix.
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("mame.txt");
        fs::write(&path, "not a program").expect("write file");

        let report = validate_executable(&path);

        assert_eq!(report.status, ValidationStatus::NotExecutable);
    }

    #[test]
    fn rejects_executable_looking_file_that_is_not_a_program() {
        // Files named and marked like executables whose content is not a
        // program, in a directory whose name contains spaces and non-ASCII characters.
        let dir = tempfile::tempdir().expect("temp dir");
        let install_dir = dir.path().join("Émulateurs MAME ✓");
        fs::create_dir(&install_dir).expect("create dir");
        let path = install_dir.join(platform::EXECUTABLE_NAMES[0]);

        for content in [&[0_u8; 64][..], b"not a program"] {
            fs::write(&path, content).expect("write file");
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("chmod");
            }

            let report = validate_executable(&path);

            assert_eq!(report.executable_path, path);
            // Windows refuses to start the file. On Unix, the C library may
            // fall back to running it with `/bin/sh`, which then fails.
            if cfg!(windows) {
                assert_eq!(report.status, ValidationStatus::NotExecutable);
            } else {
                assert!(
                    matches!(
                        report.status,
                        ValidationStatus::NotExecutable | ValidationStatus::InvalidExitCode { .. }
                    ),
                    "{report:?}"
                );
            }
        }
    }

    #[test]
    fn serializes_report_to_frontend_contract() {
        let report = ValidationReport {
            executable_path: PathBuf::from("/opt/mame/mame"),
            status: ValidationStatus::TimedOut { timeout_ms: 10_000 },
            version: None,
            output: None,
            checked_at: 1_700_000_000_000,
        };

        let json = serde_json::to_value(&report).expect("serialize");

        assert_eq!(
            json,
            serde_json::json!({
                "executablePath": "/opt/mame/mame",
                "status": { "kind": "timedOut", "timeoutMs": 10000 },
                "version": null,
                "output": null,
                "checkedAt": 1_700_000_000_000_u64
            })
        );
    }

    #[test]
    fn serializes_valid_report_with_version_and_output() {
        let report = ValidationReport {
            executable_path: PathBuf::from("/opt/mame/mame"),
            status: ValidationStatus::Valid,
            version: Some(MameVersion {
                release: "0.280".to_owned(),
                revision: "mame0280".to_owned(),
            }),
            output: Some(output(Some(0), "0.280 (mame0280)\n", "")),
            checked_at: 1,
        };

        let json = serde_json::to_value(&report).expect("serialize");

        assert_eq!(json["status"], serde_json::json!({ "kind": "valid" }));
        assert_eq!(
            json["version"],
            serde_json::json!({ "release": "0.280", "revision": "mame0280" })
        );
        assert_eq!(
            json["output"],
            serde_json::json!({ "exitCode": 0, "stdout": "0.280 (mame0280)\n", "stderr": "" })
        );
    }

    /// End-to-end check against a real MAME executable. Run with:
    /// `YMDB_TEST_MAME_EXECUTABLE=/path/to/mame cargo test -- --ignored`
    #[test]
    #[ignore = "requires a local MAME executable in YMDB_TEST_MAME_EXECUTABLE"]
    fn validates_real_mame_executable() {
        let path = std::env::var_os("YMDB_TEST_MAME_EXECUTABLE")
            .expect("YMDB_TEST_MAME_EXECUTABLE must be set");

        let report = validate_executable(Path::new(&path));

        assert_eq!(report.status, ValidationStatus::Valid, "{report:?}");
        assert!(report.version.is_some());
    }
}
