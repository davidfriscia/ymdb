//! Commands of the global user lists: Favorites and custom lists.
//!
//! The lists are kept in memory and saved to `lists.json` after every change.
//! A change is applied to a copy and becomes visible only once saved, so a
//! failed save leaves the lists as they were. An unreadable file is never
//! replaced: every command reports it until it can be read. Changes are
//! serialized by the lists lock; every change of a custom list names the
//! lists it changes by id and sets wanted states, so a stale view in the
//! frontend can never undo a change made elsewhere.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::DialogExt;

use super::catalog::now_millis;
use super::installations::InstallationsState;
use super::{CommandError, data_change, run_blocking};
use crate::atomic_file;
use crate::catalog::search::ListMembers;
use crate::catalog::search::members::{self, ListMachine, ListMachinePage};
use crate::lists::exchange::{self, ListImportSummary, ParsedImport};
use crate::lists::folder_import::{self, FolderImportOutcome, Naming, PlannedAction};
use crate::lists::ini::{self, FolderFile, IniWarning};
use crate::lists::model::{ImportMode, ImportOutcome, List, ListEntry, ListError, ListKind, Lists};
use crate::lists::store::ListsStore;

/// Managed state: the lists, loaded once, and the imports waiting for confirmation.
pub struct ListsState {
    store: Option<ListsStore>,
    lists: Mutex<Option<Lists>>,
    pending_import: Mutex<Option<PendingImport>>,
    pending_folder_import: Mutex<Option<PendingFolderImport>>,
    /// Search members of each list, by kind and id, built once per change of
    /// the lists so that searches do not serialize a large list every time.
    members_cache: Mutex<HashMap<(MemberKind, String), ListMembers>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum MemberKind {
    Machine,
    Software,
}

struct PendingFolderImport {
    id: u64,
    file_name: String,
    file: FolderFile,
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
            pending_folder_import: Mutex::new(None),
            members_cache: Mutex::new(HashMap::new()),
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
        self.members_cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
        Ok(result)
    }

    /// The members of each list of `ids`, to restrict a search. The cache is
    /// read and filled under the lists lock, so it never outlives a change.
    fn members(&self, ids: &[String], kind: MemberKind) -> Result<Vec<ListMembers>, CommandError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let guard = self.loaded()?;
        let lists = guard.as_ref().ok_or_else(not_loaded)?;
        let mut cache = self
            .members_cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        ids.iter()
            .map(|id| {
                if let Some(members) = cache.get(&(kind, id.clone())) {
                    return Ok(members.clone());
                }
                let list = lists.list(id)?;
                let members = match kind {
                    MemberKind::Machine => ListMembers::machines(list.machine_names()),
                    MemberKind::Software => ListMembers::software(list.software_keys()),
                };
                cache.insert((kind, id.clone()), members.clone());
                Ok(members)
            })
            .collect()
    }

    /// The machine entries of every list of `ids`, to restrict a search.
    pub fn machine_members(&self, ids: &[String]) -> Result<Vec<ListMembers>, CommandError> {
        self.members(ids, MemberKind::Machine)
    }

    /// The software entries of every list of `ids`, to restrict a search.
    pub fn software_members(&self, ids: &[String]) -> Result<Vec<ListMembers>, CommandError> {
        self.members(ids, MemberKind::Software)
    }

    /// Keeps a parsed folder file until its import is applied or replaced.
    pub fn prepare_folder_import(&self, file_name: String, file: FolderFile) -> FolderFileSummary {
        let mut pending = self
            .pending_folder_import
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let id = pending.as_ref().map_or(1, |previous| previous.id + 1);
        let summary = FolderFileSummary {
            import_id: id,
            suggested_name: suggested_name(&file_name),
            file_name: file_name.clone(),
            root_entries: file.root.len(),
            folders: file.folders.len(),
            empty_folders: file.empty_folders,
            entries: file.root.len()
                + file
                    .folders
                    .iter()
                    .map(|folder| folder.machines.len())
                    .sum::<usize>(),
            duplicates: file.duplicates,
            warnings: file.warnings.clone(),
        };
        *pending = Some(PendingFolderImport {
            id,
            file_name,
            file,
        });
        summary
    }

    fn with_folder_import<T>(
        &self,
        import_id: u64,
        work: impl FnOnce(&PendingFolderImport) -> Result<T, CommandError>,
    ) -> Result<T, CommandError> {
        let pending = self
            .pending_folder_import
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        match pending.as_ref() {
            Some(prepared) if prepared.id == import_id => work(prepared),
            _ => Err(import_unavailable()),
        }
    }

    /// What importing the prepared file with these choices would change.
    pub fn preview_folder_import(
        &self,
        import_id: u64,
        name: &str,
        naming: Naming,
    ) -> Result<FolderImportPreview, CommandError> {
        self.with_folder_import(import_id, |prepared| {
            let plan =
                self.read(|lists| folder_import::plan(lists, &prepared.file, name, naming))??;
            Ok(FolderImportPreview {
                import_id,
                file_name: prepared.file_name.clone(),
                name: name.trim().to_owned(),
                naming,
                has_subfolders: prepared.file.has_subfolders(),
                to_create: plan.to_create(),
                to_merge: plan.to_merge(),
                empty_folders: prepared.file.empty_folders,
                new_entries: plan.new_entries(),
                existing_entries: plan.existing_entries(),
                duplicates: prepared.file.duplicates,
                warnings: prepared.file.warnings.clone(),
                lists: plan
                    .lists
                    .iter()
                    .map(|list| PlannedListView {
                        name: list.name.clone(),
                        action: list.action(),
                        entries: list.machines.len(),
                        new_entries: list.new_entries,
                    })
                    .collect(),
            })
        })
    }

    /// Applies the prepared file to the current lists, in one saved change.
    /// The plan is made again on the lists as they are now, so lists created,
    /// renamed or changed since the preview are merged correctly.
    pub fn apply_folder_import(
        &self,
        import_id: u64,
        name: &str,
        naming: Naming,
        now: u64,
    ) -> Result<FolderImportOutcome, CommandError> {
        let mut pending = self
            .pending_folder_import
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let prepared = match pending.as_ref() {
            Some(prepared) if prepared.id == import_id => prepared,
            _ => return Err(import_unavailable()),
        };
        let outcome = self.update(|lists| {
            folder_import::apply(lists, &prepared.file, name, naming, now, new_list_id)
                .map_err(CommandError::from)
        })?;
        *pending = None;
        Ok(outcome)
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
            ListError::NameInUse(_) => Self::InvalidRequest {
                message: error.to_string(),
            },
            ListError::SystemList(_) => Self::Refused {
                message: error.to_string(),
            },
        }
    }
}

