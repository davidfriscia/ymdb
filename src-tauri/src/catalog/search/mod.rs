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
pub mod request;
pub mod software;
mod sql;
mod text;

use std::fmt;

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
