//! MAME catalog commands.
//!
//! Every installation has its own catalog. An import (of machines or of
//! software lists) runs on a dedicated thread for the installation it was
//! started for, whatever the active installation becomes meanwhile. Only one
//! import runs at a time, which keeps MAME's memory and disk use bounded and
//! guarantees that a catalog file is never replaced by two operations
//! concurrently. The frontend polls
//! [`get_catalog_import_status`] (cheap, in-memory) while an import runs and
//! [`get_catalog_status`] (reads the catalog database) when it needs the
//! catalog summary.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::State;

use super::installations::{InstallationsState, ResolvedInstallation};
use super::{CommandError, run_blocking};
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
        installation_id: String,
        kind: ImportKind,
        phase: ImportPhase,
        /// Machines or software stored so far, depending on `kind`.
        items_processed: u64,
        started_at: u64,
        cancel_requested: bool,
    },
    Completed {
        installation_id: String,
        kind: ImportKind,
        /// Machines or software imported, depending on `kind`.
        items: u64,
        duration_ms: u64,
        finished_at: u64,
    },
    Failed {
        installation_id: String,
        kind: ImportKind,
        error: ImportError,
        finished_at: u64,
    },
    Cancelled {
        installation_id: String,
        kind: ImportKind,
        finished_at: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogStatus {
    pub installation_id: String,
    pub catalog: Option<CatalogInfo>,
    /// Set when a catalog file exists but cannot be used.
    pub catalog_error: Option<String>,
    /// `None` when there is no usable catalog.
    pub freshness: Option<CatalogFreshness>,
    /// The current or last import, of any installation.
    pub import: ImportStatus,
}

/// Managed state of the catalog imports.
#[derive(Default)]
pub struct CatalogState {
    /// Shared with the import thread, which records the outcome.
    job: Arc<Mutex<ImportJob>>,
}

#[derive(Default)]
struct ImportJob {
    running: Option<RunningImport>,
    last: Option<ImportStatus>,
}

struct RunningImport {
    installation_id: String,
    kind: ImportKind,
    cancel: Arc<AtomicBool>,
    progress: Arc<ImportProgress>,
    started_at: u64,
}

impl CatalogState {
    /// Runs `work` while no import of the installation can start or run.
    pub fn while_not_importing<T>(
        &self,
        installation_id: &str,
        work: impl FnOnce() -> Result<T, CommandError>,
    ) -> Result<T, CommandError> {
        let job = self.job.lock().unwrap_or_else(PoisonError::into_inner);
        if job
            .running
            .as_ref()
            .is_some_and(|running| running.installation_id == installation_id)
        {
            return Err(CommandError::RemovalNotAllowed {
                message: "a catalog import of this installation is running. Cancel it or wait \
                          for it to end first."
                    .to_owned(),
            });
        }
        let result = work();
        drop(job);
        result
    }

    pub(super) fn cancel(&self) {
        if let Some(running) = &self
            .job
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .running
        {
            running.cancel.store(true, Ordering::Relaxed);
        }
    }

    pub(super) fn import_status(&self) -> ImportStatus {
        let job = self.job.lock().unwrap_or_else(PoisonError::into_inner);
        match &job.running {
            Some(running) => ImportStatus::Running {
                installation_id: running.installation_id.clone(),
                kind: running.kind,
                phase: running.progress.phase(),
                items_processed: running.progress.items_processed(),
                started_at: running.started_at,
                cancel_requested: running.cancel.load(Ordering::Relaxed),
            },
            None => job.last.clone().unwrap_or(ImportStatus::Idle),
        }
    }
}

fn finish(
    job: &Mutex<ImportJob>,
    installation_id: String,
    kind: ImportKind,
    result: Result<CatalogInfo, ImportError>,
) {
    {
        let finished_at = now_millis();
        let status = match result {
            Ok(info) => ImportStatus::Completed {
                installation_id,
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
            Err(ImportError::Cancelled) => ImportStatus::Cancelled {
                installation_id,
                kind,
                finished_at,
            },
            Err(error) => ImportStatus::Failed {
                installation_id,
                kind,
                error,
                finished_at,
            },
        };
        let mut job = job.lock().unwrap_or_else(PoisonError::into_inner);
        job.running = None;
        job.last = Some(status);
    }
}

impl CatalogState {
    /// Starts an import of `installation` on a dedicated thread, unless one
    /// is already running. The thread keeps the installation's store and
    /// executable, so it is not affected by later changes of the active
    /// installation.
    fn start(
        &self,
        kind: ImportKind,
        resolved: ResolvedInstallation,
        installations: &InstallationsState,
    ) -> Result<ImportStatus, CommandError> {
        let executable = resolved.installation.executable_path.clone();
        self.start_with(
            kind,
            resolved,
            installations,
            move |store, cancel, progress| match kind {
                ImportKind::Machines => import_catalog(&executable, store, cancel, progress),
                ImportKind::SoftwareLists => {
                    import_software_lists(&executable, store, cancel, progress)
                }
            },
        )
    }

    pub(super) fn start_with(
        &self,
        kind: ImportKind,
        resolved: ResolvedInstallation,
        installations: &InstallationsState,
        import: impl FnOnce(
            &CatalogStore,
            &Arc<AtomicBool>,
            &ImportProgress,
        ) -> Result<CatalogInfo, ImportError>
        + Send
        + 'static,
    ) -> Result<ImportStatus, CommandError> {
        let installation_id = resolved.installation.id.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let progress = Arc::new(ImportProgress::default());
        {
            let mut job = self.job.lock().unwrap_or_else(PoisonError::into_inner);
            if job.running.is_some() {
                return Err(CommandError::ImportInProgress);
            }
            // Removal checks the running import under the same lock: an
            // installation removed meanwhile is not imported.
            if !installations.with_registry(|registry| registry.get(&installation_id).is_some())? {
                return Err(CommandError::InstallationNotFound { installation_id });
            }
            job.running = Some(RunningImport {
                installation_id: installation_id.clone(),
                kind,
                cancel: Arc::clone(&cancel),
                progress: Arc::clone(&progress),
                started_at: now_millis(),
            });
        }

        let job = Arc::clone(&self.job);
        let thread_id = installation_id.clone();
        let spawned = thread::Builder::new()
            .name("catalog-import".to_owned())
            .spawn(move || {
                let result = import(&resolved.catalog, &cancel, &progress);
                finish(&job, thread_id, kind, result);
            });
        if let Err(error) = spawned {
            finish(
                &self.job,
                installation_id,
                kind,
                Err(ImportError::LaunchFailed {
                    message: format!("failed to start the import thread: {error}"),
                }),
            );
        }
        Ok(self.import_status())
    }
}

/// The catalog of an installation and its freshness against the
/// installation's executable. Reads the catalog database.
#[tauri::command]
pub async fn get_catalog_status(
    installation_id: String,
    installations: State<'_, InstallationsState>,
    catalog: State<'_, CatalogState>,
) -> Result<CatalogStatus, CommandError> {
    let resolved = installations.resolve(&installation_id)?;
    let (info, catalog_error) = run_blocking(move || match resolved.catalog.read_info() {
        Ok(Some(info)) => {
            let freshness = check_freshness(&info, &resolved.installation.executable_path);
            (Some((info, freshness)), None)
        }
        Ok(None) => (None, None),
        Err(error) => (None, Some(error.to_string())),
    })
    .await?;
    let (info, freshness) = info.unzip();
    Ok(CatalogStatus {
        installation_id,
        catalog: info,
        catalog_error,
        freshness,
        import: catalog.import_status(),
    })
}

#[tauri::command]
pub fn get_catalog_import_status(catalog: State<'_, CatalogState>) -> ImportStatus {
    catalog.import_status()
}

/// Starts importing the machine catalog of an installation from its
/// executable. The new catalog replaces the installation's current one,
/// including its software lists; other installations are not affected.
#[tauri::command]
pub fn start_catalog_import(
    installation_id: String,
    installations: State<'_, InstallationsState>,
    catalog: State<'_, CatalogState>,
) -> Result<ImportStatus, CommandError> {
    let resolved = installations.resolve(&installation_id)?;
    catalog.start(ImportKind::Machines, resolved, &installations)
}

/// Starts importing the software lists of an installation from its
/// executable, which must be the one its machine catalog was imported from.
#[tauri::command]
pub fn start_software_list_import(
    installation_id: String,
    installations: State<'_, InstallationsState>,
    catalog: State<'_, CatalogState>,
) -> Result<ImportStatus, CommandError> {
    let resolved = installations.resolve(&installation_id)?;
    catalog.start(ImportKind::SoftwareLists, resolved, &installations)
}

/// Requests cancellation of the running import, if any.
#[tauri::command]
pub fn cancel_catalog_import(catalog: State<'_, CatalogState>) -> ImportStatus {
    catalog.cancel();
    catalog.import_status()
}

pub(super) fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "0123456789abcdef0123456789abcdef";

    #[test]
    fn reports_idle_then_running_then_last_outcome() {
        let state = CatalogState::default();
        assert_eq!(state.import_status(), ImportStatus::Idle);

        state.job.lock().expect("lock").running = Some(RunningImport {
            installation_id: ID.to_owned(),
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
                cancel_requested: false,
                ..
            }
        ));
        // Nothing else can be done with the installation while it imports.
        assert!(matches!(
            state.while_not_importing(ID, || Ok(())),
            Err(CommandError::RemovalNotAllowed { .. })
        ));
        assert_eq!(
            state.while_not_importing("fedcba9876543210fedcba9876543210", || Ok(1)),
            Ok(1)
        );

        finish(
            &state.job,
            ID.to_owned(),
            ImportKind::SoftwareLists,
            Err(ImportError::Cancelled),
        );
        assert!(matches!(
            state.import_status(),
            ImportStatus::Cancelled {
                kind: ImportKind::SoftwareLists,
                ref installation_id,
                ..
            } if installation_id == ID
        ));
        assert_eq!(state.while_not_importing(ID, || Ok(2)), Ok(2));

        finish(
            &state.job,
            ID.to_owned(),
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
            installation_id: ID.to_owned(),
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
                "installationId": ID,
                "kind": "softwareLists",
                "phase": "reading",
                "itemsProcessed": 1200,
                "startedAt": 7,
                "cancelRequested": false
            })
        );
        assert_eq!(
            serde_json::to_value(ImportStatus::Failed {
                installation_id: ID.to_owned(),
                kind: ImportKind::Machines,
                error: ImportError::MachineCatalogRequired,
                finished_at: 1
            })
            .expect("serialize"),
            serde_json::json!({
                "state": "failed",
                "installationId": ID,
                "kind": "machines",
                "error": { "kind": "machineCatalogRequired" },
                "finishedAt": 1
            })
        );
    }
}