fn import_unavailable() -> CommandError {
    CommandError::InvalidRequest {
        message: "this import is no longer available; choose the file again".to_owned(),
    }
}

/// Ids of new user lists: opaque and never derived from the name.
fn new_list_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

/// The import name proposed for a file: its name without extension, with an
/// uppercase first letter (`bootlegs.ini` gives `Bootlegs`).
fn suggested_name(file_name: &str) -> String {
    let stem = Path::new(file_name)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(file_name)
        .trim();
    let mut chars = stem.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => "Imported list".to_owned(),
    }
}

/// A custom list without its entries, for the Lists page and the pickers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListSummary {
    pub id: String,
    pub name: String,
    /// Machine entries.
    pub machines: usize,
    /// All entries (machines and software).
    pub entries: usize,
    pub created_at: Option<u64>,
    pub updated_at: Option<u64>,
}

impl ListSummary {
    fn of(list: &List) -> Self {
        Self {
            id: list.id.clone(),
            name: list.display_name().to_owned(),
            machines: list.machine_names().count(),
            entries: list.entries.len(),
            created_at: list.created_at,
            updated_at: list.updated_at,
        }
    }
}

pub fn user_list_summaries(lists: &Lists) -> Vec<ListSummary> {
    lists.user_lists().map(ListSummary::of).collect()
}

/// A parsed folder file, before the user chooses the import name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderFileSummary {
    /// Passed back to preview and apply this file.
    pub import_id: u64,
    pub file_name: String,
    /// The file name without extension, proposed as import name.
    pub suggested_name: String,
    /// Machines of `[ROOT_FOLDER]`.
    pub root_entries: usize,
    /// Subfolders with machines.
    pub folders: usize,
    pub empty_folders: usize,
    /// Distinct machines of the root and every folder, summed.
    pub entries: usize,
    pub duplicates: usize,
    pub warnings: Vec<IniWarning>,
}

/// One list of a folder import preview.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedListView {
    pub name: String,
    pub action: PlannedAction,
    /// Distinct machines of the file for this list.
    pub entries: usize,
    /// Machines the list does not have yet.
    pub new_entries: usize,
}

