//! Commands of the Application Data settings section: the inventory of the
//! files YMDB uses, full backups and full restores.
//!
//! An export or the check of a backup to import runs on a dedicated thread;
//! the frontend polls [`get_backup_status`] and may cancel it. A checked
//! backup stays staged until it is applied (YMDB then restarts and replaces
//! its data at startup, see `app_data::restore`) or discarded.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use serde::Serialize;
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::DialogExt;

use super::catalog::CatalogState;
use super::installations::InstallationsState;
use super::launcher::LauncherState;
use super::roms::RomManagerState;
use super::{CommandError, run_blocking};
use crate::app_data::export::{self, SnapshotSources};
use crate::app_data::inventory::{self, DataEntry, ExternalReference};
use crate::app_data::manifest::FILE_SUFFIX;
use crate::app_data::restore::{self, RestoreOutcome, RestorePreview};
use crate::app_data::{AppDirs, BackupError, Phase, Progress, gate, now_ms};

/// Managed state of the Application Data section.
pub struct AppDataState {
    dirs: Option<AppDirs>,
    webview_dir: Option<PathBuf>,
    /// Shared with the thread of the running job.
    job: Arc<Mutex<Option<Job>>>,
    next_id: AtomicU64,
    /// Result of a restore applied at this start, reported once.
    outcome: Mutex<Option<RestoreOutcome>>,
}

struct Job {
    id: u64,
    kind: JobKind,
    progress: Arc<Progress>,
    result: Option<JobResult>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum JobKind {
    Export,
    ImportCheck,
}

enum JobResult {
    Exported { path: PathBuf, size: u64 },
    Prepared(Box<RestorePreview>),
    Failed(String),
    Cancelled,
}

/// Mirrors `BackupJobStatus` in `features/app-data/appDataApi.ts`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(
    tag = "state",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum BackupJobStatus {
    Idle,
    Running {
        job_id: u64,
        kind: JobKind,
        phase: Phase,
        bytes_done: u64,
        bytes_total: u64,
        cancel_requested: bool,
    },
    Exported {
        job_id: u64,
        path: PathBuf,
        size: u64,
    },
    /// A backup was checked and staged; it waits for confirmation.
    Prepared {
        job_id: u64,
        preview: Box<RestorePreview>,
    },
    Failed {
        job_id: u64,
        kind: JobKind,
        message: String,
    },
    Cancelled {
        job_id: u64,
        kind: JobKind,
    },
}

/// What the Application Data section shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplicationDataView {
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    /// `YMDB_DATA_DIR` puts both directories below one chosen directory.
    pub custom_data_dir: bool,
    pub entries: Vec<DataEntry>,
    pub external_references: Vec<ExternalReference>,
    pub restore_pending: bool,
}

impl AppDataState {
    pub fn new(
        dirs: Option<AppDirs>,
        webview_dir: Option<PathBuf>,
        outcome: Option<RestoreOutcome>,
    ) -> Self {
        Self {
            dirs,
            webview_dir,
            job: Arc::new(Mutex::new(None)),
            next_id: AtomicU64::new(1),
            outcome: Mutex::new(outcome),
        }
    }

    fn dirs(&self) -> Result<&AppDirs, CommandError> {
        self.dirs.as_ref().ok_or_else(|| CommandError::Settings {
            message: "the application data directories are unavailable".to_owned(),
        })
    }

    fn status(&self) -> BackupJobStatus {
        let job = self.job.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(job) = job.as_ref() else {
            return BackupJobStatus::Idle;
        };
        let (job_id, kind) = (job.id, job.kind);
        match &job.result {
            None => BackupJobStatus::Running {
                job_id,
                kind,
                phase: job.progress.phase(),
                bytes_done: job.progress.done(),
                bytes_total: job.progress.total(),
                cancel_requested: job.progress.is_cancel_requested(),
            },
            Some(JobResult::Exported { path, size }) => BackupJobStatus::Exported {
                job_id,
                path: path.clone(),
                size: *size,
            },
            Some(JobResult::Prepared(preview)) => BackupJobStatus::Prepared {
                job_id,
                preview: preview.clone(),
            },
            Some(JobResult::Failed(message)) => BackupJobStatus::Failed {
                job_id,
                kind,
                message: message.clone(),
            },
            Some(JobResult::Cancelled) => BackupJobStatus::Cancelled { job_id, kind },
        }
    }

