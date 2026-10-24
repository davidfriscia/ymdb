//! MAME installation commands and the managed installation registry.
//!
//! Every command that reads a catalog or a launch configuration receives an
//! explicit installation id and resolves it once with
//! [`InstallationsState::resolve`]; changing the active installation while a
//! request runs never changes the catalog that request reads.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::State;

use super::catalog::CatalogState;
use super::launcher::LauncherState;
use super::{CommandError, data_change, run_blocking};
use crate::catalog::store::{CATALOG_FILE_NAME, CatalogStore};
use crate::installations::migration::{MigrationError, load_or_migrate};
use crate::installations::registry::{
    ExecutableIdentity, ExecutableStatus, InstallationRegistry, MameInstallation, RegistryError,
};
use crate::installations::store::RegistryStore;
use crate::installations::{AppLayout, new_id};
use crate::launcher::store::LaunchConfigStore;
use crate::mame::validation::{ValidationReport, validate_executable};
use crate::mame::version::MameVersion;
use crate::remote::sources::RemoteSourcesStore;

/// Managed state: the registry (kept in memory, saved on every change) and
/// one catalog store per installation used since startup.
pub struct InstallationsState {
    /// `None` when the platform directories could not be resolved.
    layout: Option<AppLayout>,
    /// The error is kept until the registry can be loaded: every access
    /// retries, so a migration blocked by another program completes once that
    /// program is closed.
    registry: Mutex<Result<InstallationRegistry, CommandError>>,
    catalogs: Mutex<HashMap<String, CatalogStore>>,
}

/// An installation resolved for one request.
#[derive(Debug, Clone)]
pub struct ResolvedInstallation {
    pub installation: MameInstallation,
    pub catalog: CatalogStore,
    pub launch_config: LaunchConfigStore,
    pub remote_sources: RemoteSourcesStore,
}

impl InstallationsState {
    /// Loads the registry, migrating the single-installation layout if needed.
    /// MAME is never run here.
    pub fn new(layout: Option<AppLayout>) -> Self {
        let registry = load(layout.as_ref());
        Self {
            layout,
            registry: Mutex::new(registry),
            catalogs: Mutex::new(HashMap::new()),
        }
    }

    fn layout(&self) -> Result<&AppLayout, CommandError> {
        self.layout.as_ref().ok_or_else(unavailable)
    }

    /// Runs `read` on the current registry.
    pub fn with_registry<T>(
        &self,
        read: impl FnOnce(&InstallationRegistry) -> T,
    ) -> Result<T, CommandError> {
        let mut registry = self.registry.lock().unwrap_or_else(PoisonError::into_inner);
        if registry.is_err() {
            *registry = load(self.layout.as_ref());
        }
        registry.as_ref().map(read).map_err(Clone::clone)
    }

    /// Applies `change` to a copy of the registry and saves it; the change
    /// becomes visible only once saved.
    fn update<T>(
        &self,
        change: impl FnOnce(&mut InstallationRegistry) -> Result<T, RegistryError>,
    ) -> Result<T, CommandError> {
        let layout = self.layout()?;
        let mut registry = self.registry.lock().unwrap_or_else(PoisonError::into_inner);
        if registry.is_err() {
            *registry = load(Some(layout));
        }
        let current = registry.as_mut().map_err(|error| error.clone())?;
        let mut changed = current.clone();
        let value = change(&mut changed).map_err(registry_error)?;
        RegistryStore::in_dir(&layout.config_dir)
            .save(&changed)
            .map_err(|error| CommandError::Settings {
                message: error.to_string(),
            })?;
        *current = changed;
        Ok(value)
    }

    /// The installation, its catalog store and its launch configuration.
    pub fn resolve(&self, id: &str) -> Result<ResolvedInstallation, CommandError> {
        let layout = self.layout()?;
        let (installation, active) = self.with_registry(|registry| {
            (
                registry.get(id).cloned(),
                registry.active_installation_id.clone(),
            )
        })?;
        let installation = installation.ok_or_else(|| CommandError::InstallationNotFound {
            installation_id: id.to_owned(),
        })?;
        Ok(ResolvedInstallation {
            catalog: self.catalog_store(layout, id, active.as_deref()),
            launch_config: LaunchConfigStore::in_dir(&layout.launch_config_dir(id)),
            remote_sources: RemoteSourcesStore::in_dir(&layout.launch_config_dir(id)),
            installation,
        })
    }

