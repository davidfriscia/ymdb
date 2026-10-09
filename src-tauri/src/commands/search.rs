//! Catalog search commands.
//!
//! Searches run on the blocking thread pool with the catalog store's cached
//! read connection, so they never block the main thread.

use tauri::State;

use super::catalog::CatalogState;
use super::{CommandError, run_blocking};
use crate::catalog::search::SearchError;
use crate::catalog::search::machines::{MachineSearchResponse, search_machines};
use crate::catalog::search::request::{MachineSearchRequest, SoftwareSearchRequest};
use crate::catalog::search::software::{SoftwareSearchResponse, search_software};
use crate::catalog::store::CatalogStore;

#[tauri::command]
pub async fn search_catalog_machines(
    request: MachineSearchRequest,
    catalog: State<'_, CatalogState>,
) -> Result<MachineSearchResponse, CommandError> {
    let store = catalog.store()?.clone();
    run_blocking(move || search(&store, |connection| search_machines(connection, &request))).await?
}

#[tauri::command]
pub async fn search_catalog_software(
    request: SoftwareSearchRequest,
    catalog: State<'_, CatalogState>,
) -> Result<SoftwareSearchResponse, CommandError> {
    let store = catalog.store()?.clone();
    run_blocking(move || search(&store, |connection| search_software(connection, &request))).await?
}

fn search<T>(
    store: &CatalogStore,
    query: impl FnOnce(&rusqlite::Connection) -> Result<T, SearchError>,
) -> Result<T, CommandError> {
    store
        .with_reader(|connection| Ok(query(connection)))
        .map_err(|error| CommandError::Internal {
            message: error.to_string(),
        })?
        .ok_or(CommandError::CatalogNotImported)?
        .map_err(|error| match error {
            SearchError::InvalidRequest(message) => CommandError::InvalidRequest { message },
            SearchError::SoftwareListsNotImported => CommandError::SoftwareListsNotImported,
            SearchError::Sqlite(error) => CommandError::Internal {
                message: error.to_string(),
            },
        })
}
