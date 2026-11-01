//! Search and faceted filtering of the catalog.
//!
//! - [`request`]: request models and their validation.
//! - `text`: conversion of user text into a safe FTS5 query.
//! - `sql`: composition of SQL conditions from validated values only.
//! - [`machines`], [`software`]: queries, facets and result mapping.
//!
//! The functions take a read-only [`rusqlite::Connection`] to the catalog and
//! do not depend on Tauri. Machines and software are searched separately.

pub mod machines;
pub mod members;
pub mod request;
pub mod software;
mod sql;
mod text;

use std::fmt;
use std::sync::Arc;

use serde::Serialize;

/// A page of results with the total and the requested facets.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResponse<T, F> {
    pub items: Vec<T>,
    /// Number of results matching the text and all filters.
    pub total: u64,
    /// Zero-based page index, as requested.
    pub page: u32,
    pub page_size: u32,
    pub page_count: u64,
    pub facets: Vec<FacetResult<F>>,
    /// Time spent executing the search, in milliseconds.
    pub elapsed_ms: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FacetResult<F> {
    pub facet: F,
    pub values: Vec<FacetValue>,
    /// Whether more values exist beyond the requested limit.
    pub has_more: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FacetValue {
    /// `None` for entries without a value (for example devices without a
    /// manufacturer). It can be selected as a filter value as well.
    pub value: Option<String>,
    pub count: u64,
}

/// Entries of a user list that restrict a search (`userLists` filter). Lists
/// are global and never stored in a catalog: the caller resolves each list and
/// its entries are bound as one JSON parameter, read with `json_each`, so the
/// restriction applies before counting, faceting and paging, whatever the size
/// of the list. Several lists are several AND conditions. Cloning shares the
/// JSON, so callers can cache it between searches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListMembers {
    json: Arc<str>,
}

impl ListMembers {
    /// Machines by short name.
    pub fn machines<'a>(names: impl IntoIterator<Item = &'a str>) -> Self {
        let names: Vec<&str> = names.into_iter().collect();
        Self {
            json: serde_json::json!(names).to_string().into(),
        }
    }

    /// Software by `(software list name, short name)`.
    pub fn software<'a>(keys: impl IntoIterator<Item = (&'a str, &'a str)>) -> Self {
        let keys: Vec<[&str; 2]> = keys.into_iter().map(|(list, name)| [list, name]).collect();
        Self {
            json: serde_json::json!(keys).to_string().into(),
        }
    }

    pub(super) fn json(&self) -> &str {
        &self.json
    }
}

/// Most lists one search combines (Favorites and a custom list, with room to spare).
pub const MAX_USER_LISTS: usize = 4;

/// The members are given for every list of the `userLists` filter.
fn check_members(lists: &[String], members: &[ListMembers]) -> Result<(), SearchError> {
    if lists.len() > MAX_USER_LISTS {
        return Err(SearchError::InvalidRequest(format!(
            "at most {MAX_USER_LISTS} user lists can restrict a search"
        )));
    }
    if lists.len() == members.len() {
        Ok(())
    } else {
        Err(SearchError::InvalidRequest(
            "the list filter was not resolved".to_owned(),
        ))
    }
}

#[derive(Debug)]
pub enum SearchError {
    /// The request is invalid; the message explains why.
    InvalidRequest(String),
    /// The catalog contains no software lists.
    SoftwareListsNotImported,
    Sqlite(rusqlite::Error),
}

impl fmt::Display for SearchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRequest(message) => write!(f, "invalid search request: {message}"),
            Self::SoftwareListsNotImported => {
                write!(f, "the software lists have not been imported")
            }
            Self::Sqlite(error) => write!(f, "search failed: {error}"),
        }
    }
}

impl std::error::Error for SearchError {}

impl From<rusqlite::Error> for SearchError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Sqlite(error)
    }
}

#[cfg(test)]
mod benchmark;
#[cfg(test)]
mod tests;
