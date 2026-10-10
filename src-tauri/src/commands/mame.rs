//! MAME executable selection and detection commands.
//!
//! MAME is executed only by explicit user actions (detect), always with the
//! single fixed argument `-version`. Registering installations is done by
//! `commands::installations`.

use std::path::PathBuf;

use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use super::installations::InstallationsState;
use super::{CommandError, run_blocking};
use crate::mame::detection::{DetectedInstallation, SearchLocations, detect_installations};
use crate::mame::platform;

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

/// A detected executable and the installation that already registers it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedCandidate {
    #[serde(flatten)]
    pub detected: DetectedInstallation,
    pub registered_installation_id: Option<String>,
}

/// Searches well-known locations and validates the candidates found. The
/// registered installations are never modified by detection.
#[tauri::command]
pub async fn detect_mame_installations(
    installations: State<'_, InstallationsState>,
) -> Result<Vec<DetectedCandidate>, CommandError> {
    // The active installation is listed first, as the "configured" one. An
    // unreadable registry must not prevent detection.
    let active = installations
        .with_registry(|registry| {
            registry
                .active()
                .map(|installation| installation.executable_path.clone())
        })
        .ok()
        .flatten();
    let detected =
        run_blocking(move || detect_installations(&SearchLocations::for_current_platform(active)))
            .await?;
    Ok(detected
        .into_iter()
        .map(|detected| {
            let registered_installation_id = installations
                .with_registry(|registry| {
                    registry
                        .find_by_executable(&detected.report.executable_path)
                        .map(|installation| installation.id.clone())
                })
                .ok()
                .flatten();
            DetectedCandidate {
                detected,
                registered_installation_id,
            }
        })
        .collect())
}
