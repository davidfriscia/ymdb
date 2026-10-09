//! MAME catalog commands.
//!
//! An import runs on a dedicated thread. The frontend polls
//! [`get_catalog_import_status`] (cheap, in-memory) while an import runs and
//! [`get_catalog_status`] (reads the catalog database) when it needs the
//! catalog summary.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use super::{CommandError, SettingsState};
use crate::catalog::freshness::{CatalogFreshness, check_freshness};
use crate::catalog::import::{ImportError, ImportPhase, ImportProgress, import_catalog};
use crate::catalog::store::{CatalogInfo, CatalogStore};

/// Current or last import, as reported to the frontend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "state",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ImportStatus {
    /// No import has run since the application started.
    Idle,
    Running {
        phase: ImportPhase,
        machines_processed: u64,
        started_at: u64,
        cancel_requested: bool,
    },
    Completed {
        machines: u64,
        duration_ms: u64,
        finished_at: u64,
    },
    Failed {
        error: ImportError,
        finished_at: u64,
    },
    Cancelled {
        finished_at: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogStatus {
    pub catalog: Option<CatalogInfo>,
    /// Set when a catalog file exists but cannot be used.
    pub catalog_error: Option<String>,
    /// `None` when there is no usable catalog.
    pub freshness: Option<CatalogFreshness>,
    pub mame_configured: bool,
    pub import: ImportStatus,
}

/// Managed state of the catalog feature.
pub struct CatalogState {
    /// `None` when the platform data directory could not be resolved.
    store: Option<CatalogStore>,
    job: Mutex<ImportJob>,
}

#[derive(Default)]
struct ImportJob {
    running: Option<RunningImport>,
    last: Option<ImportStatus>,
}

struct RunningImport {
    cancel: Arc<AtomicBool>,
    progress: Arc<ImportProgress>,
    started_at: u64,
}

impl CatalogState {
    pub fn new(store: Option<CatalogStore>) -> Self {
        Self {
            store,
            job: Mutex::new(ImportJob::default()),
        }
    }

    fn store(&self) -> Result<&CatalogStore, CommandError> {
        self.store.as_ref().ok_or_else(|| CommandError::Internal {
            message: "the application data directory is unavailable".to_owned(),
        })
    }

    fn import_status(&self) -> ImportStatus {
        let job = self.job.lock().unwrap_or_else(PoisonError::into_inner);
        match &job.running {
            Some(running) => ImportStatus::Running {
                phase: running.progress.phase(),
                machines_processed: running.progress.machines_processed(),
                started_at: running.started_at,
                cancel_requested: running.cancel.load(Ordering::Relaxed),
            },
            None => job.last.clone().unwrap_or(ImportStatus::Idle),
        }
    }

    fn finish(&self, result: Result<CatalogInfo, ImportError>) {
        let finished_at = now_millis();
        let status = match result {
            Ok(info) => ImportStatus::Completed {
                machines: info.statistics.machines,
                duration_ms: info.import_duration_ms,
                finished_at,
            },
            Err(ImportError::Cancelled) => ImportStatus::Cancelled { finished_at },
            Err(error) => ImportStatus::Failed { error, finished_at },
        };
        let mut job = self.job.lock().unwrap_or_else(PoisonError::into_inner);
        job.running = None;
        job.last = Some(status);
    }
}

#[tauri::command]
pub fn get_catalog_status(
    settings: State<'_, SettingsState>,
    catalog: State<'_, CatalogState>,
) -> Result<CatalogStatus, CommandError> {
    // A broken settings file is reported by the MAME installation panel; here
    // it only means that no executable is known.
    let configured = settings.configured_executable().ok().flatten();
    let (info, catalog_error) = match catalog.store()?.read_info() {
        Ok(info) => (info, None),
        Err(error) => (None, Some(error.to_string())),
    };
    let freshness = info
        .as_ref()
        .map(|info| check_freshness(info, configured.as_deref()));
    Ok(CatalogStatus {
        catalog: info,
        catalog_error,
        freshness,
        mame_configured: configured.is_some(),
        import: catalog.import_status(),
    })
}

#[tauri::command]
pub fn get_catalog_import_status(catalog: State<'_, CatalogState>) -> ImportStatus {
    catalog.import_status()
}

/// Starts importing the catalog from the configured MAME executable.
#[tauri::command]
pub fn start_catalog_import(
    app: AppHandle,
    settings: State<'_, SettingsState>,
    catalog: State<'_, CatalogState>,
) -> Result<ImportStatus, CommandError> {
    let executable: PathBuf = settings
        .configured_executable()?
        .ok_or(CommandError::NotConfigured)?;
    let store = catalog.store()?.clone();

    let cancel = Arc::new(AtomicBool::new(false));
    let progress = Arc::new(ImportProgress::default());
    {
        let mut job = catalog.job.lock().unwrap_or_else(PoisonError::into_inner);
        if job.running.is_some() {
            return Err(CommandError::ImportInProgress);
        }
        job.running = Some(RunningImport {
            cancel: Arc::clone(&cancel),
            progress: Arc::clone(&progress),
            started_at: now_millis(),
        });
    }

    let spawned = thread::Builder::new()
        .name("catalog-import".to_owned())
        .spawn(move || {
            let result = import_catalog(&executable, &store, &cancel, &progress);
            app.state::<CatalogState>().finish(result);
        });
    if let Err(error) = spawned {
        catalog.finish(Err(ImportError::LaunchFailed {
            message: format!("failed to start the import thread: {error}"),
        }));
    }
    Ok(catalog.import_status())
}

/// Requests cancellation of the running import, if any.
#[tauri::command]
pub fn cancel_catalog_import(catalog: State<'_, CatalogState>) -> ImportStatus {
    if let Some(running) = &catalog
        .job
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .running
    {
        running.cancel.store(true, Ordering::Relaxed);
    }
    catalog.import_status()
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

    #[test]
    fn reports_idle_then_running_then_last_outcome() {
        let state = CatalogState::new(None);
        assert_eq!(state.import_status(), ImportStatus::Idle);

        let progress = Arc::new(ImportProgress::default());
        state.job.lock().expect("lock").running = Some(RunningImport {
            cancel: Arc::new(AtomicBool::new(false)),
            progress,
            started_at: 42,
        });
        assert!(matches!(
            state.import_status(),
            ImportStatus::Running {
                phase: ImportPhase::Validating,
                machines_processed: 0,
                started_at: 42,
                cancel_requested: false
            }
        ));

        state.finish(Err(ImportError::Cancelled));
        assert!(matches!(
            state.import_status(),
            ImportStatus::Cancelled { .. }
        ));

        state.finish(Err(ImportError::Parse {
            message: "bad".to_owned(),
        }));
        assert!(matches!(
            state.import_status(),
            ImportStatus::Failed {
                error: ImportError::Parse { .. },
                ..
            }
        ));
    }

    #[test]
    fn serializes_status_to_frontend_contract() {
        let running = ImportStatus::Running {
            phase: ImportPhase::Reading,
            machines_processed: 1200,
            started_at: 7,
            cancel_requested: false,
        };

        assert_eq!(
            serde_json::to_value(running).expect("serialize"),
            serde_json::json!({
                "state": "running",
                "phase": "reading",
                "machinesProcessed": 1200,
                "startedAt": 7,
                "cancelRequested": false
            })
        );
        assert_eq!(
            serde_json::to_value(ImportStatus::Failed {
                error: ImportError::Cancelled,
                finished_at: 1
            })
            .expect("serialize"),
            serde_json::json!({ "state": "failed", "error": { "kind": "cancelled" }, "finishedAt": 1 })
        );
    }
}
