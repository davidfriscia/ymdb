//! Runs external programs with a timeout.
//!
//! Programs are always started directly through [`std::process::Command`],
//! never through a shell, so paths and arguments are passed verbatim.

use std::io::{self, Read};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;

use super::platform;

/// Maximum number of bytes captured from each output stream. Additional
/// output is read and discarded so the child process never blocks on a full pipe.
const MAX_CAPTURED_BYTES: usize = 64 * 1024;
const POLL_INTERVAL: Duration = Duration::from_millis(20);
/// How long to wait for the output streams to close after the process exited.
const OUTPUT_DRAIN_TIMEOUT: Duration = Duration::from_secs(2);

/// Output of a process that ran to completion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessOutput {
    /// `None` when the process was terminated by a signal.
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug)]
pub enum ProcessError {
    /// The process could not be started.
    Spawn(io::Error),
    /// Waiting for the process failed; the process has been terminated.
    Wait(io::Error),
    /// The process did not exit in time and has been terminated.
    TimedOut,
}

/// Runs `command` to completion, capturing its output.
///
/// Standard input is closed. If the process is still running after `timeout`,
/// it is killed and reaped, and [`ProcessError::TimedOut`] is returned.
pub fn run_with_timeout(
    mut command: Command,
    timeout: Duration,
) -> Result<ProcessOutput, ProcessError> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    platform::configure_background_process(&mut command);

    let mut child = command.spawn().map_err(ProcessError::Spawn)?;
    let stdout = spawn_reader(child.stdout.take());
    let stderr = spawn_reader(child.stderr.take());

    let status = match wait_with_deadline(&mut child, timeout) {
        Ok(Some(status)) => status,
        Ok(None) => {
            terminate(&mut child);
            return Err(ProcessError::TimedOut);
        }
        Err(error) => {
            terminate(&mut child);
            return Err(ProcessError::Wait(error));
        }
    };

    Ok(ProcessOutput {
        exit_code: status.code(),
        stdout: collect(&stdout),
        stderr: collect(&stderr),
    })
}

fn wait_with_deadline(child: &mut Child, timeout: Duration) -> io::Result<Option<ExitStatus>> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(status));
        }
        let now = Instant::now();
        if now >= deadline {
            return Ok(None);
        }
        thread::sleep(POLL_INTERVAL.min(deadline - now));
    }
}

fn terminate(child: &mut Child) {
    // Errors are ignored: the process may have exited in the meantime.
    let _ = child.kill();
    let _ = child.wait();
}

/// Reads a stream to its end on a background thread and sends the captured bytes.
fn spawn_reader<R: Read + Send + 'static>(source: Option<R>) -> mpsc::Receiver<Vec<u8>> {
    let (sender, receiver) = mpsc::channel();
    if let Some(mut source) = source {
        thread::spawn(move || {
            let mut captured = Vec::new();
            let mut buffer = [0_u8; 8192];
            loop {
                match source.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(read) => {
                        let room = MAX_CAPTURED_BYTES - captured.len();
                        captured.extend_from_slice(&buffer[..read.min(room)]);
                    }
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                    Err(_) => break,
                }
            }
            // The receiver may be gone if the caller stopped waiting.
            let _ = sender.send(captured);
        });
    }
    receiver
}

/// Collects captured output. A stream kept open by another process (for
/// example a grandchild) is abandoned after a short grace period.
fn collect(receiver: &mpsc::Receiver<Vec<u8>>) -> String {
    receiver
        .recv_timeout(OUTPUT_DRAIN_TIMEOUT)
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captures_stdout_and_exit_code() {
        // `hostname` exists on Windows, macOS and Linux and prints a non-empty line.
        let output = run_with_timeout(Command::new("hostname"), Duration::from_secs(10))
            .expect("hostname should run");

        assert_eq!(output.exit_code, Some(0));
        assert!(!output.stdout.trim().is_empty());
    }

    #[test]
    fn reports_spawn_error_for_missing_program() {
        let dir = tempfile::tempdir().expect("temp dir");
        let missing = dir.path().join("missing-program");

        let result = run_with_timeout(Command::new(missing), Duration::from_secs(5));

        match result {
            Err(ProcessError::Spawn(error)) => assert_eq!(error.kind(), io::ErrorKind::NotFound),
            other => panic!("expected spawn error, got {other:?}"),
        }
    }

    #[test]
    fn terminates_process_after_timeout() {
        let command = if cfg!(windows) {
            let mut command = Command::new("ping");
            command.args(["-n", "30", "127.0.0.1"]);
            command
        } else {
            let mut command = Command::new("sleep");
            command.arg("30");
            command
        };
        let started = Instant::now();

        let result = run_with_timeout(command, Duration::from_millis(300));

        assert!(
            matches!(result, Err(ProcessError::TimedOut)),
            "got {result:?}"
        );
        assert!(started.elapsed() < Duration::from_secs(10));
    }
}
