//! The portable export format of lists and its import.
//!
//! An export is a JSON document `{"format": "ymdb-lists", "version": 1,
//! "lists": [...]}` whose lists have the same shape as in `lists.json`. It
//! holds only list ids, kinds, names and MAME names: no paths, installation
//! ids or catalog ids, so it can be imported with any installation.
//!
//! Imports are checked completely before anything changes: the format, the
//! version, every entry. Lists this version cannot import (user lists, unknown
//! system lists) are reported and ignored; duplicate entries are counted and
//! imported once.

use std::collections::HashSet;
use std::fs::File;
use std::io::Read;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::model::{List, ListEntry, ListError, ListKind, Lists, MAX_ENTRIES};

pub const FORMAT: &str = "ymdb-lists";
pub const FORMAT_VERSION: u32 = 1;
/// Largest accepted import file.
pub const MAX_FILE_SIZE: u64 = 16 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListsExport {
    pub format: String,
    pub version: u32,
    pub lists: Vec<List>,
}

/// The export document of the lists `ids`, in that order.
pub fn export(lists: &Lists, ids: &[String]) -> Result<ListsExport, ListError> {
    let mut exported = Vec::with_capacity(ids.len());
    for id in ids {
        let list = lists.list(id)?;
        if !exported
            .iter()
            .any(|existing: &List| existing.id == list.id)
        {
            exported.push(list.clone());
        }
    }
    Ok(ListsExport {
        format: FORMAT.to_owned(),
        version: FORMAT_VERSION,
        lists: exported,
    })
}

pub fn to_json(export: &ListsExport) -> Vec<u8> {
    // Serializing plain strings and enums cannot fail.
    let mut content = serde_json::to_vec_pretty(export).unwrap_or_default();
    content.push(b'\n');
    content
}

/// The entries of one list of an import file, valid and deduplicated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedList {
    pub id: String,
    pub entries: Vec<ListEntry>,
    /// Entries listed more than once in the file (counted once).
    pub duplicates: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedImport {
    pub lists: Vec<ImportedList>,
    /// Displayed names of the lists of the file that are not imported.
    pub ignored_lists: Vec<String>,
}

/// Reads and checks an import file, refusing files larger than [`MAX_FILE_SIZE`].
pub fn read_file(path: &Path, lists: &Lists) -> Result<ParsedImport, String> {
    let file = File::open(path).map_err(|error| format!("the file cannot be read: {error}"))?;
    let mut content = Vec::new();
    file.take(MAX_FILE_SIZE + 1)
        .read_to_end(&mut content)
        .map_err(|error| format!("the file cannot be read: {error}"))?;
    if content.len() as u64 > MAX_FILE_SIZE {
        return Err(format!(
            "the file is larger than {} MB",
            MAX_FILE_SIZE / (1024 * 1024)
        ));
    }
    parse(&content, lists)
}

/// Checks an export document. Only the lists that exist in `lists` and are
/// system lists are imported; nothing is changed here.
pub fn parse(content: &[u8], lists: &Lists) -> Result<ParsedImport, String> {
    let value: Value = serde_json::from_slice(content)
        .map_err(|error| format!("the file is not valid JSON: {error}"))?;
    if value.get("format").and_then(Value::as_str) != Some(FORMAT) {
        return Err("the file is not a YMDB lists export".to_owned());
    }
    match value.get("version").and_then(Value::as_u64) {
        Some(version) if version == u64::from(FORMAT_VERSION) => {}
        Some(version) => {
            return Err(format!(
                "the file has version {version}; this version of YMDB reads version {FORMAT_VERSION}"
            ));
        }
        None => return Err("the file has no valid version".to_owned()),
    }
    let document: ListsExport = serde_json::from_value(value)
        .map_err(|error| format!("the file has an invalid structure: {error}"))?;

    let mut imported: Vec<ImportedList> = Vec::new();
    let mut ignored_lists = Vec::new();
    for list in document.lists {
        let importable = list.kind == ListKind::System && lists.get(&list.id).is_some();
        if !importable {
            ignored_lists.push(list.display_name().to_owned());
            continue;
        }
        list.validate_header()?;
        if imported.iter().any(|existing| existing.id == list.id) {
            return Err(format!("the list '{}' is defined twice", list.id));
        }
        imported.push(deduplicate(list)?);
    }
    if imported.is_empty() {
        return Err("the file contains no list that this version of YMDB can import".to_owned());
    }
    Ok(ParsedImport {
        lists: imported,
        ignored_lists,
    })
}

fn deduplicate(list: List) -> Result<ImportedList, String> {
    let mut seen = HashSet::with_capacity(list.entries.len());
    let mut entries = Vec::with_capacity(list.entries.len());
    let mut duplicates = 0;
    for entry in list.entries {
        entry.validate()?;
        if seen.contains(&entry) {
            duplicates += 1;
        } else {
            seen.insert(entry.clone());
            entries.push(entry);
        }
    }
    if entries.len() > MAX_ENTRIES {
        return Err(format!(
            "the list '{}' has more than {MAX_ENTRIES} entries",
            list.id
        ));
    }
    Ok(ImportedList {
        id: list.id,
        entries,
        duplicates,
    })
}

/// What importing one list would change, shown before confirming.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListImportSummary {
    pub list_id: String,
    pub name: String,
    /// Distinct entries in the file.
    pub entries: usize,
    /// Entries of the file that are not in the list yet.
    pub new_entries: usize,
    /// Entries of the file already in the list.
    pub existing_entries: usize,
    /// Entries listed more than once in the file.
    pub duplicates: usize,
    /// Entries of the list missing from the file: removed by a replace.
    pub removed_by_replace: usize,
}

pub fn summarize(
    lists: &Lists,
    parsed: &ParsedImport,
) -> Result<Vec<ListImportSummary>, ListError> {
    parsed
        .lists
        .iter()
        .map(|imported| {
            let list = lists.list(&imported.id)?;
            let current: HashSet<&ListEntry> = list.entries.iter().collect();
            let incoming: HashSet<&ListEntry> = imported.entries.iter().collect();
            let existing = imported
                .entries
                .iter()
                .filter(|entry| current.contains(entry))
                .count();
            Ok(ListImportSummary {
                list_id: list.id.clone(),
                name: list.display_name().to_owned(),
                entries: imported.entries.len(),
                new_entries: imported.entries.len() - existing,
                existing_entries: existing,
                duplicates: imported.duplicates,
                removed_by_replace: list
                    .entries
                    .iter()
                    .filter(|entry| !incoming.contains(entry))
                    .count(),
            })
        })
        .collect()
}
