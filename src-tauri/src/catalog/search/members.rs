//! The machines of a user list, as found in one catalog.
//!
//! The Lists page shows every machine of a list, including those this
//! catalog does not have (they may come from another MAME version), with the
//! catalog description when there is one. The list is bound as one JSON
//! parameter, like the search `userLists` filter, and joined with
//! `machines` by its unique name; searching, counting and paging run in SQL,
//! so the frontend only receives one page.

use rusqlite::{Connection, params};
use serde::Serialize;

use super::{ListMembers, SearchError};

/// Largest page of list entries.
pub const MAX_PAGE_SIZE: u32 = 200;

/// One machine of a list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListMachine {
    /// MAME short name, as stored in the list.
    pub name: String,
    /// The catalog description; `None` when the catalog does not have it.
    pub description: Option<String>,
    /// Whether the catalog has the machine; `None` without a catalog.
    pub in_catalog: Option<bool>,
}

/// A page of the machines of a list matching a search.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListMachinePage {
    pub items: Vec<ListMachine>,
    /// Machines matching the search.
    pub matched: u64,
}

/// Lowercase search text, or `None` without one. Searched as plain text in
/// names and descriptions (never as SQL patterns).
pub fn search_key(search: Option<&str>) -> Option<String> {
    search
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_lowercase)
}

pub fn validate_page(page: u32, page_size: u32) -> Result<(), SearchError> {
    if page_size == 0 || page_size > MAX_PAGE_SIZE {
        return Err(SearchError::InvalidRequest(format!(
            "the page size must be between 1 and {MAX_PAGE_SIZE}"
        )));
    }
    if u64::from(page) * u64::from(page_size) > i64::MAX as u64 {
        return Err(SearchError::InvalidRequest(
            "the page is too large".to_owned(),
        ));
    }
    Ok(())
}

/// The machines of `members` matching `search` (short name or description,
/// case-insensitive), sorted by short name, one page at a time.
pub fn machine_page(
    connection: &Connection,
    members: &ListMembers,
    search: Option<&str>,
    page: u32,
    page_size: u32,
) -> Result<ListMachinePage, SearchError> {
    validate_page(page, page_size)?;
    let search = search_key(search);
    // `instr` on lowercase text: the search is plain text, never a pattern.
    const FROM: &str = "FROM json_each(?1) j LEFT JOIN machines m ON m.name = j.value
         WHERE ?2 IS NULL OR instr(lower(j.value), ?2) > 0
            OR instr(lower(m.description), ?2) > 0";
    let matched: i64 = connection.query_row(
        &format!("SELECT COUNT(*) {FROM}"),
        params![members.json(), search],
        |row| row.get(0),
    )?;
    let offset = i64::from(page) * i64::from(page_size);
    let items = connection
        .prepare_cached(&format!(
            "SELECT j.value, m.description, m.id IS NOT NULL {FROM}
             ORDER BY j.value LIMIT ?3 OFFSET ?4"
        ))?
        .query_map(
            params![members.json(), search, i64::from(page_size), offset],
            |row| {
                Ok(ListMachine {
                    name: row.get(0)?,
                    description: row.get(1)?,
                    in_catalog: Some(row.get(2)?),
                })
            },
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(ListMachinePage {
        items,
        matched: u64::try_from(matched).unwrap_or(0),
    })
}