    /// The store of an installation's catalog. Only the requested and the
    /// active catalogs keep a read connection open; the others are closed so
    /// that browsing many installations does not keep every catalog open.
    fn catalog_store(&self, layout: &AppLayout, id: &str, active: Option<&str>) -> CatalogStore {
        let mut stores = self.catalogs.lock().unwrap_or_else(PoisonError::into_inner);
        for (other, store) in stores.iter() {
            if other != id && Some(other.as_str()) != active {
                store.release_reader();
            }
        }
        stores
            .entry(id.to_owned())
            .or_insert_with(|| CatalogStore::in_dir(&layout.catalog_dir(id)))
            .clone()
    }

    /// Closes the cached read connections of the catalogs (those not in use
    /// by a query), before the application restarts to restore a backup.
    pub fn release_catalog_readers(&self) {
        for store in self
            .catalogs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .values()
        {
            store.release_reader();
        }
    }

    /// The store of a catalog directory for a backup: the shared instance of
    /// a known installation (so that its file lock applies), otherwise a new one.
    pub fn backup_catalog_store(&self, id: &str) -> Result<CatalogStore, CommandError> {
        let layout = self.layout()?;
        let stores = self.catalogs.lock().unwrap_or_else(PoisonError::into_inner);
        Ok(stores
            .get(id)
            .cloned()
            .unwrap_or_else(|| CatalogStore::in_dir(&layout.catalog_dir(id))))
    }

    /// Number of catalogs with an open read connection.
    #[cfg(test)]
    fn open_readers(&self) -> usize {
        self.catalogs
            .lock()
            .expect("lock")
            .values()
            .filter(|store| store.has_open_reader())
            .count()
    }

    fn forget_catalog(&self, id: &str) {
        self.catalogs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(id);
    }

    pub fn view(&self) -> Result<InstallationsView, CommandError> {
        let layout = self.layout()?;
        self.with_registry(|registry| InstallationsView {
            installations: registry
                .installations
                .iter()
                .map(|installation| summarize(layout, registry, installation))
                .collect(),
            active_installation_id: registry.active_installation_id.clone(),
        })
    }

    fn summary(&self, id: &str) -> Result<InstallationSummary, CommandError> {
        let layout = self.layout()?;
        self.with_registry(|registry| {
            registry
                .get(id)
                .map(|installation| summarize(layout, registry, installation))
        })?
        .ok_or_else(|| CommandError::InstallationNotFound {
            installation_id: id.to_owned(),
        })
    }

    /// Registers a validated executable. Fails when it is already registered.
    pub fn register(&self, report: &ValidationReport) -> Result<String, CommandError> {
        let mut installation =
            MameInstallation::new(new_id(), report.executable_path.clone(), now_millis());
        installation.version.clone_from(&report.version);
        installation.identity = ExecutableIdentity::read(&report.executable_path);
        installation.verified_at = Some(report.checked_at);
        let id = installation.id.clone();
        self.update(|registry| registry.add(installation))?;
        Ok(id)
    }

    /// Records a successful verification of an installation.
    pub fn record_verification(
        &self,
        id: &str,
        version: MameVersion,
        identity: Option<ExecutableIdentity>,
        verified_at: u64,
    ) -> Result<(), CommandError> {
        self.update(|registry| registry.record_verification(id, version, identity, verified_at))
    }

    pub fn rename(&self, id: &str, name: Option<&str>) -> Result<(), CommandError> {
        self.update(|registry| registry.rename(id, name))
    }

    pub fn set_active(&self, id: &str) -> Result<(), CommandError> {
        self.update(|registry| registry.set_active(id))?;
        // The previous active catalog is no longer kept open.
        let layout = self.layout()?;
        self.catalog_store(layout, id, Some(id));
        Ok(())
    }

