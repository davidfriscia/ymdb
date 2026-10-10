//! Runs external programs.
//!
//! Programs are always started directly through [`std::process::Command`],
//! never through a shell, so paths and arguments are passed verbatim.
//!
//! Three modes are provided:
//!
//! - [`run_with_timeout`] for short commands whose whole output is small
//!   (for example `mame -version`).
//! - [`spawn_streaming`] for long-running commands with large output (for
//!   example `mame -listxml`): standard output is consumed incrementally by the
//!   caller, while a watchdog terminates the process on cancellation or
//!   inactivity.
//! - [`spawn_detached`] for game sessions: no timeout, output to a log file.

use std::fs::File;
use std::io::{self, Read};
use std::process::{Child, ChildStdout, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;

use super::platform;

/// Maximum number of bytes captured from a buffered output stream. Additional
/// output is read and discarded so the child process never blocks on a full pipe.
const MAX_CAPTURED_BYTES: usize = 64 * 1024;
const POLL_INTERVAL: Duration = Duration::from_millis(20);
/// How long to wait for the output streams to close after the process exited.
const OUTPUT_DRAIN_TIMEOUT: Duration = Duration::from_secs(2);
/// How often the streaming watchdog checks for cancellation and inactivity.
const WATCHDOG_INTERVAL: Duration = Duration::from_millis(50);
/// How long a streaming process may take to exit after its output was consumed.
const STREAMING_EXIT_GRACE: Duration = Duration::from_secs(30);

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
    command: Command,
    timeout: Duration,
) -> Result<ProcessOutput, ProcessError> {
    let mut child = spawn_piped(command)?;
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

/// Why the watchdog of a streaming process terminated it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Termination {
    /// The cancellation flag was set.
    Cancelled,
    /// No output was produced for longer than the inactivity timeout.
    Inactive,
}

/// Result of a streaming process once it has exited.
#[derive(Debug)]
pub struct StreamingOutcome {
    /// `None` when the process was terminated by a signal.
    pub exit_code: Option<i32>,
    /// Captured standard error (bounded).
    pub stderr: String,
    /// Set when the watchdog terminated the process.
    pub termination: Option<Termination>,
}

/// A running process whose standard output is consumed incrementally.
///
/// Dropping it terminates and reaps the process if it is still running, so an
/// early return of the caller never leaks a process.
pub struct StreamingProcess {
    child: Arc<Mutex<Child>>,
    stdout: Option<ChildStdout>,
    stderr: mpsc::Receiver<Vec<u8>>,
    watch: Arc<Watch>,
    watchdog: Option<thread::JoinHandle<()>>,
    reaped: bool,
}

struct Watch {
    started: Instant,
    /// Milliseconds since `started` at which output was last received.
    last_activity_ms: AtomicU64,
    stopped: AtomicBool,
    termination: Mutex<Option<Termination>>,
}

impl Watch {
    fn record_activity(&self) {
        self.last_activity_ms
            .store(elapsed_millis(self.started), Ordering::Relaxed);
    }

    fn idle_for(&self) -> Duration {
        let last = self.last_activity_ms.load(Ordering::Relaxed);
        Duration::from_millis(elapsed_millis(self.started).saturating_sub(last))
    }

