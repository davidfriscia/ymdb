//! Lists, entries and their rules.

use std::collections::HashSet;
use std::fmt;

use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 1;
/// Id of the Favorites system list.
pub const FAVORITES: &str = "favorites";
/// System lists: they always exist and are named by the application.
pub const SYSTEM_LISTS: &[(&str, &str)] = &[(FAVORITES, "Favorites")];
/// Maximum number of entries of one list.
pub const MAX_ENTRIES: usize = 100_000;
/// Maximum length of a MAME short name or software list name, in characters.
pub const MAX_NAME_LENGTH: usize = 64;
/// Maximum length of the name of a user list, in characters.
pub const MAX_LIST_NAME_LENGTH: usize = 200;
const MAX_LIST_ID_LENGTH: usize = 64;
/// Maximum number of user lists (a `category.ini` import creates about 650).
pub const MAX_USER_LISTS: usize = 5_000;

/// A reference to a machine or a software, independent of any installation.
/// Software names are unique within their software list only.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ListEntry {
    /// A machine, by MAME short name.
    Machine { name: String },
    /// A software, by software list name and short name.
    Software { list_name: String, name: String },
}

impl ListEntry {
    #[cfg(test)]
    pub fn machine(name: &str) -> Self {
        Self::Machine {
            name: name.to_owned(),
        }
    }

    #[cfg(test)]
    pub fn software(list_name: &str, name: &str) -> Self {
        Self::Software {
            list_name: list_name.to_owned(),
            name: name.to_owned(),
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Machine { name } => validate_name("machine name", name),
            Self::Software { list_name, name } => {
                validate_name("software list name", list_name)?;
                validate_name("software name", name)
            }
        }
    }
}

impl fmt::Display for ListEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Machine { name } => write!(f, "machine '{name}'"),
            Self::Software { list_name, name } => write!(f, "software '{list_name}:{name}'"),
        }
    }
}

