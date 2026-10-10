//! MAME launcher commands.
//!
//! MAME is started only by an explicit Play action. The frontend sends a
//! structured request (installation, machine, software, mounted parts,
//! profile); the command line is always built by the backend from the
//! installation's catalog and launch configuration and validated options.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use serde::Serialize;
use tauri::State;

use super::installations::{InstallationsState, ResolvedInstallation};
use super::{CommandError, run_blocking};
use crate::catalog::freshness::{CatalogFreshness, check_freshness};
use crate::catalog::store::CatalogInfo;
use crate::launcher::config::LaunchConfig;
use crate::launcher::options::{DEFINITIONS, OptionDefinition};
use crate::launcher::resolve::{SoftwareLaunchInfo, software_launch_info};
use crate::launcher::session::{SessionError, SessionInfo, SessionManager};
use crate::launcher::store::LaunchStoreError;
use crate::launcher::{LaunchError, LaunchPreview, LaunchRequest, PreparedLaunch, prepare};

/// Managed state of the launcher: the sessions of every installation and a
/// lock serializing access to the launch configuration files.
pub struct LauncherState {
    lock: Mutex<()>,
    sessions: SessionManager,
}

impl LauncherState {
    pub fn new(log_dir: Option<PathBuf>) -> Self {
        Self {
            lock: Mutex::new(()),
            sessions: SessionManager::new(log_dir),
        }
    }

    /// Reads an installation's `launch.json`; the error means the file is
    /// unreadable or invalid.
    fn load(&self, resolved: &ResolvedInstallation) -> Result<LaunchConfig, LaunchStoreError> {
        let _guard = self.lock.lock().unwrap_or_else(PoisonError::into_inner);
        resolved.launch_config.load()
    }

    /// Running sessions started from an installation.
    pub fn running_sessions(&self, installation_id: &str) -> usize {
        self.sessions.running_for(installation_id)
    }
}

fn store_error(error: &LaunchStoreError) -> CommandError {
    match error {
        LaunchStoreError::Rejected(message) => CommandError::InvalidRequest {
            message: message.clone(),
        },
        other => CommandError::Settings {
            message: other.to_string(),
        },
    }
}

/// The launch configuration of an installation and the options it may contain.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchConfigView {
    pub installation_id: String,
    pub config: LaunchConfig,
    pub definitions: &'static [OptionDefinition],
    /// Set when `launch.json` could not be read: `config` is then the default
    /// configuration, which replaces the file on the next save.
    pub load_error: Option<String>,
}

#[tauri::command]
pub fn get_launch_config(
    installation_id: String,
    installations: State<'_, InstallationsState>,
    launcher: State<'_, LauncherState>,
) -> Result<LaunchConfigView, CommandError> {
    let resolved = installations.resolve(&installation_id)?;
    let (config, load_error) = match launcher.load(&resolved) {
        Ok(config) => (config, None),
        Err(error) => (LaunchConfig::default(), Some(error.to_string())),
    };
    Ok(LaunchConfigView {
        installation_id,
        config,
        definitions: DEFINITIONS,
        load_error,
    })
}

/// Validates and saves the whole launch configuration of an installation.
/// Other installations are not affected.
#[tauri::command]
pub fn save_launch_config(
    installation_id: String,
    config: LaunchConfig,
    installations: State<'_, InstallationsState>,
    launcher: State<'_, LauncherState>,
) -> Result<LaunchConfigView, CommandError> {
    let resolved = installations.resolve(&installation_id)?;
    let _guard = launcher.lock.lock().unwrap_or_else(PoisonError::into_inner);
    let saved = resolved
        .launch_config
        .save(config)
        .map_err(|error| store_error(&error))?;
    Ok(LaunchConfigView {
        installation_id,
        config: saved,
        definitions: DEFINITIONS,
        load_error: None,
    })
}

/// Candidate machines and parts for launching a software; `null` when the
/// software is not in the installation's catalog.
#[tauri::command]
pub async fn get_software_launch_info(
    installation_id: String,
    list: String,
    name: String,
    installations: State<'_, InstallationsState>,
) -> Result<Option<SoftwareLaunchInfo>, CommandError> {
    let store = installations.resolve(&installation_id)?.catalog;
    run_blocking(move || {
        store
            .with_reader(|connection| Ok(software_launch_info(connection, &list, &name)))?
            .ok_or(CommandError::CatalogNotImported)?
            .map_err(|error| internal(&error))
    })
    .await?
}

/// The command line a launch would run, without running it.
#[tauri::command]
pub async fn preview_launch(
    request: LaunchRequest,
    installations: State<'_, InstallationsState>,
    launcher: State<'_, LauncherState>,
) -> Result<LaunchPreview, CommandError> {
    Ok(prepare_request(request, &installations, &launcher)
        .await?
        .0
        .into())
}

/// Starts MAME and returns the new session.
#[tauri::command]
pub async fn start_launch(
    request: LaunchRequest,
    installations: State<'_, InstallationsState>,
    launcher: State<'_, LauncherState>,
) -> Result<SessionInfo, CommandError> {
    let (prepared, label) = prepare_request(request, &installations, &launcher).await?;
    launcher
        .sessions
        .start(prepared.target, prepared.title, label, &prepared.plan)
        .map_err(session_error)
}

/// Sessions started during this run, from every installation, most recent
/// first. Cheap: suitable for polling.
#[tauri::command]
pub fn get_launch_sessions(launcher: State<'_, LauncherState>) -> Vec<SessionInfo> {
    launcher.sessions.list()
}