    fn termination(&self) -> Option<Termination> {
        *self
            .termination
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

/// Starts `command` with piped standard output for incremental consumption.
///
/// A watchdog thread kills the process when `cancel` becomes `true` or when no
/// output has been read for `inactivity_timeout`. Standard error is captured
/// in the background (bounded).
pub fn spawn_streaming(
    command: Command,
    inactivity_timeout: Duration,
    cancel: Arc<AtomicBool>,
) -> Result<StreamingProcess, ProcessError> {
    let mut child = spawn_piped(command)?;
    let stdout = child.stdout.take();
    let stderr = spawn_reader(child.stderr.take());
    let child = Arc::new(Mutex::new(child));
    let watch = Arc::new(Watch {
        started: Instant::now(),
        last_activity_ms: AtomicU64::new(0),
        stopped: AtomicBool::new(false),
        termination: Mutex::new(None),
    });

    let watchdog = {
        let child = Arc::clone(&child);
        let watch = Arc::clone(&watch);
        thread::spawn(move || run_watchdog(&child, &watch, &cancel, inactivity_timeout))
    };

    Ok(StreamingProcess {
        child,
        stdout,
        stderr,
        watch,
        watchdog: Some(watchdog),
        reaped: false,
    })
}

fn run_watchdog(
    child: &Mutex<Child>,
    watch: &Watch,
    cancel: &AtomicBool,
    inactivity_timeout: Duration,
) {
    while !watch.stopped.load(Ordering::Relaxed) {
        let reason = if cancel.load(Ordering::Relaxed) {
            Some(Termination::Cancelled)
        } else if watch.idle_for() > inactivity_timeout {
            Some(Termination::Inactive)
        } else {
            None
        };
        if let Some(reason) = reason {
            *watch
                .termination
                .lock()
                .unwrap_or_else(PoisonError::into_inner) = Some(reason);
            // The process may have exited in the meantime.
            let _ = child.lock().unwrap_or_else(PoisonError::into_inner).kill();
            return;
        }
        thread::sleep(WATCHDOG_INTERVAL);
    }
}

impl StreamingProcess {
    /// Takes the standard output stream. Reading from it counts as activity
    /// for the inactivity watchdog.
    pub fn take_stdout(&mut self) -> Option<ActivityReader> {
        self.stdout.take().map(|inner| ActivityReader {
            inner,
            watch: Arc::clone(&self.watch),
        })
    }

    /// Returns why the watchdog terminated the process, if it did.
    pub fn termination(&self) -> Option<Termination> {
        self.watch.termination()
    }

    /// Returns whether the process has already exited on its own.
    pub fn has_exited(&self) -> bool {
        self.child
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .try_wait()
            .is_ok_and(|status| status.is_some())
    }

    /// Kills the process, for example when the caller stops consuming its output.
    pub fn kill(&self) {
        let _ = self
            .child
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .kill();
    }

    /// Stops the watchdog and waits for the process to exit.
    ///
    /// Call it after the standard output has been consumed (or after
    /// [`kill`](Self::kill)). A process that does not exit within a grace
    /// period is killed and [`ProcessError::TimedOut`] is returned.
    pub fn finish(mut self) -> Result<StreamingOutcome, ProcessError> {
        self.stop_watchdog();
        let status = {
            let mut child = self.child.lock().unwrap_or_else(PoisonError::into_inner);
            let result = wait_with_deadline(&mut child, STREAMING_EXIT_GRACE);
            match result {
                Ok(Some(status)) => status,
                Ok(None) => {
                    terminate(&mut child);
                    self.reaped = true;
                    return Err(ProcessError::TimedOut);
                }
                Err(error) => {
                    terminate(&mut child);
                    self.reaped = true;
                    return Err(ProcessError::Wait(error));
                }
            }
        };
        self.reaped = true;
        Ok(StreamingOutcome {
            exit_code: status.code(),
            stderr: collect(&self.stderr),
            termination: self.termination(),
        })
    }

    fn stop_watchdog(&mut self) {
        self.watch.stopped.store(true, Ordering::Relaxed);
        if let Some(watchdog) = self.watchdog.take() {
            let _ = watchdog.join();
        }
    }
}

impl Drop for StreamingProcess {
    fn drop(&mut self) {
        self.stop_watchdog();
        if !self.reaped {
            terminate(&mut self.child.lock().unwrap_or_else(PoisonError::into_inner));
        }
    }
}

/// Standard output of a [`StreamingProcess`] that records read activity.
pub struct ActivityReader {
    inner: ChildStdout,
    watch: Arc<Watch>,
}

impl Read for ActivityReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let read = self.inner.read(buf)?;
        if read > 0 {
            self.watch.record_activity();
        }
        Ok(read)
    }
}

