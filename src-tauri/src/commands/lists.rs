//! Commands of the global user lists (Favorites).
//!
//! The lists are kept in memory and saved to `lists.json` after every change.
//! A change is applied to a copy and becomes visible only once saved, so a
//! failed save leaves the lists as they were. An unreadable file is never
//! replaced: every command reports it until it can be read.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use serde::Serialize;
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::DialogExt;

use super::{CommandError, data_change, run_blocking};
use crate::atomic_file;
use crate::catalog::search::ListMembers;
use crate::lists::exchange::{self, ListImportSummary, ParsedImport};
use crate::lists::model::{ImportMode, ImportOutcome, ListEntry, ListError, ListKind, Lists};
use crate::lists::store::ListsStore;

/// Managed state: the lists, loaded once, and the import waiting for confirmation.
pub struct ListsState {
    store: Option<ListsStore>,
    lists: Mutex<Option<Lists>>,
    pending_import: Mutex<Option<PendingImport>>,
}

struct PendingImport {
    id: u64,
    parsed: ParsedImport,
}

impl ListsState {
    /// Loads `lists.json` if possible; a failed load is retried on next use.
    pub fn new(config_dir: Option<&Path>) -> Self {
        let store = config_dir.map(ListsStore::in_dir);
        let lists = store.as_ref().and_then(|store| store.load().ok());
        Self {
            store,
            lists: Mutex::new(lists),
            pending_import: Mutex::new(None),
        }
    }

    fn store(&self) -> Result<&ListsStore, CommandError> {
        self.store.as_ref().ok_or_else(|| CommandError::Settings {
            message: "the application configuration directory is unavailable".to_owned(),
        })
    }

    /// The loaded lists, under the lock that serializes changes.
    fn loaded(&self) -> Result<MutexGuard<'_, Option<Lists>>, CommandError> {
        let mut guard = self.lists.lock().unwrap_or_else(PoisonError::into_inner);
        if guard.is_none() {
            let lists = self
                .store()?
                .load()
                .map_err(|error| CommandError::Settings {
                    message: error.to_string(),
                })?;
            *guard = Some(lists);
        }
        Ok(guard)
    }

    pub fn read<T>(&self, read: impl FnOnce(&Lists) -> T) -> Result<T, CommandError> {
        let guard = self.loaded()?;
        let lists = guard.as_ref().ok_or_else(not_loaded)?;
        Ok(read(lists))
    }

    /// Applies `change` to a copy of the lists, saves it, then keeps it.
    pub fn update<T>(
        &self,
        change: impl FnOnce(&mut Lists) -> Result<T, CommandError>,
    ) -> Result<T, CommandError> {
        let mut guard = self.loaded()?;
        let mut next = guard.as_ref().ok_or_else(not_loaded)?.clone();
        let result = change(&mut next)?;
        self.store()?
            .save(&next)
            .map_err(|error| CommandError::Settings {
                message: error.to_string(),
            })?;
        *guard = Some(next);
        Ok(result)
    }

    /// The machine entries of `list`, to restrict a search; `None` without a list.
    pub fn machine_members(&self, list: Option<&str>) -> Result<Option<ListMembers>, CommandError> {
        list.map(|id| {
            self.read(|lists| {
                lists
                    .list(id)
                    .map(|list| ListMembers::machines(list.machine_names()))
            })?
            .map_err(CommandError::from)
        })
        .transpose()
    }

    /// The software entries of `list`, to restrict a search; `None` without a list.
    pub fn software_members(
        &self,
        list: Option<&str>,
    ) -> Result<Option<ListMembers>, CommandError> {
        list.map(|id| {
            self.read(|lists| {
                lists
                    .list(id)
                    .map(|list| ListMembers::software(list.software_keys()))
            })?
            .map_err(CommandError::from)
        })
        .transpose()
    }

    /// Checks an import and keeps it until it is applied or replaced.
    pub fn prepare_import(&self, parsed: ParsedImport) -> Result<ListsImportPreview, CommandError> {
        let lists = self.read(|lists| exchange::summarize(lists, &parsed))??;
        let mut pending = self
            .pending_import
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let id = pending.as_ref().map_or(1, |previous| previous.id + 1);
        let ignored_lists = parsed.ignored_lists.clone();
        *pending = Some(PendingImport { id, parsed });
        Ok(ListsImportPreview {
            import_id: id,
            lists,
            ignored_lists,
        })
    }

    /// Applies the prepared import `import_id` to every list at once.
    pub fn apply_import(
        &self,
        import_id: u64,
        mode: ImportMode,
    ) -> Result<Vec<ImportOutcome>, CommandError> {
        let mut pending = self
            .pending_import
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let parsed = match pending.as_ref() {
            Some(prepared) if prepared.id == import_id => &prepared.parsed,
            _ => {
                return Err(CommandError::InvalidRequest {
                    message: "this import is no longer available; choose the file again".to_owned(),
                });
            }
        };
        let outcomes = self.update(|lists| {
            parsed
                .lists
                .iter()
                .map(|imported| {
                    lists
                        .import(&imported.id, &imported.entries, mode)
                        .map_err(CommandError::from)
                })
                .collect()
        })?;
        *pending = None;
        Ok(outcomes)
    }
}

