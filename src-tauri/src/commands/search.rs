//! Catalog search commands.
//!
//! Every search names the installation whose catalog it reads. Searches run
//! on the blocking thread pool with that catalog's cached read connection, so
//! they never block the main thread. A `list` filter is resolved here from the
//! global lists: the catalog never stores them.

use tauri::State;

use super::installations::InstallationsState;
use super::lists::ListsState;
use super::support_files::{SupportFilesState, sync_catalog};
use super::{CommandError, run_blocking};
use crate::catalog::search::SearchError;
use crate::catalog::search::machines::{MachineSearchResponse, search_machines};
use crate::catalog::search::request::{MachineSearchRequest, SoftwareSearchRequest};
use crate::catalog::search::software::{SoftwareSearchResponse, search_software};
use crate::catalog::store::CatalogStore;

#[tauri::command]
pub async fn search_catalog_machines(
    installation_id: String,
    request: MachineSearchRequest,
    installations: State<'_, InstallationsState>,
    lists: State<'_, ListsState>,
    support: State<'_, SupportFilesState>,
) -> Result<MachineSearchResponse, CommandError> {
    let store = installations.resolve(&installation_id)?.catalog;
    let members = lists.machine_members(request.filters.list.as_deref())?;
    let target = support.target();
    run_blocking(move || {
        // A failure keeps the previous projection; the status reports it.
        let _ = sync_catalog(&store, target);
        search(&store, |connection| {
            search_machines(connection, &request, members.as_ref())
        })
    })
    .await?
}

#[tauri::command]
pub async fn search_catalog_software(
    installation_id: String,
    request: SoftwareSearchRequest,
    installations: State<'_, InstallationsState>,
    lists: State<'_, ListsState>,
) -> Result<SoftwareSearchResponse, CommandError> {
    let store = installations.resolve(&installation_id)?.catalog;
    let members = lists.software_members(request.filters.list.as_deref())?;
    run_blocking(move || {
        search(&store, |connection| {
            search_software(connection, &request, members.as_ref())
        })
    })
    .await?
}

pub(super) fn search<T>(
    store: &CatalogStore,
    query: impl FnOnce(&rusqlite::Connection) -> Result<T, SearchError>,
) -> Result<T, CommandError> {
    store
        .with_reader(|connection| Ok(query(connection)))?
        .ok_or(CommandError::CatalogNotImported)?
        .map_err(|error| match error {
            SearchError::InvalidRequest(message) => CommandError::InvalidRequest { message },
            SearchError::SoftwareListsNotImported => CommandError::SoftwareListsNotImported,
            SearchError::Sqlite(error) => CommandError::Internal {
                message: error.to_string(),
            },
        })
}
