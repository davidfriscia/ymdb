//! MAME catalog commands.
//!
//! An import (of machines or of software lists) runs on a dedicated thread.
//! Only one import runs at a time, so the catalog file is never replaced by
//! two operations concurrently. The frontend polls
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
use crate::catalog::import::{
    ImportError, ImportPhase, ImportProgress, import_catalog, import_software_lists,
};
use crate::catalog::store::{CatalogInfo, CatalogStore};

/// What an import reads from MAME.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ImportKind {
    /// Machines from `-listxml`; replaces the whole catalog.
    Machines,
    /// Software lists from `-listsoftware`; replaces the software lists only.
    SoftwareLists,
}

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
        kind: ImportKind,
        phase: ImportPhase,
        /// Machines or software stored so far, depending on `kind`.
        items_processed: u64,
        started_at: u64,
        cancel_requested: bool,
    },
    Completed {
        kind: ImportKind,
        /// Machines or software imported, depending on `kind`.
        items: u64,
        duration_ms: u64,
        finished_at: u64,
    },
    Failed {
        kind: ImportKind,
        error: ImportError,
        finished_at: u64,
    },
    Cancelled {
        kind: ImportKind,
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
    kind: ImportKind,
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

    pub(super) fn store(&self) -> Result<&CatalogStore, CommandError> {
        self.store.as_ref().ok_or_else(|| CommandError::Internal {
            message: "the application data directory is unavailable".to_owned(),
        })
    }

    fn import_status(&self) -> ImportStatus {
        let job = self.job.lock().unwrap_or_else(PoisonError::into_inner);
        match &job.running {
            Some(running) => ImportStatus::Running {
                kind: running.kind,
                phase: running.progress.phase(),
                items_processed: running.progress.items_processed(),
                started_at: running.started_at,
                cancel_requested: running.cancel.load(Ordering::Relaxed),
            },
            None => job.last.clone().unwrap_or(ImportStatus::Idle),
        }
    }

    fn finish(&self, kind: ImportKind, result: Result<CatalogInfo, ImportError>) {
        let finished_at = now_millis();
        let status = match result {
            Ok(info) => ImportStatus::Completed {
                kind,
                items: match kind {
                    ImportKind::Machines => info.statistics.machines,
                    ImportKind::SoftwareLists => info
                        .software
                        .as_ref()
                        .map_or(0, |software| software.statistics.software),
                },
                duration_ms: match kind {
                    ImportKind::Machines => info.import_duration_ms,
                    ImportKind::SoftwareLists => info
                        .software
                        .as_ref()
                        .map_or(0, |software| software.import_duration_ms),
                },
                finished_at,
            },
            Err(ImportError::Cancelled) => ImportStatus::Cancelled { kind, finished_at },
            Err(error) => ImportStatus::Failed {
                kind,
                error,
                finished_at,
            },
        };
        let mut job = self.job.lock().unwrap_or_else(PoisonError::into_inner);
        job.running = None;
        job.last = Some(status);
    }

    /// Starts an import on a dedicated thread, unless one is already running.
    fn start(
        &self,
        app: AppHandle,
        kind: ImportKind,
        executable: PathBuf,
    ) -> Result<ImportStatus, CommandError> {
        let store = self.store()?.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let progress = Arc::new(ImportProgress::default());
        {
            let mut job = self.job.lock().unwrap_or_else(PoisonError::into_inner);
            if job.running.is_some() {
                return Err(CommandError::ImportInProgress);
            }
            job.running = Some(RunningImport {
                kind,
                cancel: Arc::clone(&cancel),
                progress: Arc::clone(&progress),
                started_at: now_millis(),
            });
        }

        let spawned = thread::Builder::new()
            .name("catalog-import".to_owned())
            .spawn(move || {
                let result = match kind {
                    ImportKind::Machines => import_catalog(&executable, &store, &cancel, &progress),
                    ImportKind::SoftwareLists => {
                        import_software_lists(&executable, &store, &cancel, &progress)
                    }
                };
                app.state::<CatalogState>().finish(kind, result);
            });
        if let Err(error) = spawned {
            self.finish(
                kind,
                Err(ImportError::LaunchFailed {
                    message: format!("failed to start the import thread: {error}"),
                }),
            );
        }
        Ok(self.import_status())
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

/// Starts importing the machine catalog from the configured MAME executable.
/// The new catalog replaces the current one, including its software lists.
#[tauri::command]
pub fn start_catalog_import(
    app: AppHandle,
    settings: State<'_, SettingsState>,
    catalog: State<'_, CatalogState>,
) -> Result<ImportStatus, CommandError> {
    let executable = settings
        .configured_executable()?
        .ok_or(CommandError::NotConfigured)?;
    catalog.start(app, ImportKind::Machines, executable)
}

/// Starts importing the software lists from the configured MAME executable,
/// which must be the one the machine catalog was imported from.
#[tauri::command]
pub fn start_software_list_import(
    app: AppHandle,
    settings: State<'_, SettingsState>,
    catalog: State<'_, CatalogState>,
) -> Result<ImportStatus, CommandError> {
    let executable = settings
        .configured_executable()?
        .ok_or(CommandError::NotConfigured)?;
    catalog.start(app, ImportKind::SoftwareLists, executable)
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

        state.job.lock().expect("lock").running = Some(RunningImport {
            kind: ImportKind::SoftwareLists,
            cancel: Arc::new(AtomicBool::new(false)),
            progress: Arc::new(ImportProgress::default()),
            started_at: 42,
        });
        assert!(matches!(
            state.import_status(),
            ImportStatus::Running {
                kind: ImportKind::SoftwareLists,
                phase: ImportPhase::Validating,
                items_processed: 0,
                started_at: 42,
                cancel_requested: false
            }
        ));

        state.finish(ImportKind::SoftwareLists, Err(ImportError::Cancelled));
        assert!(matches!(
            state.import_status(),
            ImportStatus::Cancelled {
                kind: ImportKind::SoftwareLists,
                ..
            }
        ));

        state.finish(
            ImportKind::Machines,
            Err(ImportError::Parse {
                message: "bad".to_owned(),
            }),
        );
        assert!(matches!(
            state.import_status(),
            ImportStatus::Failed {
                kind: ImportKind::Machines,
                error: ImportError::Parse { .. },
                ..
            }
        ));
    }

    #[test]
    fn serializes_status_to_frontend_contract() {
        let running = ImportStatus::Running {
            kind: ImportKind::SoftwareLists,
            phase: ImportPhase::Reading,
            items_processed: 1200,
            started_at: 7,
            cancel_requested: false,
        };

        assert_eq!(
            serde_json::to_value(running).expect("serialize"),
            serde_json::json!({
                "state": "running",
                "kind": "softwareLists",
                "phase": "reading",
                "itemsProcessed": 1200,
                "startedAt": 7,
                "cancelRequested": false
            })
        );
        assert_eq!(
            serde_json::to_value(ImportStatus::Failed {
                kind: ImportKind::Machines,
                error: ImportError::MachineCatalogRequired,
                finished_at: 1
            })
            .expect("serialize"),
            serde_json::json!({
                "state": "failed",
                "kind": "machines",
                "error": { "kind": "machineCatalogRequired" },
                "finishedAt": 1
            })
        );
    }
}