fn not_loaded() -> CommandError {
    CommandError::Internal {
        message: "the lists are not loaded".to_owned(),
    }
}

impl From<ListError> for CommandError {
    fn from(error: ListError) -> Self {
        match error {
            ListError::UnknownList(_) => Self::NotFound {
                message: error.to_string(),
            },
            ListError::Invalid(message) => Self::InvalidRequest { message },
        }
    }
}

/// A list with its entries, for the frontend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListView {
    pub id: String,
    pub kind: ListKind,
    pub name: String,
    pub entries: Vec<ListEntry>,
}

/// What an import file would change, before confirming it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListsImportPreview {
    /// Passed back to apply this import.
    pub import_id: u64,
    pub lists: Vec<ListImportSummary>,
    /// Lists of the file that this version does not import.
    pub ignored_lists: Vec<String>,
}

pub fn list_view(lists: &Lists, id: &str) -> Result<ListView, CommandError> {
    let list = lists.list(id)?;
    Ok(ListView {
        id: list.id.clone(),
        kind: list.kind,
        name: list.display_name().to_owned(),
        entries: list.entries.clone(),
    })
}

#[tauri::command]
pub async fn get_list(list_id: String, app: AppHandle) -> Result<ListView, CommandError> {
    run_blocking(move || {
        app.state::<ListsState>()
            .read(|lists| list_view(lists, &list_id))?
    })
    .await?
}

/// Adds `entry` to the list or removes it; returns whether it is a member now.
/// Setting the wanted state (not toggling) makes repeated requests harmless.
#[tauri::command]
pub async fn set_list_membership(
    list_id: String,
    entry: ListEntry,
    member: bool,
    app: AppHandle,
) -> Result<bool, CommandError> {
    run_blocking(move || {
        let _change = data_change()?;
        let state = app.state::<ListsState>();
        let changed = state.read(|lists| {
            lists
                .list(&list_id)
                .map(|list| list.contains(&entry) != member)
        })??;
        if changed {
            state.update(|lists| {
                lists.set_membership(&list_id, &entry, member)?;
                Ok(())
            })?;
        }
        Ok(member)
    })
    .await?
}

/// Exports the lists to a JSON file chosen with the native save dialog.
/// Returns the file written, or `None` when the user cancels.
#[tauri::command]
pub async fn export_lists(
    list_ids: Vec<String>,
    app: AppHandle,
) -> Result<Option<PathBuf>, CommandError> {
    run_blocking(move || {
        let document = app
            .state::<ListsState>()
            .read(|lists| exchange::export(lists, &list_ids))??;
        let selection = app
            .dialog()
            .file()
            .set_title("Export favorites")
            .set_file_name("ymdb-favorites.json")
            .add_filter("JSON", &["json"])
            .blocking_save_file();
        let Some(selection) = selection else {
            return Ok(None);
        };
        let path = selection
            .into_path()
            .map_err(|error| CommandError::Internal {
                message: format!("unsupported file selection: {error}"),
            })?;
        write_export(&path, &document)?;
        Ok(Some(path))
    })
    .await?
}

pub fn write_export(path: &Path, document: &exchange::ListsExport) -> Result<(), CommandError> {
    atomic_file::write(path, &exchange::to_json(document)).map_err(|error| CommandError::Settings {
        message: format!("failed to write '{}': {error}", path.display()),
    })
}

/// Lets the user choose an export file and checks it. Nothing changes until
/// the import is applied. `None` when the user cancels.
#[tauri::command]
pub async fn preview_lists_import(
    app: AppHandle,
) -> Result<Option<ListsImportPreview>, CommandError> {
    run_blocking(move || {
        let selection = app
            .dialog()
            .file()
            .set_title("Import favorites")
            .add_filter("JSON", &["json"])
            .blocking_pick_file();
        let Some(selection) = selection else {
            return Ok(None);
        };
        let path = selection
            .into_path()
            .map_err(|error| CommandError::Internal {
                message: format!("unsupported file selection: {error}"),
            })?;
        let state = app.state::<ListsState>();
        let parsed = state
            .read(|lists| exchange::read_file(&path, lists))?
            .map_err(|message| CommandError::InvalidFile { message })?;
        state.prepare_import(parsed).map(Some)
    })
    .await?
}

/// Applies the import previewed as `import_id`, atomically.
#[tauri::command]
pub async fn apply_lists_import(
    import_id: u64,
    mode: ImportMode,
    app: AppHandle,
) -> Result<Vec<ImportOutcome>, CommandError> {
    run_blocking(move || {
        let _change = data_change()?;
        app.state::<ListsState>().apply_import(import_id, mode)
    })
    .await?
}

#[cfg(test)]
mod tests;
