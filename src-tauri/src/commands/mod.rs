//! Tauri commands exposed to the frontend.
//!
//! Commands are thin adapters: they extract what they need from the Tauri
//! runtime and delegate to plain, testable functions. Blocking work (process
//! execution, native dialogs) runs on the blocking thread pool so the UI and
//! the main thread stay responsive.

pub mod catalog;
pub mod health;
pub mod mame;
pub mod search;

use std::path::PathBuf;
use std::sync::{Mutex, PoisonError};

use serde::Serialize;

use crate::settings::{SettingsError, SettingsStore};

/// Error returned by commands. Validation failures are not errors: they are
/// reported as part of a successful validation result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum CommandError {
    /// The settings could not be read or written.
    Settings { message: String },
    /// The operation requires a configured MAME executable.
    NotConfigured,
    /// A catalog import is already running.
    ImportInProgress,
    /// The search request is invalid (for example a page size out of range).
    InvalidRequest { message: String },
    /// No catalog has been imported yet.
    CatalogNotImported,
    /// The catalog contains no software lists.
    SoftwareListsNotImported,
    /// Unexpected failure (for example a native dialog error).
    Internal { message: String },
}

impl From<SettingsError> for CommandError {
    fn from(error: SettingsError) -> Self {
        Self::Settings {
            message: error.to_string(),
        }
    }
}

/// Managed state giving commands serialized access to the settings file.
pub struct SettingsState {
    /// `None` when the platform configuration directory could not be resolved.
    store: Option<SettingsStore>,
    lock: Mutex<()>,
}

impl SettingsState {
    pub fn new(store: Option<SettingsStore>) -> Self {
        Self {
            store,
            lock: Mutex::new(()),
        }
    }

    /// Returns the configured MAME executable, if any.
    pub fn configured_executable(&self) -> Result<Option<PathBuf>, CommandError> {
        self.with_store(|store| {
            store
                .load()
                .map(|settings| settings.mame.map(|mame| mame.executable_path))
        })
    }

    fn with_store<T>(
        &self,
        operation: impl FnOnce(&SettingsStore) -> Result<T, SettingsError>,
    ) -> Result<T, CommandError> {
        let store = self.store.as_ref().ok_or_else(|| CommandError::Settings {
            message: "the application configuration directory is unavailable".to_owned(),
        })?;
        let _guard = self.lock.lock().unwrap_or_else(PoisonError::into_inner);
        operation(store).map_err(CommandError::from)
    }
}

/// Runs blocking work on the blocking thread pool.
async fn run_blocking<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, CommandError> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|error| CommandError::Internal {
            message: error.to_string(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_errors_to_frontend_contract() {
        let settings = CommandError::Settings {
            message: "disk full".to_owned(),
        };

        assert_eq!(
            serde_json::to_value(settings).expect("serialize"),
            serde_json::json!({ "kind": "settings", "message": "disk full" })
        );
        assert_eq!(
            serde_json::to_value(CommandError::NotConfigured).expect("serialize"),
            serde_json::json!({ "kind": "notConfigured" })
        );
    }

    #[test]
    fn reports_missing_configuration_directory_as_settings_error() {
        let state = SettingsState::new(None);

        let result = state.with_store(SettingsStore::load);

        assert!(matches!(result, Err(CommandError::Settings { .. })));
    }
}