/// MAME names are short ASCII identifiers; separators and whitespace are
/// rejected so that an entry always has one unambiguous textual key.
fn validate_name(what: &str, name: &str) -> Result<(), String> {
    if name.is_empty() || name.len() > MAX_NAME_LENGTH {
        return Err(format!(
            "the {what} must have 1 to {MAX_NAME_LENGTH} characters"
        ));
    }
    if !name
        .bytes()
        .all(|byte| byte.is_ascii_graphic() && !matches!(byte, b'/' | b'\\' | b':'))
    {
        return Err(format!("the {what} '{name}' is not a valid MAME name"));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ListKind {
    /// Defined by the application (Favorites).
    System,
    /// Created by the user (custom lists).
    User,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct List {
    /// Stable identifier; never the displayed name.
    pub id: String,
    pub kind: ListKind,
    /// Name of a user list; system lists are named by the application.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Entries in the order they were added, without duplicates.
    #[serde(default)]
    pub entries: Vec<ListEntry>,
    /// Creation of a user list, in milliseconds since the Unix epoch (absent
    /// for system lists).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<u64>,
    /// Last change of the name or the entries of a user list.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<u64>,
}

impl List {
    pub fn system(id: &str) -> Self {
        Self {
            id: id.to_owned(),
            kind: ListKind::System,
            name: None,
            entries: Vec::new(),
            created_at: None,
            updated_at: None,
        }
    }

    /// A new, empty user list.
    pub fn user(id: &str, name: &str, now: u64) -> Self {
        Self {
            id: id.to_owned(),
            kind: ListKind::User,
            name: Some(name.to_owned()),
            entries: Vec::new(),
            created_at: Some(now),
            updated_at: Some(now),
        }
    }

    /// Records a change of a user list; system lists keep no dates.
    fn touch(&mut self, now: u64) {
        if self.kind == ListKind::User {
            self.updated_at = Some(now);
        }
    }

    /// The displayed name.
    pub fn display_name(&self) -> &str {
        match self.kind {
            ListKind::System => system_name(&self.id).unwrap_or(&self.id),
            ListKind::User => self.name.as_deref().unwrap_or(&self.id),
        }
    }

    /// Checks the identity of the list (not its entries).
    pub fn validate_header(&self) -> Result<(), String> {
        match self.kind {
            ListKind::System => {
                if system_name(&self.id).is_none() {
                    return Err(format!("unknown system list '{}'", self.id));
                }
                if self.name.is_some() {
                    return Err(format!("the system list '{}' cannot be renamed", self.id));
                }
            }
            ListKind::User => {
                if self.id.is_empty()
                    || self.id.len() > MAX_LIST_ID_LENGTH
                    || !self
                        .id
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
                {
                    return Err(format!("invalid list id '{}'", self.id));
                }
                if system_name(&self.id).is_some() {
                    return Err(format!("the list id '{}' is reserved", self.id));
                }
                validate_list_name(self.name.as_deref().unwrap_or_default())
                    .map_err(|message| format!("the list '{}': {message}", self.id))?;
            }
        }
        Ok(())
    }

    /// Checks the identity and the entries: valid, unique, not too many.
    pub fn validate(&self) -> Result<(), String> {
        self.validate_header()?;
        if self.entries.len() > MAX_ENTRIES {
            return Err(format!(
                "the list '{}' has more than {MAX_ENTRIES} entries",
                self.id
            ));
        }
        let mut seen = HashSet::with_capacity(self.entries.len());
        for entry in &self.entries {
            entry.validate()?;
            if !seen.insert(entry) {
                return Err(format!("{entry} is listed twice in '{}'", self.id));
            }
        }
        Ok(())
    }

    pub fn contains(&self, entry: &ListEntry) -> bool {
        self.entries.contains(entry)
    }

    /// Short names of the machine entries.
    pub fn machine_names(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().filter_map(|entry| match entry {
            ListEntry::Machine { name } => Some(name.as_str()),
            ListEntry::Software { .. } => None,
        })
    }

    /// `(list name, short name)` of the software entries.
    pub fn software_keys(&self) -> impl Iterator<Item = (&str, &str)> {
        self.entries.iter().filter_map(|entry| match entry {
            ListEntry::Software { list_name, name } => Some((list_name.as_str(), name.as_str())),
            ListEntry::Machine { .. } => None,
        })
    }
}

/// Checks the name of a user list as stored: 1 to [`MAX_LIST_NAME_LENGTH`]
/// characters, no leading or trailing whitespace, no control characters.
/// Every other character (including `/`, used by imported folder names) is
/// kept as written.
pub fn validate_list_name(name: &str) -> Result<(), String> {
    if name.trim().is_empty() || name.chars().count() > MAX_LIST_NAME_LENGTH {
        return Err(format!(
            "a list name must have 1 to {MAX_LIST_NAME_LENGTH} characters"
        ));
    }
    if name.trim() != name {
        return Err("a list name cannot start or end with spaces".to_owned());
    }
    if name.chars().any(char::is_control) {
        return Err("a list name cannot contain control characters".to_owned());
    }
    Ok(())
}

/// The key that identifies a user list name: names are unique ignoring case
/// (Unicode lowercase), so "Shooters" and "shooters" are the same list. The
/// displayed name keeps the spelling it was created with.
pub fn name_key(name: &str) -> String {
    name.trim().to_lowercase()
}

pub fn system_name(id: &str) -> Option<&'static str> {
    SYSTEM_LISTS
        .iter()
        .find(|(system_id, _)| *system_id == id)
        .map(|(_, name)| *name)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListError {
    UnknownList(String),
    Invalid(String),
    /// Another user list already has this name (ignoring case).
    NameInUse(String),
    /// The operation is not possible on a system list (Favorites).
    SystemList(String),
}

impl fmt::Display for ListError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownList(id) => write!(f, "the list '{id}' does not exist"),
            Self::Invalid(message) => f.write_str(message),
            Self::NameInUse(name) => write!(f, "a list named '{name}' already exists"),
            Self::SystemList(id) => {
                write!(
                    f,
                    "the list '{id}' is managed by YMDB and cannot be changed here"
                )
            }
        }
    }
}

impl std::error::Error for ListError {}

/// How imported entries are applied to a list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ImportMode {
    /// Adds the new entries after the existing ones; removes nothing.
    Merge,
    /// The list becomes exactly the imported entries.
    Replace,
}

/// Changes made to a list by an import.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportOutcome {
    pub list_id: String,
    pub added: usize,
    pub removed: usize,
    /// Number of entries after the import.
    pub entries: usize,
}

/// All the lists, as stored in `lists.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Lists {
    #[serde(default = "schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub lists: Vec<List>,
}

fn schema_version() -> u32 {
    SCHEMA_VERSION
}

impl Default for Lists {
    fn default() -> Self {
        Self::new()
    }
}

