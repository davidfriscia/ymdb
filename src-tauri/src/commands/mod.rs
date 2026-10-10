//! Tauri commands exposed to the frontend.
//!
//! Commands are thin adapters: they extract what they need from the Tauri
//! runtime and delegate to plain, testable functions. Blocking work (process
//! execution, native dialogs) runs on the blocking thread pool so the UI and
//! the main thread stay responsive.

pub mod catalog;
pub mod detail;
pub mod health;
pub mod installations;
pub mod launcher;
pub mod mame;
pub mod media;
pub mod search;

use serde::Serialize;

use crate::catalog::freshness::CatalogFreshness;
use crate::catalog::store::StoreError;

/// Error returned by commands. Validation failures are not errors: they are
/// reported as part of a successful validation result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum CommandError {
    /// A configuration file could not be read or written.
    Settings { message: String },
    /// The installation does not exist (for example it was removed).
    InstallationNotFound { installation_id: String },
    /// The executable is already registered.
    DuplicateInstallation { existing_installation_id: String },
    /// The installation cannot be removed now (for example MAME is running).
    RemovalNotAllowed { message: String },
    /// The configuration of an older YMDB could not be upgraded; nothing was lost.
    MigrationFailed { message: String },
    /// A catalog import is already running.
    ImportInProgress,
    /// The request is invalid (for example a page size out of range).
    InvalidRequest { message: String },
    /// No catalog has been imported yet for the installation.
    CatalogNotImported,
    /// The catalog file exists but cannot be read.
    CatalogUnreadable { message: String },
    /// The catalog contains no software lists.
    SoftwareListsNotImported,
    /// The catalog does not match the installation's executable, so names may
    /// differ: it must be imported again before launching.
    CatalogMismatch { freshness: CatalogFreshness },
    /// A machine, software or session does not exist.
    NotFound { message: String },
    /// A session for the same machine and software is already running.
    AlreadyRunning { session_id: u64 },
    /// MAME could not be started.
    LaunchFailed { message: String },
    /// Unexpected failure (for example a native dialog error).
    Internal { message: String },
}

impl From<StoreError> for CommandError {
    fn from(error: StoreError) -> Self {
        Self::CatalogUnreadable {
            message: error.to_string(),
        }
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
            serde_json::to_value(CommandError::InstallationNotFound {
                installation_id: "abc".to_owned()
            })
            .expect("serialize"),
            serde_json::json!({ "kind": "installationNotFound", "installationId": "abc" })
        );
        assert_eq!(
            serde_json::to_value(CommandError::DuplicateInstallation {
                existing_installation_id: "abc".to_owned()
            })
            .expect("serialize"),
            serde_json::json!({ "kind": "duplicateInstallation", "existingInstallationId": "abc" })
        );
        assert_eq!(
            serde_json::to_value(CommandError::AlreadyRunning { session_id: 3 })
                .expect("serialize"),
            serde_json::json!({ "kind": "alreadyRunning", "sessionId": 3 })
        );
        assert_eq!(
            serde_json::to_value(CommandError::CatalogMismatch {
                freshness: CatalogFreshness::DifferentExecutable
            })
            .expect("serialize"),
            serde_json::json!({ "kind": "catalogMismatch", "freshness": "differentExecutable" })
        );
    }
}
