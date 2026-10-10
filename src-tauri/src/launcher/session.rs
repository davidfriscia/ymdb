//! MAME sessions started by the launcher.
//!
//! Each session runs without a timeout (a game can last hours). A monitor
//! thread checks the process periodically and records how it ended. Sessions
//! are never terminated implicitly: only an explicit [`SessionManager::stop`]
//! kills a process, and closing YMDB leaves MAME running.

use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::Child;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;

use super::command::LaunchPlan;
use crate::mame::process::{self, ProcessError};

const POLL_INTERVAL: Duration = Duration::from_millis(250);
/// Finished sessions kept in memory, and log files kept on disk.
const HISTORY: usize = 20;
const LOG_TAIL_BYTES: u64 = 4096;

/// What a session runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchTarget {
    pub machine: String,
    pub software: Option<SoftwareRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SoftwareRef {
    pub list: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "state",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SessionState {
    Running,
    /// MAME exited on its own.
    Exited {
        /// `None` when terminated by a signal.
        exit_code: Option<i32>,
        success: bool,
        message: String,
        /// End of MAME's output, for diagnostics.
        log_tail: String,
        finished_at: u64,
    },
    /// The user stopped the session.
    Stopped {
        finished_at: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub id: u64,
    /// The installation the session was started from.
    pub installation_id: String,
    /// Name of that installation when the session started, for display
    /// even after the installation is renamed or removed.
    pub installation_label: String,
    pub target: LaunchTarget,
    /// Description of the machine or software, for display.
    pub title: String,
    pub command_line: Vec<String>,
    /// Milliseconds since the Unix epoch.
    pub started_at: u64,
    pub state: SessionState,
}

#[derive(Debug)]
pub enum SessionError {
    /// A session for the same target and installation is still running.
    AlreadyRunning(u64),
    Spawn(io::Error),
    NotFound,
    NotRunning,
}

struct Session {
    info: SessionInfo,
    child: Child,
    stop_requested: bool,
    log: Option<PathBuf>,
}

/// Tracks the sessions started during this run of YMDB.
pub struct SessionManager {
    sessions: Arc<Mutex<Vec<Session>>>,
    next_id: AtomicU64,
    /// `None` when output is discarded (no writable data directory).
    log_dir: Option<PathBuf>,
    poll_interval: Duration,
}

impl SessionManager {
    pub fn new(log_dir: Option<PathBuf>) -> Self {
        Self::with_poll_interval(log_dir, POLL_INTERVAL)
    }

    fn with_poll_interval(log_dir: Option<PathBuf>, poll_interval: Duration) -> Self {
        Self {
            sessions: Arc::new(Mutex::new(Vec::new())),
            next_id: AtomicU64::new(1),
            log_dir,
            poll_interval,
        }
    }

    /// Starts MAME for `plan` and begins monitoring it. The same target may
    /// run once per installation at a time.
    pub fn start(
        &self,
        target: LaunchTarget,
        title: String,
        installation_label: String,
        plan: &LaunchPlan,
    ) -> Result<SessionInfo, SessionError> {
        let mut sessions = self.lock();
        if let Some(running) = sessions.iter().find(|session| {
            session.info.target == target
                && session.info.installation_id == plan.installation_id
                && session.info.state == SessionState::Running
        }) {
            return Err(SessionError::AlreadyRunning(running.info.id));
        }
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let started_at = now_millis();
        let log = self
            .log_dir
            .as_deref()
            .and_then(|dir| create_log(dir, started_at, id, &target.machine));
        let file = log.as_ref().and_then(|path| File::create(path).ok());
        let child = process::spawn_detached(plan.command(), file).map_err(|error| match error {
            ProcessError::Spawn(error) | ProcessError::Wait(error) => SessionError::Spawn(error),
            ProcessError::TimedOut => SessionError::Spawn(io::Error::other("timed out")),
        })?;
        let info = SessionInfo {
            id,
            installation_id: plan.installation_id.clone(),
            installation_label,
            target,
            title,
            command_line: plan.command_line(),
            started_at,
            state: SessionState::Running,
        };
        sessions.push(Session {
            info: info.clone(),
            child,
            stop_requested: false,
            log,
        });
        prune_history(&mut sessions);
        drop(sessions);
        self.monitor(id);
        Ok(info)
    }

    /// Sessions of this run, most recent first.
    pub fn list(&self) -> Vec<SessionInfo> {
        self.lock()
            .iter()
            .rev()
            .map(|session| session.info.clone())
            .collect()
    }

    /// Number of running sessions started from an installation.
    pub fn running_for(&self, installation_id: &str) -> usize {
        self.lock()
            .iter()
            .filter(|session| {
                session.info.installation_id == installation_id
                    && session.info.state == SessionState::Running
            })
            .count()
    }

    /// Kills a running session. MAME cannot save NVRAM or its configuration.
    pub fn stop(&self, id: u64) -> Result<SessionInfo, SessionError> {
        let mut sessions = self.lock();
        let session = sessions
            .iter_mut()
            .find(|session| session.info.id == id)
            .ok_or(SessionError::NotFound)?;
        if session.info.state != SessionState::Running {
            return Err(SessionError::NotRunning);
        }
        session.stop_requested = true;
        // The process may have exited in the meantime; the monitor records it.
        let _ = session.child.kill();
        if let Ok(Some(_)) = session.child.try_wait() {
            session.info.state = SessionState::Stopped {
                finished_at: now_millis(),
            };
        }
        Ok(session.info.clone())
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<Session>> {
        self.sessions.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn monitor(&self, id: u64) {
        let sessions = Arc::clone(&self.sessions);
        let poll_interval = self.poll_interval;
        thread::spawn(move || {
            loop {
                thread::sleep(poll_interval);
                let mut sessions = sessions.lock().unwrap_or_else(PoisonError::into_inner);
                let Some(session) = sessions.iter_mut().find(|session| session.info.id == id)
                else {
                    return;
                };
                if session.info.state != SessionState::Running {
                    return;
                }
                match session.child.try_wait() {
                    Ok(None) => {}
                    Ok(Some(status)) => {
                        session.info.state = finished_state(session, status.code());
                        return;
                    }
                    Err(error) => {
                        session.info.state = SessionState::Exited {
                            exit_code: None,
                            success: false,
                            message: format!("The MAME process could not be monitored: {error}"),
                            log_tail: String::new(),
                            finished_at: now_millis(),
                        };
                        return;
                    }
                }
            }
        });
    }
}

fn finished_state(session: &Session, exit_code: Option<i32>) -> SessionState {
    let finished_at = now_millis();
    if session.stop_requested {
        return SessionState::Stopped { finished_at };
    }
    SessionState::Exited {
        exit_code,
        success: exit_code == Some(0),
        message: describe_exit(exit_code),
        log_tail: session.log.as_deref().map(log_tail).unwrap_or_default(),
        finished_at,
    }
}

/// Meaning of MAME's exit codes (`emu.h`).
pub fn describe_exit(exit_code: Option<i32>) -> String {
    match exit_code {
        Some(0) => "MAME exited normally.".to_owned(),
        Some(1) => "MAME failed its validity checks.".to_owned(),
        Some(2) => "Required files are missing (ROMs, CHDs or software media).".to_owned(),
        Some(3) => "MAME stopped with a fatal error.".to_owned(),
        Some(4) => "A media device or software image could not be loaded.".to_owned(),
        Some(5) => "MAME does not know this machine.".to_owned(),
        Some(6) => "MAME rejected the configuration (unknown option or software).".to_owned(),
        Some(code) => format!("MAME exited with code {code}."),
        None => "MAME was terminated without an exit code.".to_owned(),
    }
}

/// Keeps every running session and the most recent finished ones.
fn prune_history(sessions: &mut Vec<Session>) {
    let finished = sessions
        .iter()
        .filter(|session| session.info.state != SessionState::Running)
        .count();
    let mut excess = finished.saturating_sub(HISTORY);
    sessions.retain(|session| {
        if excess > 0 && session.info.state != SessionState::Running {
            excess -= 1;
            false
        } else {
            true
        }
    });
}

/// Creates the log directory and returns a new log path, deleting old logs.
fn create_log(dir: &Path, started_at: u64, id: u64, machine: &str) -> Option<PathBuf> {
    fs::create_dir_all(dir).ok()?;
    let mut logs: Vec<PathBuf> = fs::read_dir(dir)
        .ok()?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|extension| extension == "log"))
        .collect();
    // Names start with a zero-padded timestamp, so they sort chronologically.
    logs.sort();
    let excess = (logs.len() + 1).saturating_sub(HISTORY);
    for old in logs.iter().take(excess) {
        let _ = fs::remove_file(old);
    }
    Some(dir.join(format!("{started_at:015}-{id}-{machine}.log")))
}

/// The last lines of a log file, without a partial first line.
fn log_tail(path: &Path) -> String {
    let Ok(mut file) = File::open(path) else {
        return String::new();
    };
    let length = file.metadata().map(|metadata| metadata.len()).unwrap_or(0);
    let start = length.saturating_sub(LOG_TAIL_BYTES);
    let mut bytes = Vec::new();
    if file.seek(SeekFrom::Start(start)).is_err() || file.read_to_end(&mut bytes).is_err() {
        return String::new();
    }
    let text = String::from_utf8_lossy(&bytes);
    let text = if start > 0 {
        text.split_once('\n').map_or("", |(_, rest)| rest)
    } else {
        &text
    };
    text.trim().to_owned()
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|elapsed| u64::try_from(elapsed.as_millis()).ok())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;
    use crate::launcher::command::{ArgumentRole, PlanArgument};

    /// A plan running a test program from `dir` instead of MAME.
    fn plan(program: &str, args: &[&str], dir: &Path) -> LaunchPlan {
        plan_for(FIRST, program, args, dir)
    }

    const FIRST: &str = "0123456789abcdef0123456789abcdef";
    const SECOND: &str = "fedcba9876543210fedcba9876543210";

    fn plan_for(installation_id: &str, program: &str, args: &[&str], dir: &Path) -> LaunchPlan {
        LaunchPlan {
            installation_id: installation_id.to_owned(),
            profile_id: "default".to_owned(),
            executable: PathBuf::from(program),
            working_directory: dir.to_path_buf(),
            arguments: args
                .iter()
                .map(|&arg| PlanArgument {
                    role: ArgumentRole::Machine,
                    tokens: vec![arg.to_owned()],
                })
                .collect(),
        }
    }

    /// Runs for a long time without output.
    fn long_running(dir: &Path) -> LaunchPlan {
        long_running_for(FIRST, dir)
    }

    fn long_running_for(installation_id: &str, dir: &Path) -> LaunchPlan {
        if cfg!(windows) {
            plan_for(installation_id, "ping", &["-n", "60", "127.0.0.1"], dir)
        } else {
            plan_for(installation_id, "sleep", &["60"], dir)
        }
    }

    /// Prints a line and exits with `code`.
    fn exiting(dir: &Path, code: i32) -> LaunchPlan {
        if cfg!(windows) {
            let script = format!("echo missing rom & exit {code}");
            plan("cmd", &["/C", script.as_str()], dir)
        } else {
            let script = format!("echo missing rom; exit {code}");
            plan("sh", &["-c", script.as_str()], dir)
        }
    }

    fn target(machine: &str) -> LaunchTarget {
        LaunchTarget {
            machine: machine.to_owned(),
            software: None,
        }
    }

    fn manager(dir: &Path) -> SessionManager {
        SessionManager::with_poll_interval(Some(dir.join("launch-logs")), Duration::from_millis(20))
    }

    fn wait_until_finished(manager: &SessionManager, id: u64) -> SessionInfo {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let info = manager
                .list()
                .into_iter()
                .find(|session| session.id == id)
                .expect("session");
            if info.state != SessionState::Running || Instant::now() > deadline {
                return info;
            }
            thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn records_exit_codes_and_output() {
        let dir = tempfile::tempdir().expect("temp dir");
        let manager = manager(dir.path());

        let started = manager
            .start(
                target("pacman"),
                "Pac-Man".to_owned(),
                "MAME 0.280".to_owned(),
                &exiting(dir.path(), 2),
            )
            .expect("start");
        assert_eq!(started.state, SessionState::Running);
        match wait_until_finished(&manager, started.id).state {
            SessionState::Exited {
                exit_code,
                success,
                message,
                log_tail,
                ..
            } => {
                assert_eq!(exit_code, Some(2));
                assert!(!success);
                assert!(message.contains("missing"), "{message}");
                assert!(log_tail.contains("missing rom"), "{log_tail}");
            }
            other => panic!("unexpected state {other:?}"),
        }

        let ok = manager
            .start(
                target("pacman"),
                "Pac-Man".to_owned(),
                "MAME 0.280".to_owned(),
                &exiting(dir.path(), 0),
            )
            .expect("start again after exit");
        assert!(matches!(
            wait_until_finished(&manager, ok.id).state,
            SessionState::Exited {
                exit_code: Some(0),
                success: true,
                ..
            }
        ));
    }

    #[test]
    fn long_sessions_run_until_stopped_and_duplicates_are_refused() {
        let dir = tempfile::tempdir().expect("temp dir");
        let manager = manager(dir.path());
        let session = manager
            .start(
                target("galaga"),
                "Galaga".to_owned(),
                "MAME 0.280".to_owned(),
                &long_running(dir.path()),
            )
            .expect("start");

        // No timeout: still running after many monitor cycles.
        thread::sleep(Duration::from_millis(600));
        assert_eq!(manager.list()[0].state, SessionState::Running);

        // The same target cannot be started twice while running.
        assert!(matches!(
                   manager.start(target("galaga"), "Galaga".to_owned(),
        "MAME 0.280".to_owned(), &long_running(dir.path())),
                   Err(SessionError::AlreadyRunning(id)) if id == session.id
               ));
        // Another target can.
        let other = manager
            .start(
                target("pacman"),
                "Pac-Man".to_owned(),
                "MAME 0.280".to_owned(),
                &long_running(dir.path()),
            )
            .expect("other target");

        manager.stop(session.id).expect("stop");
        assert!(matches!(
            wait_until_finished(&manager, session.id).state,
            SessionState::Stopped { .. }
        ));
        assert!(matches!(
            manager.stop(session.id),
            Err(SessionError::NotRunning)
        ));
        assert!(matches!(manager.stop(999), Err(SessionError::NotFound)));

        manager.stop(other.id).expect("stop other");
        wait_until_finished(&manager, other.id);
        let ids: Vec<u64> = manager.list().iter().map(|session| session.id).collect();
        assert_eq!(ids, [other.id, session.id]);
    }

    #[test]
    fn the_same_target_runs_once_per_installation() {
        let dir = tempfile::tempdir().expect("temp dir");
        let manager = manager(dir.path());
        let first = manager
            .start(
                target("galaga"),
                "Galaga".to_owned(),
                "MAME 0.279".to_owned(),
                &long_running_for(FIRST, dir.path()),
            )
            .expect("first installation");
        let second = manager
            .start(
                target("galaga"),
                "Galaga".to_owned(),
                "MAME 0.280".to_owned(),
                &long_running_for(SECOND, dir.path()),
            )
            .expect("same target, other installation");
        assert!(matches!(
            manager.start(
                target("galaga"),
                "Galaga".to_owned(),
                "MAME 0.280".to_owned(),
                &long_running_for(SECOND, dir.path()),
            ),
            Err(SessionError::AlreadyRunning(id)) if id == second.id
        ));
        assert_eq!(manager.running_for(FIRST), 1);
        assert_eq!(manager.running_for(SECOND), 1);
        let listed = manager.list();
        assert_eq!(
            listed
                .iter()
                .map(|session| (
                    session.installation_id.as_str(),
                    session.installation_label.as_str()
                ))
                .collect::<Vec<_>>(),
            [(SECOND, "MAME 0.280"), (FIRST, "MAME 0.279")]
        );

        // Stopping one installation's session leaves the other running.
        manager.stop(first.id).expect("stop");
        wait_until_finished(&manager, first.id);
        assert_eq!(manager.running_for(FIRST), 0);
        assert_eq!(manager.list()[0].state, SessionState::Running);
        manager.stop(second.id).expect("stop");
        wait_until_finished(&manager, second.id);
    }

    #[test]
    fn reports_spawn_errors_for_missing_executables() {
        let dir = tempfile::tempdir().expect("temp dir");
        let manager = manager(dir.path());
        let missing = dir.path().join("Émulateurs MAME ✓").join("mame-missing");
        let result = manager.start(
            target("pacman"),
            "Pac-Man".to_owned(),
            "MAME 0.280".to_owned(),
            &plan(missing.to_str().expect("utf-8"), &["pacman"], dir.path()),
        );
        assert!(matches!(result, Err(SessionError::Spawn(_))));
        assert!(manager.list().is_empty());
    }

    #[test]
    fn keeps_a_bounded_number_of_logs() {
        let dir = tempfile::tempdir().expect("temp dir");
        let logs = dir.path().join("logs");
        for index in 0..25_u64 {
            let path = create_log(&logs, 1_000 + index, index, "pacman").expect("log");
            fs::write(path, "x").expect("write");
        }
        let count = fs::read_dir(&logs).expect("read").count();
        assert_eq!(count, HISTORY);
    }

    #[test]
    fn reads_the_end_of_long_logs() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("session.log");
        let mut content = "noise line\n".repeat(1000);
        content.push_str("Fatal error: Required files are missing\n");
        fs::write(&path, content).expect("write");
        let tail = log_tail(&path);
        assert!(tail.ends_with("Fatal error: Required files are missing"));
        assert!(tail.len() <= 4096);
        assert!(tail.starts_with("noise line"));
    }

    #[test]
    fn describes_mame_exit_codes() {
        assert!(describe_exit(Some(2)).contains("missing"));
        assert!(describe_exit(Some(5)).contains("machine"));
        assert_eq!(describe_exit(Some(42)), "MAME exited with code 42.");
    }
}
