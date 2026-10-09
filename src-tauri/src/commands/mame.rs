//! MAME installation management commands.
//!
//! MAME is executed only by explicit user actions (select, verify, detect),
//! always with the single fixed argument `-version`.

use std::path::PathBuf;

use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use super::{CommandError, SettingsState, run_blocking};
use crate::mame::detection::{DetectedInstallation, SearchLocations, detect_installations};
use crate::mame::platform;
use crate::mame::validation::{ValidationReport, validate_executable};
use crate::settings::MameSettings;

/// Saved MAME configuration, as known without running MAME.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MameConfiguration {
    pub executable_path: Option<PathBuf>,
    /// Whether a file still exists at the configured path.
    pub executable_exists: bool,
}

#[tauri::command]
pub fn get_mame_configuration(
    state: State<'_, SettingsState>,
) -> Result<MameConfiguration, CommandError> {
    let executable_path = configured_executable(&state)?;
    let executable_exists = executable_path.as_ref().is_some_and(|path| path.is_file());
    Ok(MameConfiguration {
        executable_path,
        executable_exists,
    })
}

/// Opens the native file picker. Returns `None` when the user cancels.
#[tauri::command]
pub async fn pick_mame_executable(app: AppHandle) -> Result<Option<PathBuf>, CommandError> {
    let selection = run_blocking(move || {
        let mut dialog = app.dialog().file().set_title("Select MAME executable");
        if let Some((name, extensions)) = platform::EXECUTABLE_FILE_FILTER {
            dialog = dialog.add_filter(name, extensions);
        }
        dialog.blocking_pick_file()
    })
    .await?;

    selection
        .map(|file| file.into_path())
        .transpose()
        .map_err(|error| CommandError::Internal {
            message: format!("unsupported file selection: {error}"),
        })
}

/// Validates an executable and saves it as the configured MAME executable
/// only if validation succeeds.
#[tauri::command]
pub async fn set_mame_executable(
    path: PathBuf,
    state: State<'_, SettingsState>,
) -> Result<ValidationReport, CommandError> {
    let report = run_blocking(move || validate_executable(&path)).await?;
    if report.is_valid() {
        let executable_path = report.executable_path.clone();
        state.with_store(|store| {
            store.update(|settings| settings.mame = Some(MameSettings { executable_path }))
        })?;
    }
    Ok(report)
}

/// Validates the configured executable again. The saved configuration is
/// not modified, whatever the result.
#[tauri::command]
pub async fn verify_mame_configuration(
    state: State<'_, SettingsState>,
) -> Result<ValidationReport, CommandError> {
    let path = configured_executable(&state)?.ok_or(CommandError::NotConfigured)?;
    run_blocking(move || validate_executable(&path)).await
}

#[tauri::command]
pub fn clear_mame_configuration(state: State<'_, SettingsState>) -> Result<(), CommandError> {
    state.with_store(|store| store.update(|settings| settings.mame = None).map(drop))
}

/// Searches well-known locations and validates the candidates found. The
/// saved configuration is never modified by detection.
#[tauri::command]
pub async fn detect_mame_installations(
    state: State<'_, SettingsState>,
) -> Result<Vec<DetectedInstallation>, CommandError> {
    // An unreadable settings file must not prevent detection.
    let configured = configured_executable(&state).ok().flatten();
    run_blocking(move || detect_installations(&SearchLocations::for_current_platform(configured)))
        .await
}

fn configured_executable(state: &SettingsState) -> Result<Option<PathBuf>, CommandError> {
    state.with_store(|store| {
        store
            .load()
            .map(|settings| settings.mame.map(|mame| mame.executable_path))
    })
}