    /// Registers a new job unless one is running or a restore is staged.
    fn begin(&self, kind: JobKind, phase: Phase) -> Result<(u64, Arc<Progress>), CommandError> {
        let mut job = self.job.lock().unwrap_or_else(PoisonError::into_inner);
        match job.as_ref().map(|job| &job.result) {
            Some(None) => return Err(busy("an export or import is already running")),
            Some(Some(JobResult::Prepared(_))) => {
                return Err(busy(
                    "a backup is ready to be restored: restore it or cancel it first",
                ));
            }
            _ => {}
        }
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let progress = Arc::new(Progress::new(phase));
        *job = Some(Job {
            id,
            kind,
            progress: Arc::clone(&progress),
            result: None,
        });
        Ok((id, progress))
    }

    fn end(&self, id: u64, result: JobResult) {
        if let Some(job) = self
            .job
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_mut()
            .filter(|job| job.id == id)
        {
            job.result = Some(result);
        }
    }

    fn prepared_job(&self) -> Option<u64> {
        self.job
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .filter(|job| matches!(job.result, Some(JobResult::Prepared(_))))
            .map(|job| job.id)
    }
}

fn busy(message: &str) -> CommandError {
    CommandError::Refused {
        message: message.to_owned(),
    }
}

fn job_result<T>(result: Result<T, BackupError>, done: impl FnOnce(T) -> JobResult) -> JobResult {
    match result {
        Ok(value) => done(value),
        Err(error) if error.is_cancelled() => JobResult::Cancelled,
        Err(error) => JobResult::Failed(error.to_string()),
    }
}

/// Refuses while MAME sessions, catalog imports or ROM downloads run: they
/// write data a backup contains (MAME saves its configuration in the
/// directories of controller profiles) or may start a session, and cannot be
/// paused.
fn check_quiet(app: &AppHandle) -> Result<(), BackupError> {
    let sessions = app.state::<LauncherState>().running_sessions_total();
    if sessions > 0 {
        return Err(BackupError::new(format!(
            "{sessions} MAME session(s) running. Quit MAME first: YMDB never stops MAME on its own"
        )));
    }
    if app.state::<CatalogState>().is_importing() {
        return Err(BackupError::new(
            "a catalog import is running. Wait for it to end or cancel it first",
        ));
    }
    if app.state::<RomManagerState>().is_downloading() {
        return Err(BackupError::new(
            "a ROM download is running. Wait for it to end or cancel it first",
        ));
    }
    Ok(())
}

/// The storage directory of the system web view, when YMDB knows it.
pub fn webview_data_dir(app_local_data_dir: Option<&Path>) -> Option<PathBuf> {
    if cfg!(windows) {
        // WebView2 keeps its user data folder in the local data directory
        // unless WEBVIEW2_USER_DATA_FOLDER chooses another one.
        std::env::var_os("WEBVIEW2_USER_DATA_FOLDER")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .or_else(|| app_local_data_dir.map(|dir| dir.join("EBWebView")))
    } else {
        // WebKit stores its data in system locations YMDB does not manage.
        None
    }
}

#[tauri::command]
pub async fn get_application_data(app: AppHandle) -> Result<ApplicationDataView, CommandError> {
    run_blocking(move || {
        let state = app.state::<AppDataState>();
        let dirs = state.dirs()?;
        Ok(ApplicationDataView {
            config_dir: dirs.config.clone(),
            data_dir: dirs.data.clone(),
            custom_data_dir: std::env::var_os("YMDB_DATA_DIR")
                .is_some_and(|value| !value.is_empty()),
            entries: inventory::entries(dirs, state.webview_dir.as_deref()),
            external_references: inventory::external_references(&dirs.config),
            restore_pending: restore::has_pending(dirs),
        })
    })
    .await?
}

/// Shows a location in the system file manager: a directory is opened, a
/// file is selected in its directory; a location not created yet opens its
/// nearest existing parent.
#[tauri::command]
pub async fn open_application_data_location(
    entry_id: String,
    app: AppHandle,
) -> Result<(), CommandError> {
    run_blocking(move || {
        let state = app.state::<AppDataState>();
        let path = inventory::entry_path(state.dirs()?, state.webview_dir.as_deref(), &entry_id)
            .ok_or_else(|| CommandError::NotFound {
                message: format!("unknown application data entry '{entry_id}'"),
            })?;
        let result = if path.is_file() {
            tauri_plugin_opener::reveal_item_in_dir(&path)
        } else {
            let existing = path.ancestors().find(|dir| dir.is_dir()).ok_or_else(|| {
                CommandError::NotFound {
                    message: format!("'{}' does not exist", path.display()),
                }
            })?;
            tauri_plugin_opener::open_path(existing, None::<&str>)
        };
        result.map_err(|error| CommandError::Internal {
            message: format!("cannot open '{}': {error}", path.display()),
        })
    })
    .await?
}