impl Lists {
    /// Every system list, empty.
    pub fn new() -> Self {
        let mut lists = Self {
            schema_version: SCHEMA_VERSION,
            lists: Vec::new(),
        };
        lists.ensure_system_lists();
        lists
    }

    /// Adds the system lists that are missing, so they are always available.
    pub fn ensure_system_lists(&mut self) {
        for (id, _) in SYSTEM_LISTS {
            if self.get(id).is_none() {
                self.lists.push(List::system(id));
            }
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(format!(
                "unsupported lists file version {}",
                self.schema_version
            ));
        }
        let mut ids = HashSet::new();
        let mut names = HashSet::new();
        for list in &self.lists {
            list.validate()?;
            if !ids.insert(list.id.as_str()) {
                return Err(format!("the list '{}' is defined twice", list.id));
            }
            if list.kind == ListKind::User && !names.insert(name_key(list.display_name())) {
                return Err(format!(
                    "the list name '{}' is used twice",
                    list.display_name()
                ));
            }
        }
        if names.len() > MAX_USER_LISTS {
            return Err(format!("there are more than {MAX_USER_LISTS} lists"));
        }
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<&List> {
        self.lists.iter().find(|list| list.id == id)
    }

    pub fn list(&self, id: &str) -> Result<&List, ListError> {
        self.get(id)
            .ok_or_else(|| ListError::UnknownList(id.to_owned()))
    }

    /// The lists created by the user, in creation order.
    pub fn user_lists(&self) -> impl Iterator<Item = &List> {
        self.lists.iter().filter(|list| list.kind == ListKind::User)
    }

    /// The user list with this name, ignoring case.
    pub fn user_list_named(&self, name: &str) -> Option<&List> {
        let key = name_key(name);
        self.user_lists()
            .find(|list| name_key(list.display_name()) == key)
    }

    /// Ids of the user lists that contain `entry`.
    pub fn user_lists_containing(&self, entry: &ListEntry) -> Vec<String> {
        self.user_lists()
            .filter(|list| list.contains(entry))
            .map(|list| list.id.clone())
            .collect()
    }

    fn user_list_mut(&mut self, id: &str) -> Result<&mut List, ListError> {
        let list = self.list_mut(id)?;
        if list.kind == ListKind::System {
            return Err(ListError::SystemList(id.to_owned()));
        }
        Ok(list)
    }

    /// Creates an empty user list with the id `id`; the name must not be
    /// used by another user list (ignoring case).
    pub fn create_user_list(&mut self, id: &str, name: &str, now: u64) -> Result<(), ListError> {
        let name = name.trim();
        validate_list_name(name).map_err(ListError::Invalid)?;
        if let Some(existing) = self.user_list_named(name) {
            return Err(ListError::NameInUse(existing.display_name().to_owned()));
        }
        if self.user_lists().count() >= MAX_USER_LISTS {
            return Err(ListError::Invalid(format!(
                "there cannot be more than {MAX_USER_LISTS} lists"
            )));
        }
        if self.get(id).is_some() {
            return Err(ListError::Invalid(format!("the list id '{id}' is in use")));
        }
        let list = List::user(id, name, now);
        list.validate_header().map_err(ListError::Invalid)?;
        self.lists.push(list);
        Ok(())
    }

    /// Renames a user list, keeping its id and entries. A name used by
    /// another list is refused: renaming never merges lists.
    pub fn rename_user_list(&mut self, id: &str, name: &str, now: u64) -> Result<(), ListError> {
        let name = name.trim();
        validate_list_name(name).map_err(ListError::Invalid)?;
        self.user_list_mut(id)?;
        if let Some(existing) = self.user_list_named(name)
            && existing.id != id
        {
            return Err(ListError::NameInUse(existing.display_name().to_owned()));
        }
        let list = self.user_list_mut(id)?;
        if list.name.as_deref() != Some(name) {
            list.name = Some(name.to_owned());
            list.touch(now);
        }
        Ok(())
    }

    /// Deletes a user list and its entries; nothing else changes.
    pub fn delete_user_list(&mut self, id: &str) -> Result<(), ListError> {
        self.user_list_mut(id)?;
        self.lists.retain(|list| list.id != id);
        Ok(())
    }

    /// Sets the membership of `entry` in several user lists at once, as
    /// `(list id, member)` pairs of wanted states. Lists not named keep their
    /// entries, so changes made elsewhere in the meantime are kept. Fails,
    /// changing nothing, if a list does not exist or is a system list.
    /// Returns the number of lists changed.
    pub fn set_user_memberships(
        &mut self,
        entry: &ListEntry,
        changes: &[(String, bool)],
        now: u64,
    ) -> Result<usize, ListError> {
        entry.validate().map_err(ListError::Invalid)?;
        for (id, _) in changes {
            if self.list(id)?.kind == ListKind::System {
                return Err(ListError::SystemList(id.clone()));
            }
        }
        let mut changed = 0;
        for (id, member) in changes {
            if self.set_membership(id, entry, *member)? {
                self.touch(id, now)?;
                changed += 1;
            }
        }
        Ok(changed)
    }

    /// Records a change of the list `id` (user lists only keep dates).
    pub fn touch(&mut self, id: &str, now: u64) -> Result<(), ListError> {
        self.list_mut(id)?.touch(now);
        Ok(())
    }

    /// Adds entries to a user list after the existing ones: a set union,
    /// entries already present stay where they are and nothing is removed.
    /// Returns the number of entries added.
    pub fn merge_into(
        &mut self,
        id: &str,
        entries: &[ListEntry],
        now: u64,
    ) -> Result<usize, ListError> {
        let list = self.user_list_mut(id)?;
        let existing: HashSet<&ListEntry> = list.entries.iter().collect();
        let mut seen = HashSet::new();
        let added: Vec<ListEntry> = entries
            .iter()
            .filter(|entry| !existing.contains(entry) && seen.insert(*entry))
            .cloned()
            .collect();
        if list.entries.len() + added.len() > MAX_ENTRIES {
            return Err(ListError::Invalid(format!(
                "the list '{}' cannot have more than {MAX_ENTRIES} entries",
                list.display_name()
            )));
        }
        let count = added.len();
        if count > 0 {
            list.entries.extend(added);
            list.touch(now);
        }
        Ok(count)
    }

    fn list_mut(&mut self, id: &str) -> Result<&mut List, ListError> {
        self.lists
            .iter_mut()
            .find(|list| list.id == id)
            .ok_or_else(|| ListError::UnknownList(id.to_owned()))
    }

    /// Adds `entry` to the list or removes it from it. Returns whether the
    /// list changed: adding a member or removing a non-member does nothing.
    pub fn set_membership(
        &mut self,
        id: &str,
        entry: &ListEntry,
        member: bool,
    ) -> Result<bool, ListError> {
        entry.validate().map_err(ListError::Invalid)?;
        let list = self.list_mut(id)?;
        let position = list.entries.iter().position(|existing| existing == entry);
        match (member, position) {
            (true, None) => {
                if list.entries.len() >= MAX_ENTRIES {
                    return Err(ListError::Invalid(format!(
                        "the list '{id}' cannot have more than {MAX_ENTRIES} entries"
                    )));
                }
                list.entries.push(entry.clone());
                Ok(true)
            }
            (false, Some(index)) => {
                list.entries.remove(index);
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    /// Applies imported entries (valid and without duplicates) to a list.
    pub fn import(
        &mut self,
        id: &str,
        entries: &[ListEntry],
        mode: ImportMode,
    ) -> Result<ImportOutcome, ListError> {
        for entry in entries {
            entry.validate().map_err(ListError::Invalid)?;
        }
        let imported: HashSet<&ListEntry> = entries.iter().collect();
        if imported.len() != entries.len() {
            return Err(ListError::Invalid(
                "the imported entries contain duplicates".to_owned(),
            ));
        }
        let list = self.list_mut(id)?;
        let existing: HashSet<&ListEntry> = list.entries.iter().collect();
        let added: Vec<ListEntry> = entries
            .iter()
            .filter(|entry| !existing.contains(entry))
            .cloned()
            .collect();
        let added_count = added.len();
        let (next, removed) = match mode {
            ImportMode::Merge => {
                let mut next = list.entries.clone();
                next.extend(added);
                (next, 0)
            }
            ImportMode::Replace => {
                let removed = list
                    .entries
                    .iter()
                    .filter(|entry| !imported.contains(entry))
                    .count();
                (entries.to_vec(), removed)
            }
        };
        if next.len() > MAX_ENTRIES {
            return Err(ListError::Invalid(format!(
                "the list '{id}' cannot have more than {MAX_ENTRIES} entries"
            )));
        }
        list.entries = next;
        Ok(ImportOutcome {
            list_id: id.to_owned(),
            added: added_count,
            removed,
            entries: list.entries.len(),
        })
    }
}