    /// Removes an installation and the files the user chose to delete. The
    /// caller checks that no import or session uses it.
    pub fn remove(
        &self,
        resolved: &ResolvedInstallation,
        delete_catalog: bool,
        delete_launch_config: bool,
    ) -> Result<RemovalResult, CommandError> {
        let id = resolved.installation.id.clone();
        // Files first: if one cannot be deleted, the installation stays
        // registered and nothing else changes.
        if delete_catalog {
            resolved
                .catalog
                .delete_files()
                .map_err(|error| CommandError::CatalogUnreadable {
                    message: error.to_string(),
                })?;
        }
        if delete_launch_config {
            // The installation's settings: its remote sources go with its
            // launch options (the remote connections are global and stay).
            resolved
                .remote_sources
                .delete()
                .map_err(|error| CommandError::Settings {
                    message: format!("could not delete the ROM manager settings: {error}"),
                })?;
            resolved
                .launch_config
                .delete()
                .map_err(|error| CommandError::Settings {
                    message: format!("could not delete the launch options: {error}"),
                })?;
        }
        let active = self.update(|registry| {
            registry.remove(&id)?;
            Ok(registry.active_installation_id.clone())
        })?;
        resolved.catalog.release_reader();
        self.forget_catalog(&id);
        let mut kept_paths = Vec::new();
        if resolved.catalog.catalog_path().exists() {
            kept_paths.push(resolved.catalog.dir().to_path_buf());
        }
        if resolved.launch_config.exists() || resolved.remote_sources.exists() {
            kept_paths.push(resolved.launch_config.dir().to_path_buf());
        }
        Ok(RemovalResult {
            removed_installation_id: id,
            active_installation_id: active,
            kept_paths,
        })
    }
}

fn load(layout: Option<&AppLayout>) -> Result<InstallationRegistry, CommandError> {
    let layout = layout.ok_or_else(unavailable)?;
    load_or_migrate(layout).map_err(|error| match error {
        MigrationError::Registry(error) => CommandError::Settings {
            message: error.to_string(),
        },
        other => CommandError::MigrationFailed {
            message: other.to_string(),
        },
    })
}

fn unavailable() -> CommandError {
    CommandError::Settings {
        message: "the application configuration or data directory is unavailable".to_owned(),
    }
}

fn registry_error(error: RegistryError) -> CommandError {
    match error {
        RegistryError::NotFound(installation_id) => {
            CommandError::InstallationNotFound { installation_id }
        }
        RegistryError::Duplicate(existing_installation_id) => CommandError::DuplicateInstallation {
            existing_installation_id,
        },
        RegistryError::Invalid(message) => CommandError::InvalidRequest { message },
    }
}

/// An installation as shown to the user. Computed without running MAME.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallationSummary {
    pub id: String,
    /// Display name, or the version when none is set.
    pub label: String,
    pub display_name: Option<String>,
    pub executable_path: PathBuf,
    pub version: Option<MameVersion>,
    pub added_at: u64,
    pub verified_at: Option<u64>,
    pub executable_status: ExecutableStatus,
    /// Whether a catalog file exists (its content is read by `get_catalog_status`).
    pub catalog_imported: bool,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallationsView {
    /// In registration order.
    pub installations: Vec<InstallationSummary>,
    pub active_installation_id: Option<String>,
}

fn summarize(
    layout: &AppLayout,
    registry: &InstallationRegistry,
    installation: &MameInstallation,
) -> InstallationSummary {
    InstallationSummary {
        id: installation.id.clone(),
        label: installation.label(),
        display_name: installation.display_name.clone(),
        executable_path: installation.executable_path.clone(),
        version: installation.version.clone(),
        added_at: installation.added_at,
        verified_at: installation.verified_at,
        executable_status: installation.executable_status(),
        catalog_imported: layout
            .catalog_dir(&installation.id)
            .join(CATALOG_FILE_NAME)
            .is_file(),
        active: registry.active_installation_id.as_deref() == Some(installation.id.as_str()),
    }
}