/// Lets the user choose the archive, then starts the export. `false` when
/// the user cancels the dialog.
#[tauri::command]
pub async fn start_backup_export(
    interface_preferences: Option<serde_json::Value>,
    app: AppHandle,
) -> Result<bool, CommandError> {
    run_blocking(move || {
        if let Some(preferences) = &interface_preferences {
            export::check_interface_preferences(preferences).map_err(|error| {
                CommandError::InvalidRequest {
                    message: error.to_string(),
                }
            })?;
        }
        let dirs = app.state::<AppDataState>().dirs()?.clone();
        let selection = app
            .dialog()
            .file()
            .set_title("Export YMDB backup")
            .set_file_name(default_file_name(now_ms()))
            .add_filter("YMDB backup", &["zip"])
            .blocking_save_file();
        let Some(selection) = selection else {
            return Ok(false);
        };
        let destination = selection
            .into_path()
            .map_err(|error| CommandError::Internal {
                message: format!("unsupported file selection: {error}"),
            })?;
        let state = app.state::<AppDataState>();
        let (id, progress) = state.begin(JobKind::Export, Phase::Snapshot)?;
        let handle = app.clone();
        std::thread::spawn(move || {
            let result = run_export(
                &handle,
                &dirs,
                &destination,
                interface_preferences,
                &progress,
            );
            handle.state::<AppDataState>().end(
                id,
                job_result(result, |size| JobResult::Exported {
                    path: destination,
                    size,
                }),
            );
        });
        Ok(true)
    })
    .await?
}

fn run_export(
    app: &AppHandle,
    dirs: &AppDirs,
    destination: &Path,
    interface_preferences: Option<serde_json::Value>,
    progress: &Progress,
) -> Result<u64, BackupError> {
    let work = export::work_dir(dirs)?;
    let installations = app.state::<InstallationsState>();
    let catalog = |id: &str| {
        installations.backup_catalog_store(id).unwrap_or_else(|_| {
            crate::catalog::store::CatalogStore::in_dir(&dirs.data.join("installations").join(id))
        })
    };
    let sources = SnapshotSources {
        dirs,
        catalog: &catalog,
        interface_preferences: interface_preferences.as_ref(),
    };
    let snapshot = {
        // Changes in progress finish first; new ones wait for the copy.
        let _quiet = gate::exclusive();
        check_quiet(app).and_then(|()| export::snapshot(&sources, &work, progress))
    };
    let version = app.package_info().version.to_string();
    let result = snapshot.and_then(|files| export::finish(&files, destination, &version, progress));
    let _ = std::fs::remove_dir_all(&work);
    result
}

/// `ymdb-YYYYMMDD-HHMM.ymdb-backup.zip`, in UTC.
fn default_file_name(now_ms: u64) -> String {
    let minutes = now_ms / 60_000;
    let (year, month, day) = civil_date(minutes / (24 * 60));
    let hour = minutes / 60 % 24;
    let minute = minutes % 60;
    format!("ymdb-{year:04}{month:02}{day:02}-{hour:02}{minute:02}{FILE_SUFFIX}")
}

/// Gregorian date of a day count since 1970-01-01 (Howard Hinnant's algorithm).
fn civil_date(days: u64) -> (u64, u64, u64) {
    let z = days + 719_468;
    let era = z / 146_097;
    let day_of_era = z % 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + u64::from(month <= 2);
    (year, month, day)
}

