//! Catalog record detail commands.
//!
//! Each command returns `null` when the record does not exist, for example
//! because a re-import replaced the catalog while the user was browsing.

use tauri::State;

use super::catalog::CatalogState;
use super::{CommandError, run_blocking};
use crate::catalog::detail::machine::{MachineDetail, machine_detail};
use crate::catalog::detail::software::{
    SoftwareDetail, SoftwareListDetail, software_detail, software_list_detail,
};
use crate::catalog::store::CatalogStore;

#[tauri::command]
pub async fn get_machine_detail(
    name: String,
    catalog: State<'_, CatalogState>,
) -> Result<Option<MachineDetail>, CommandError> {
    let store = catalog.store()?.clone();
    run_blocking(move || read(&store, |connection| machine_detail(connection, &name))).await?
}

#[tauri::command]
pub async fn get_software_detail(
    list: String,
    name: String,
    catalog: State<'_, CatalogState>,
) -> Result<Option<SoftwareDetail>, CommandError> {
    let store = catalog.store()?.clone();
    run_blocking(move || {
        read(&store, |connection| {
            software_detail(connection, &list, &name)
        })
    })
    .await?
}

#[tauri::command]
pub async fn get_software_list_detail(
    name: String,
    catalog: State<'_, CatalogState>,
) -> Result<Option<SoftwareListDetail>, CommandError> {
    let store = catalog.store()?.clone();
    run_blocking(move || read(&store, |connection| software_list_detail(connection, &name))).await?
}

fn read<T>(
    store: &CatalogStore,
    query: impl FnOnce(&rusqlite::Connection) -> rusqlite::Result<T>,
) -> Result<T, CommandError> {
    let internal = |message: String| CommandError::Internal { message };
    store
        .with_reader(|connection| Ok(query(connection)))
        .map_err(|error| internal(error.to_string()))?
        .ok_or(CommandError::CatalogNotImported)?
        .map_err(|error| internal(error.to_string()))
}