/// Result of a verification: the report and, when valid, the updated installation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallationValidation {
    pub report: ValidationReport,
    /// The registered installation; `None` when an executable to add is invalid.
    pub installation: Option<InstallationSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemovalResult {
    pub removed_installation_id: String,
    /// The active installation after the removal.
    pub active_installation_id: Option<String>,
    /// Directories of the removed installation that were kept by choice.
    pub kept_paths: Vec<PathBuf>,
}

#[tauri::command]
pub fn list_installations(
    installations: State<'_, InstallationsState>,
) -> Result<InstallationsView, CommandError> {
    installations.view()
}

/// Validates an executable (runs `-version`) and registers it as a new
/// installation when valid. Existing installations are not changed.
#[tauri::command]
pub async fn add_installation(
    path: PathBuf,
    installations: State<'_, InstallationsState>,
) -> Result<InstallationValidation, CommandError> {
    let existing = installations.with_registry(|registry| {
        registry
            .find_by_executable(&path)
            .map(|installation| installation.id.clone())
    })?;
    if let Some(existing_installation_id) = existing {
        return Err(CommandError::DuplicateInstallation {
            existing_installation_id,
        });
    }
    let report = run_blocking(move || validate_executable(&path)).await?;
    if !report.is_valid() {
        return Ok(InstallationValidation {
            report,
            installation: None,
        });
    }
    let _change = data_change()?;
    let id = installations.register(&report)?;
    Ok(InstallationValidation {
        installation: Some(installations.summary(&id)?),
        report,
    })
}

/// Runs `-version` on an installation's executable. A valid result updates
/// the recorded version; an invalid one changes nothing.
#[tauri::command]
pub async fn verify_installation(
    installation_id: String,
    installations: State<'_, InstallationsState>,
) -> Result<InstallationValidation, CommandError> {
    let path = installations
        .resolve(&installation_id)?
        .installation
        .executable_path;
    let report = run_blocking(move || validate_executable(&path)).await?;
    if let Some(version) = report.version.clone().filter(|_| report.is_valid()) {
        let _change = data_change()?;
        installations.record_verification(
            &installation_id,
            version,
            ExecutableIdentity::read(&report.executable_path),
            report.checked_at,
        )?;
    }
    Ok(InstallationValidation {
        installation: Some(installations.summary(&installation_id)?),
        report,
    })
}

/// Sets (or clears with `null` or a blank name) the display name.
#[tauri::command]
pub fn rename_installation(
    installation_id: String,
    display_name: Option<String>,
    installations: State<'_, InstallationsState>,
) -> Result<InstallationSummary, CommandError> {
    let _change = data_change()?;
    installations.rename(&installation_id, display_name.as_deref())?;
    installations.summary(&installation_id)
}

/// Selects the installation browsed in the catalog and used by new launches.
/// MAME is not started and running sessions are not affected.
#[tauri::command]
pub fn set_active_installation(
    installation_id: String,
    installations: State<'_, InstallationsState>,
) -> Result<InstallationsView, CommandError> {
    let _change = data_change()?;
    installations.set_active(&installation_id)?;
    installations.view()
}

/// Unregisters an installation. Its catalog and launch options are deleted
/// only when requested; the MAME executable and MAME's own files are never
/// touched. Refused while an import or a MAME session of the installation runs.
#[tauri::command]
pub fn remove_installation(
    installation_id: String,
    delete_catalog: bool,
    delete_launch_config: bool,
    installations: State<'_, InstallationsState>,
    catalog: State<'_, CatalogState>,
    launcher: State<'_, LauncherState>,
) -> Result<RemovalResult, CommandError> {
    let _change = data_change()?;
    let resolved = installations.resolve(&installation_id)?;
    catalog.while_not_importing(&installation_id, || {
        let running = launcher.running_sessions(&installation_id);
        if running > 0 {
            return Err(CommandError::RemovalNotAllowed {
                message: format!(
                    "{running} MAME session(s) started from this installation are still running. \
                     Stop them or wait for them to end first."
                ),
            });
        }
        installations.remove(&resolved, delete_catalog, delete_launch_config)
    })
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests;
