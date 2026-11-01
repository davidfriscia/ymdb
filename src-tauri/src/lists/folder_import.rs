//! Turns a parsed folder file ([`super::ini`]) into user lists.
//!
//! The user chooses an import name and, for files with subfolders, how the
//! subfolder lists are named ([`Naming`]):
//!
//! - `[ROOT_FOLDER]`, when it has machines, becomes the list named after the
//!   import name. Its machines are never copied into the subfolder lists.
//! - Each subfolder with machines becomes a list named after the subfolder
//!   (`Arcade / Pinball`), or after the import name and the subfolder joined
//!   by [`SEPARATOR`] (`Category / Arcade / Pinball`). Names are kept as
//!   written; empty subfolders create nothing.
//!
//! Names are compared ignoring case ([`name_key`]). A planned list whose name
//! matches an existing user list is merged into it: the existing entries stay
//! and only the missing machines are added, so importing a file twice with
//! the same choices changes nothing the second time. Planned lists with the
//! same name (a subfolder named like the import name, for example) are
//! combined first.
//!
//! [`plan`] only reads the lists (the preview); [`apply`] plans again on the
//! current lists and applies every list in the same change, so the import is
//! saved entirely or not at all.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use super::ini::FolderFile;
use super::model::{
    ListEntry, ListError, Lists, MAX_ENTRIES, MAX_USER_LISTS, name_key, validate_list_name,
};

/// Joins the import name and a subfolder name in [`Naming::Prefixed`].
pub const SEPARATOR: &str = " / ";

/// How the lists of subfolders are named.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Naming {
    /// The subfolder name alone: `Arcade / Pinball`.
    Subfolder,
    /// The import name, then the subfolder name: `Category / Arcade / Pinball`.
    Prefixed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PlannedAction {
    Create,
    /// Merged into an existing list with the same name.
    Merge,
}

/// One list the import creates or merges into.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedList {
    /// The name of the new list, or of the existing list it merges into.
    pub name: String,
    /// The existing list, when merging.
    pub existing_id: Option<String>,
    /// Distinct machines of the file for this list.
    pub machines: Vec<String>,
    /// Machines not in the existing list yet (all of them for a new list).
    pub new_entries: usize,
}

impl PlannedList {
    pub fn action(&self) -> PlannedAction {
        if self.existing_id.is_some() {
            PlannedAction::Merge
        } else {
            PlannedAction::Create
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportPlan {
    /// In file order: the root list first, then the subfolders.
    pub lists: Vec<PlannedList>,
}

impl ImportPlan {
    pub fn to_create(&self) -> usize {
        self.lists
            .iter()
            .filter(|list| list.existing_id.is_none())
            .count()
    }

    pub fn to_merge(&self) -> usize {
        self.lists.len() - self.to_create()
    }

    /// Memberships the import adds.
    pub fn new_entries(&self) -> usize {
        self.lists.iter().map(|list| list.new_entries).sum()
    }

    /// Memberships of the file already present in their list.
    pub fn existing_entries(&self) -> usize {
        self.lists
            .iter()
            .map(|list| list.machines.len() - list.new_entries)
            .sum()
    }
}

/// What an applied import changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderImportOutcome {
    pub created: usize,
    pub merged: usize,
    pub entries_added: usize,
}

/// The names of the lists of `file`, before merging, in file order.
fn list_names<'a>(
    file: &'a FolderFile,
    import_name: &str,
    naming: Naming,
) -> Vec<(String, &'a [String])> {
    let mut names = Vec::with_capacity(file.folders.len() + 1);
    if !file.root.is_empty() {
        names.push((import_name.to_owned(), file.root.as_slice()));
    }
    for folder in &file.folders {
        let name = match naming {
            Naming::Subfolder => folder.name.clone(),
            Naming::Prefixed => format!("{import_name}{SEPARATOR}{}", folder.name),
        };
        names.push((name, folder.machines.as_slice()));
    }
    names
}

/// Plans the import of `file` on the current `lists`, without changing them.
pub fn plan(
    lists: &Lists,
    file: &FolderFile,
    import_name: &str,
    naming: Naming,
) -> Result<ImportPlan, ListError> {
    let import_name = import_name.trim();
    validate_list_name(import_name)
        .map_err(|message| ListError::Invalid(format!("import name: {message}")))?;

    // Lists of the file with the same name (ignoring case) are combined.
    let mut planned: Vec<(String, Vec<String>)> = Vec::new();
    let mut by_key: HashMap<String, usize> = HashMap::new();
    for (name, machines) in list_names(file, import_name, naming) {
        validate_list_name(&name)
            .map_err(|message| ListError::Invalid(format!("list '{name}': {message}")))?;
        match by_key.get(&name_key(&name)) {
            Some(&index) => {
                let (_, combined) = &mut planned[index];
                let present: HashSet<String> = combined.iter().cloned().collect();
                combined.extend(
                    machines
                        .iter()
                        .filter(|machine| !present.contains(*machine))
                        .cloned(),
                );
            }
            None => {
                by_key.insert(name_key(&name), planned.len());
                planned.push((name, machines.to_vec()));
            }
        }
    }

    let mut result = Vec::with_capacity(planned.len());
    for (name, machines) in planned {
        let (name, existing_id, new_entries) = match lists.user_list_named(&name) {
            Some(existing) => {
                let present: HashSet<&str> = existing.machine_names().collect();
                let new_entries = machines
                    .iter()
                    .filter(|machine| !present.contains(machine.as_str()))
                    .count();
                if existing.entries.len() + new_entries > MAX_ENTRIES {
                    return Err(ListError::Invalid(format!(
                        "the list '{}' cannot have more than {MAX_ENTRIES} entries",
                        existing.display_name()
                    )));
                }
                (
                    existing.display_name().to_owned(),
                    Some(existing.id.clone()),
                    new_entries,
                )
            }
            None => {
                if machines.len() > MAX_ENTRIES {
                    return Err(ListError::Invalid(format!(
                        "the list '{name}' cannot have more than {MAX_ENTRIES} entries"
                    )));
                }
                let count = machines.len();
                (name, None, count)
            }
        };
        result.push(PlannedList {
            name,
            existing_id,
            machines,
            new_entries,
        });
    }
    let plan = ImportPlan { lists: result };
    if lists.user_lists().count() + plan.to_create() > MAX_USER_LISTS {
        return Err(ListError::Invalid(format!(
            "the import would create {} lists; there cannot be more than {MAX_USER_LISTS} lists",
            plan.to_create()
        )));
    }
    Ok(plan)
}

/// Plans the import on `lists` and applies it: new lists get ids from
/// `new_id`, existing lists receive the missing machines. The caller saves
/// `lists` once, so either every list changes or none does.
pub fn apply(
    lists: &mut Lists,
    file: &FolderFile,
    import_name: &str,
    naming: Naming,
    now: u64,
    mut new_id: impl FnMut() -> String,
) -> Result<FolderImportOutcome, ListError> {
    let plan = plan(lists, file, import_name, naming)?;
    let mut outcome = FolderImportOutcome {
        created: 0,
        merged: 0,
        entries_added: 0,
    };
    for planned in plan.lists {
        let entries: Vec<ListEntry> = planned
            .machines
            .into_iter()
            .map(|name| ListEntry::Machine { name })
            .collect();
        let id = match planned.existing_id {
            Some(id) => {
                outcome.merged += 1;
                id
            }
            None => {
                let id = new_id();
                lists.create_user_list(&id, &planned.name, now)?;
                outcome.created += 1;
                id
            }
        };
        outcome.entries_added += lists.merge_into(&id, &entries, now)?;
    }
    Ok(outcome)
}
