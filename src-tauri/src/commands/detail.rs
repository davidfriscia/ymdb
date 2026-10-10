//! Catalog record detail commands.
//!
//! Each command reads the catalog of the installation it names and returns
//! `null` when the record does not exist, for example because a re-import
//! replaced the catalog while the user was browsing.

use tauri::State;

use super::installations::InstallationsState;
use super::{CommandError, run_blocking};
use crate::catalog::detail::machine::{MachineDetail, machine_detail};
use crate::catalog::detail::software::{
    SoftwareDetail, SoftwareListDetail, software_detail, software_list_detail,
};
use crate::catalog::store::CatalogStore;

#[tauri::command]
pub async fn get_machine_detail(
    installation_id: String,
    name: String,
    installations: State<'_, InstallationsState>,
) -> Result<Option<MachineDetail>, CommandError> {
    let store = installations.resolve(&installation_id)?.catalog;
    run_blocking(move || read(&store, |connection| machine_detail(connection, &name))).await?
}

#[tauri::command]
pub async fn get_software_detail(
    installation_id: String,
    list: String,
    name: String,
    installations: State<'_, InstallationsState>,
) -> Result<Option<SoftwareDetail>, CommandError> {
    let store = installations.resolve(&installation_id)?.catalog;
    run_blocking(move || {
        read(&store, |connection| {
            software_detail(connection, &list, &name)
        })
    })
    .await?
}

#[tauri::command]
pub async fn get_software_list_detail(
    installation_id: String,
    name: String,
    installations: State<'_, InstallationsState>,
) -> Result<Option<SoftwareListDetail>, CommandError> {
    let store = installations.resolve(&installation_id)?.catalog;
    run_blocking(move || read(&store, |connection| software_list_detail(connection, &name))).await?
}

fn read<T>(
    store: &CatalogStore,
    query: impl FnOnce(&rusqlite::Connection) -> rusqlite::Result<T>,
) -> Result<T, CommandError> {
    store
        .with_reader(|connection| Ok(query(connection)))?
        .ok_or(CommandError::CatalogNotImported)?
        .map_err(|error| CommandError::Internal {
            message: error.to_string(),
        })
}