/// Lets the user choose a backup, then checks and stages it on a thread;
/// the preview is reported by [`get_backup_status`]. `false` when the user
/// cancels the dialog. The current data is not modified.
#[tauri::command]
pub async fn start_backup_import_check(app: AppHandle) -> Result<bool, CommandError> {
    run_blocking(move || {
        let dirs = app.state::<AppDataState>().dirs()?.clone();
        let selection = app
            .dialog()
            .file()
            .set_title("Import YMDB backup")
            .add_filter("YMDB backup", &["zip"])
            .blocking_pick_file();
        let Some(selection) = selection else {
            return Ok(false);
        };
        let archive = selection
            .into_path()
            .map_err(|error| CommandError::Internal {
                message: format!("unsupported file selection: {error}"),
            })?;
        let (id, progress) = app
            .state::<AppDataState>()
            .begin(JobKind::ImportCheck, Phase::Checking)?;
        let handle = app.clone();
        std::thread::spawn(move || {
            let version = handle.package_info().version.to_string();
            let result = restore::stage(&archive, &dirs, &version, &progress);
            handle.state::<AppDataState>().end(
                id,
                job_result(result, |preview| JobResult::Prepared(Box::new(preview))),
            );
        });
        Ok(true)
    })
    .await?
}

#[tauri::command]
pub fn get_backup_status(app: AppHandle) -> BackupJobStatus {
    app.state::<AppDataState>().status()
}

/// Asks the running export or import check to stop.
#[tauri::command]
pub fn cancel_backup_job(app: AppHandle) {
    if let Some(job) = app
        .state::<AppDataState>()
        .job
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()
        .filter(|job| job.result.is_none())
    {
        job.progress.request_cancel();
    }
}

/// Removes a staged backup that the user does not restore, and forgets
/// the finished job.
#[tauri::command]
pub async fn discard_backup_job(app: AppHandle) -> Result<(), CommandError> {
    run_blocking(move || {
        let state = app.state::<AppDataState>();
        let mut job = state.job.lock().unwrap_or_else(PoisonError::into_inner);
        if job.as_ref().is_some_and(|job| job.result.is_none()) {
            return Err(busy("the operation is still running"));
        }
        restore::discard(state.dirs()?);
        *job = None;
        Ok(())
    })
    .await?
}

/// Confirms the restore of the staged backup `job_id` and restarts YMDB,
/// which replaces its data before opening it. Does not return on success.
#[tauri::command]
pub async fn apply_backup_import(job_id: u64, app: AppHandle) -> Result<(), CommandError> {
    run_blocking(move || {
        let state = app.state::<AppDataState>();
        let dirs = state.dirs()?.clone();
        if state.prepared_job() != Some(job_id) {
            return Err(busy("this backup is no longer ready; choose it again"));
        }
        // Held until the process exits: no change can start in between.
        let _quiet = gate::exclusive();
        check_quiet(&app)
            .and_then(|()| restore::schedule(&dirs))
            .map_err(|error| CommandError::Refused {
                message: error.to_string(),
            })?;
        app.state::<InstallationsState>().release_catalog_readers();
        app.restart()
    })
    .await?
}

/// The result of a restore applied at this start, once.
#[tauri::command]
pub fn take_restore_outcome(app: AppHandle) -> Option<RestoreOutcome> {
    app.state::<AppDataState>()
        .outcome
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_file_names_carry_the_utc_date() {
        assert_eq!(default_file_name(0), "ymdb-19700101-0000.ymdb-backup.zip");
        // 2026-10-24 13:05 UTC.
        assert_eq!(
            default_file_name(1_792_847_100_000),
            "ymdb-20261024-1305.ymdb-backup.zip"
        );
        assert_eq!(civil_date(11_016), (2000, 2, 29));
    }

    #[test]
    fn serializes_job_statuses_to_frontend_contract() {
        assert_eq!(
            serde_json::to_value(BackupJobStatus::Running {
                job_id: 2,
                kind: JobKind::ImportCheck,
                phase: Phase::Extracting,
                bytes_done: 5,
                bytes_total: 9,
                cancel_requested: false,
            })
            .expect("serialize"),
            serde_json::json!({
                "state": "running",
                "jobId": 2,
                "kind": "importCheck",
                "phase": "extracting",
                "bytesDone": 5,
                "bytesTotal": 9,
                "cancelRequested": false
            })
        );
        assert_eq!(
            serde_json::to_value(BackupJobStatus::Failed {
                job_id: 3,
                kind: JobKind::Export,
                message: "disk full".to_owned(),
            })
            .expect("serialize"),
            serde_json::json!({ "state": "failed", "jobId": 3, "kind": "export", "message": "disk full" })
        );
        assert_eq!(
            serde_json::to_value(RestoreOutcome::Failed {
                message: "m".to_owned(),
                previous_data_kept: true,
            })
            .expect("serialize"),
            serde_json::json!({ "kind": "failed", "message": "m", "previousDataKept": true })
        );
    }
}