/// What a folder import would change, before confirming it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderImportPreview {
    pub import_id: u64,
    pub file_name: String,
    pub name: String,
    pub naming: Naming,
    pub has_subfolders: bool,
    pub to_create: usize,
    pub to_merge: usize,
    /// Subfolders without machines, not imported.
    pub empty_folders: usize,
    /// Memberships the import adds.
    pub new_entries: usize,
    /// Memberships already present in their list.
    pub existing_entries: usize,
    /// Machines listed twice in a folder of the file (imported once).
    pub duplicates: usize,
    pub warnings: Vec<IniWarning>,
    /// The resulting lists, root first, in file order.
    pub lists: Vec<PlannedListView>,
}

/// The wanted membership of an entry in one list.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MembershipChange {
    pub list_id: String,
    pub member: bool,
}

/// A page of the machines of a list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListEntriesPage {
    pub list: ListSummary,
    /// The installation whose catalog gave the descriptions; `None` without
    /// an imported catalog (entries are then shown by name only).
    pub installation_id: Option<String>,
    pub items: Vec<ListMachine>,
    /// Machines matching the search.
    pub matched: u64,
    pub page: u32,
    pub page_size: u32,
}

/// Machines of a list without a catalog: by name only, sorted, one page.
pub fn machine_page_without_catalog(
    list: &List,
    search: Option<&str>,
    page: u32,
    page_size: u32,
) -> Result<ListMachinePage, CommandError> {
    members::validate_page(page, page_size).map_err(|error| CommandError::InvalidRequest {
        message: error.to_string(),
    })?;
    let search = members::search_key(search);
    let mut names: Vec<&str> = list
        .machine_names()
        .filter(|name| {
            search
                .as_ref()
                .is_none_or(|text| name.contains(text.as_str()))
        })
        .collect();
    names.sort_unstable();
    let start = usize::try_from(u64::from(page) * u64::from(page_size)).unwrap_or(usize::MAX);
    Ok(ListMachinePage {
        matched: names.len() as u64,
        items: names
            .into_iter()
            .skip(start)
            .take(page_size as usize)
            .map(|name| ListMachine {
                name: name.to_owned(),
                description: None,
                in_catalog: None,
            })
            .collect(),
    })
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
                lists.touch(&list_id, now_millis())?;
                Ok(())
            })?;
        }
        Ok(member)
    })
    .await?
}

/// The custom lists (never Favorites), in creation order.
#[tauri::command]
pub async fn list_user_lists(app: AppHandle) -> Result<Vec<ListSummary>, CommandError> {
    run_blocking(move || app.state::<ListsState>().read(user_list_summaries)).await?
}

/// Creates an empty custom list. Names are unique ignoring case.
#[tauri::command]
pub async fn create_list(name: String, app: AppHandle) -> Result<ListSummary, CommandError> {
    run_blocking(move || {
        let _change = data_change()?;
        let id = new_list_id();
        app.state::<ListsState>().update(|lists| {
            lists.create_user_list(&id, &name, now_millis())?;
            Ok(ListSummary::of(lists.list(&id)?))
        })
    })
    .await?
}

/// Renames a custom list; its id and entries, and the saved filters using
/// it, are unchanged. A name already used is refused, never merged.
#[tauri::command]
pub async fn rename_list(
    list_id: String,
    name: String,
    app: AppHandle,
) -> Result<ListSummary, CommandError> {
    run_blocking(move || {
        let _change = data_change()?;
        app.state::<ListsState>().update(|lists| {
            lists.rename_user_list(&list_id, &name, now_millis())?;
            Ok(ListSummary::of(lists.list(&list_id)?))
        })
    })
    .await?
}

/// Deletes a custom list and its entries; Favorites cannot be deleted.
#[tauri::command]
pub async fn delete_list(list_id: String, app: AppHandle) -> Result<(), CommandError> {
    run_blocking(move || {
        let _change = data_change()?;
        app.state::<ListsState>().update(|lists| {
            lists.delete_user_list(&list_id)?;
            Ok(())
        })
    })
    .await?
}