/// Kills a running session.
#[tauri::command]
pub fn stop_launch_session(
    id: u64,
    launcher: State<'_, LauncherState>,
) -> Result<SessionInfo, CommandError> {
    launcher.sessions.stop(id).map_err(session_error)
}

/// Prepares a launch with the installation named by the request (not the
/// active one, which may change meanwhile). Also returns the installation's label.
async fn prepare_request(
    request: LaunchRequest,
    installations: &InstallationsState,
    launcher: &LauncherState,
) -> Result<(PreparedLaunch, String), CommandError> {
    let resolved = installations.resolve(&request.installation_id)?;
    let config = launcher
        .load(&resolved)
        .map_err(|error| CommandError::Settings {
            message: format!("{error}. Fix or save the launch options in Settings."),
        })?;
    let label = resolved.installation.label();
    let prepared = run_blocking(move || prepare_resolved(&resolved, &config, &request)).await??;
    Ok((prepared, label))
}

pub(super) fn prepare_resolved(
    resolved: &ResolvedInstallation,
    config: &LaunchConfig,
    request: &LaunchRequest,
) -> Result<PreparedLaunch, CommandError> {
    let executable = &resolved.installation.executable_path;
    let info = resolved
        .catalog
        .read_info()?
        .ok_or(CommandError::CatalogNotImported)?;
    ensure_catalog_matches(&info, executable, request.software.is_some())?;
    resolved
        .catalog
        .with_reader(|connection| Ok(prepare(connection, executable, config, request)))?
        .ok_or(CommandError::CatalogNotImported)?
        .map_err(|error| match error {
            LaunchError::Invalid(message) => CommandError::InvalidRequest { message },
            LaunchError::NotFound(message) => CommandError::NotFound { message },
            LaunchError::Database(error) => internal(&error),
        })
}

/// A launch needs a catalog from the installation's unchanged executable,
/// with software lists when a software is launched.
fn ensure_catalog_matches(
    info: &CatalogInfo,
    executable: &Path,
    needs_software: bool,
) -> Result<(), CommandError> {
    let freshness = check_freshness(info, executable);
    if freshness != CatalogFreshness::Current {
        return Err(CommandError::CatalogMismatch { freshness });
    }
    if needs_software && info.software.is_none() {
        return Err(CommandError::SoftwareListsNotImported);
    }
    Ok(())
}

fn session_error(error: SessionError) -> CommandError {
    match error {
        SessionError::AlreadyRunning(session_id) => CommandError::AlreadyRunning { session_id },
        SessionError::Spawn(error) => CommandError::LaunchFailed {
            message: error.to_string(),
        },
        SessionError::NotFound => CommandError::NotFound {
            message: "the session does not exist".to_owned(),
        },
        SessionError::NotRunning => CommandError::InvalidRequest {
            message: "the session has already ended".to_owned(),
        },
    }
}

fn internal(error: &dyn std::fmt::Display) -> CommandError {
    CommandError::Internal {
        message: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::UNIX_EPOCH;

    use super::*;
    use crate::catalog::store::{CatalogStatistics, SoftwareCatalogInfo, SoftwareStatistics};

    fn info_for(path: &Path, software: bool) -> CatalogInfo {
        let metadata = fs::metadata(path).expect("metadata");
        CatalogInfo {
            schema_version: 3,
            mame_release: "0.280".to_owned(),
            mame_revision: "mame0280".to_owned(),
            mame_build: None,
            executable_path: path.to_path_buf(),
            executable_size: Some(metadata.len()),
            executable_modified_at: metadata
                .modified()
                .ok()
                .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                .and_then(|elapsed| u64::try_from(elapsed.as_millis()).ok()),
            imported_at: 1,
            import_duration_ms: 1,
            database_size: 1,
            statistics: CatalogStatistics::default(),
            software: software.then(|| SoftwareCatalogInfo {
                mame_release: "0.280".to_owned(),
                mame_revision: "mame0280".to_owned(),
                imported_at: 1,
                import_duration_ms: 1,
                matches_machine_catalog: true,
                statistics: SoftwareStatistics::default(),
            }),
        }
    }

    #[test]
    fn launches_only_with_a_catalog_of_the_configured_executable() {
        let dir = tempfile::tempdir().expect("temp dir");
        let mame = dir.path().join("Émulateurs MAME ✓ mame");
        fs::write(&mame, "binary").expect("write");
        let other = dir.path().join("other-mame");
        fs::write(&other, "binary").expect("write");

        assert_eq!(
            ensure_catalog_matches(&info_for(&mame, true), &mame, true),
            Ok(())
        );
        assert_eq!(
            ensure_catalog_matches(&info_for(&other, true), &mame, false),
            Err(CommandError::CatalogMismatch {
                freshness: CatalogFreshness::DifferentExecutable
            })
        );
        // The executable was updated in place after the import.
        let info = info_for(&mame, true);
        fs::write(&mame, "updated binary").expect("write");
        assert_eq!(
            ensure_catalog_matches(&info, &mame, false),
            Err(CommandError::CatalogMismatch {
                freshness: CatalogFreshness::ExecutableChanged
            })
        );
        // Software launches need the software lists.
        let info = info_for(&mame, false);
        assert_eq!(ensure_catalog_matches(&info, &mame, false), Ok(()));
        assert_eq!(
            ensure_catalog_matches(&info, &mame, true),
            Err(CommandError::SoftwareListsNotImported)
        );
    }
}