/// Starts a long-running program, such as a MAME game session, without a timeout.
///
/// Standard input is closed. Standard output and error go to `log` (or are
/// discarded) instead of pipes, so the process never blocks on a full pipe and
/// keeps running if the caller exits. The caller owns the returned child.
pub fn spawn_detached(mut command: Command, log: Option<File>) -> Result<Child, ProcessError> {
    command.stdin(Stdio::null());
    match log {
        Some(file) => {
            let stderr = file.try_clone().map_err(ProcessError::Spawn)?;
            command.stdout(file).stderr(stderr);
        }
        None => {
            command.stdout(Stdio::null()).stderr(Stdio::null());
        }
    }
    platform::configure_background_process(&mut command);
    command.spawn().map_err(ProcessError::Spawn)
}

fn spawn_piped(mut command: Command) -> Result<Child, ProcessError> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    platform::configure_background_process(&mut command);
    command.spawn().map_err(ProcessError::Spawn)
}

fn elapsed_millis(since: Instant) -> u64 {
    u64::try_from(since.elapsed().as_millis()).unwrap_or(u64::MAX)
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

    /// A command that runs for a long time and prints output periodically.
    fn long_running_command() -> Command {
        if cfg!(windows) {
            let mut command = Command::new("ping");
            command.args(["-n", "30", "127.0.0.1"]);
            command
        } else {
            let mut command = Command::new("sleep");
            command.arg("30");
            command
        }
    }

    /// A command that runs for a long time without printing anything.
    fn silent_command() -> Command {
        if cfg!(windows) {
            let mut command = Command::new("waitfor");
            command.args(["/t", "30", "YmdbTestSignalNeverSent"]);
            command
        } else {
            let mut command = Command::new("sleep");
            command.arg("30");
            command
        }
    }

    #[test]
    fn streams_stdout_and_reports_exit_code() {
        let mut process = spawn_streaming(
            Command::new("hostname"),
            Duration::from_secs(10),
            Arc::new(AtomicBool::new(false)),
        )
        .expect("spawn");
        let mut stdout = String::new();
        process
            .take_stdout()
            .expect("stdout")
            .read_to_string(&mut stdout)
            .expect("read");

        let outcome = process.finish().expect("finish");

        assert!(!stdout.trim().is_empty());
        assert_eq!(outcome.exit_code, Some(0));
        assert_eq!(outcome.termination, None);
    }

    #[test]
    fn cancellation_terminates_streaming_process() {
        let cancel = Arc::new(AtomicBool::new(false));
        let mut process = spawn_streaming(
            long_running_command(),
            Duration::from_secs(10),
            Arc::clone(&cancel),
        )
        .expect("spawn");
        let mut stdout = process.take_stdout().expect("stdout");
        let started = Instant::now();
        let canceller = thread::spawn(move || {
            thread::sleep(Duration::from_millis(300));
            cancel.store(true, Ordering::Relaxed);
        });

        // Reading ends when the killed process closes its output.
        let _ = io::copy(&mut stdout, &mut io::sink());
        let outcome = process.finish().expect("finish");
        canceller.join().expect("canceller");

        assert_eq!(outcome.termination, Some(Termination::Cancelled));
        assert_ne!(outcome.exit_code, Some(0));
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn inactivity_terminates_silent_streaming_process() {
        let mut process = spawn_streaming(
            silent_command(),
            Duration::from_millis(300),
            Arc::new(AtomicBool::new(false)),
        )
        .expect("spawn");
        let mut stdout = process.take_stdout().expect("stdout");
        let started = Instant::now();

        let _ = io::copy(&mut stdout, &mut io::sink());
        let outcome = process.finish().expect("finish");

        assert_eq!(outcome.termination, Some(Termination::Inactive));
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn dropping_streaming_process_terminates_it() {
        let started = Instant::now();
        let process = spawn_streaming(
            long_running_command(),
            Duration::from_secs(10),
            Arc::new(AtomicBool::new(false)),
        )
        .expect("spawn");

        drop(process);

        assert!(started.elapsed() < Duration::from_secs(10));
    }
}