/// One page of the machines of a custom list, with descriptions from the
/// catalog of `installation_id` when it has one. Machines missing from that
/// catalog are listed too.
#[tauri::command]
pub async fn get_list_entries(
    list_id: String,
    installation_id: Option<String>,
    search: Option<String>,
    page: u32,
    page_size: u32,
    lists: State<'_, ListsState>,
    installations: State<'_, InstallationsState>,
) -> Result<ListEntriesPage, CommandError> {
    let (list, members) = lists.read(|lists| {
        lists
            .list(&list_id)
            .map(|list| (list.clone(), ListMembers::machines(list.machine_names())))
    })??;
    let store = match &installation_id {
        Some(id) => Some(installations.resolve(id)?.catalog),
        None => None,
    };
    run_blocking(move || {
        let from_catalog = match &store {
            Some(store) => store.with_reader(|connection| {
                Ok(members::machine_page(
                    connection,
                    &members,
                    search.as_deref(),
                    page,
                    page_size,
                ))
            })?,
            None => None,
        };
        let (catalog_installation, result) = match from_catalog {
            Some(result) => (
                installation_id,
                result.map_err(|error| CommandError::InvalidRequest {
                    message: error.to_string(),
                })?,
            ),
            None => (
                None,
                machine_page_without_catalog(&list, search.as_deref(), page, page_size)?,
            ),
        };
        Ok(ListEntriesPage {
            list: ListSummary::of(&list),
            installation_id: catalog_installation,
            items: result.items,
            matched: result.matched,
            page,
            page_size,
        })
    })
    .await?
}

/// Ids of the custom lists that contain `entry`.
#[tauri::command]
pub async fn get_entry_lists(
    entry: ListEntry,
    app: AppHandle,
) -> Result<Vec<String>, CommandError> {
    run_blocking(move || {
        entry
            .validate()
            .map_err(|message| CommandError::InvalidRequest { message })?;
        app.state::<ListsState>()
            .read(|lists| lists.user_lists_containing(&entry))
    })
    .await?
}

/// Sets the membership of `entry` in several custom lists in one saved
/// change (the Manage Lists window). Only the lists named change: each gets
/// the wanted state, so changes made elsewhere since the window opened are
/// kept. Fails, changing nothing, if a list no longer exists. Returns the
/// ids of the custom lists containing the entry afterwards.
#[tauri::command]
pub async fn set_entry_memberships(
    entry: ListEntry,
    changes: Vec<MembershipChange>,
    app: AppHandle,
) -> Result<Vec<String>, CommandError> {
    run_blocking(move || {
        let _change = data_change()?;
        let changes: Vec<(String, bool)> = changes
            .into_iter()
            .map(|change| (change.list_id, change.member))
            .collect();
        let state = app.state::<ListsState>();
        let changed = state.read(|lists| {
            changes.iter().try_fold(false, |changed, (id, member)| {
                lists
                    .list(id)
                    .map(|list| changed || list.contains(&entry) != *member)
            })
        })??;
        if changed {
            state.update(|lists| {
                lists.set_user_memberships(&entry, &changes, now_millis())?;
                Ok(())
            })?;
        }
        state.read(|lists| lists.user_lists_containing(&entry))
    })
    .await?
}

/// Lets the user choose a MAME folder file (`.ini`) and reads it. Nothing
/// changes until the import is applied. `None` when the user cancels.
#[tauri::command]
pub async fn choose_folder_import_file(
    app: AppHandle,
) -> Result<Option<FolderFileSummary>, CommandError> {
    run_blocking(move || {
        let selection = app
            .dialog()
            .file()
            .set_title("Import lists from a folder file")
            .add_filter("MAME folder files", &["ini"])
            .blocking_pick_file();
        let Some(selection) = selection else {
            return Ok(None);
        };
        let path = selection
            .into_path()
            .map_err(|error| CommandError::Internal {
                message: format!("unsupported file selection: {error}"),
            })?;
        let file = ini::read_file(&path).map_err(|error| CommandError::InvalidFile {
            message: error.to_string(),
        })?;
        let file_name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        Ok(Some(
            app.state::<ListsState>()
                .prepare_folder_import(file_name, file),
        ))
    })
    .await?
}

/// The lists a folder import would create or merge into, for these choices.
#[tauri::command]
pub async fn preview_folder_import(
    import_id: u64,
    name: String,
    naming: Naming,
    app: AppHandle,
) -> Result<FolderImportPreview, CommandError> {
    run_blocking(move || {
        app.state::<ListsState>()
            .preview_folder_import(import_id, &name, naming)
    })
    .await?
}

/// Applies a folder import atomically: every list is created or merged, or
/// none is.
#[tauri::command]
pub async fn apply_folder_import(
    import_id: u64,
    name: String,
    naming: Naming,
    app: AppHandle,
) -> Result<FolderImportOutcome, CommandError> {
    run_blocking(move || {
        let _change = data_change()?;
        app.state::<ListsState>()
            .apply_folder_import(import_id, &name, naming, now_millis())
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
