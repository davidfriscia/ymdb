//! Search request models, as sent by the frontend, and their validation.
//!
//! Every field has a default, so the frontend only sends what it uses.
//! Unknown fields are rejected to surface contract mismatches early.

use std::collections::HashSet;
use std::hash::Hash;

use serde::{Deserialize, Serialize};

use super::SearchError;

pub const DEFAULT_PAGE_SIZE: u32 = 50;
pub const MAX_PAGE_SIZE: u32 = 200;
/// Highest accepted page index; deep pages are slower with offset pagination.
pub const MAX_PAGE: u32 = 100_000;
pub const DEFAULT_FACET_LIMIT: u32 = 50;
/// High enough for every value of the classification facets (about 800
/// category nodes and 720 series in the support files of MAME 0.288).
pub const MAX_FACET_LIMIT: u32 = 1000;
/// Maximum length of the search text, in characters.
pub const MAX_TEXT_LENGTH: usize = 200;
/// Maximum length of a facet value search, in characters.
pub const MAX_FACET_SEARCH_LENGTH: usize = 100;
/// Maximum number of values for one filter.
pub const MAX_FILTER_VALUES: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SortDirection {
    #[default]
    Ascending,
    Descending,
}

/// Restricts results to parents (not clones) or to clones.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CloneFilter {
    Parents,
    Clones,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FacetRequest<F> {
    pub facet: F,
    /// Case-insensitive substring that facet values must contain (ASCII
    /// case folding), to find values of large facets.
    #[serde(default)]
    pub search: Option<String>,
    /// Maximum number of values returned (default 50, at most 200).
    #[serde(default)]
    pub limit: Option<u32>,
}

// Machines

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MachineSort {
    /// Exact name first, then FTS5 bm25 rank. Without search text, `name`.
    Relevance,
    Name,
    Description,
    Year,
    Manufacturer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MachineFacet {
    Manufacturer,
    Year,
    DriverStatus,
    EmulationStatus,
    SoftwareList,
    /// Classifications from the support files (see `catalog::classification`).
    Genre,
    /// Category nodes, every ancestor included, in path order.
    Category,
    Series,
    /// Versions in MAME version order.
    VersionAdded,
}

/// Versions added between `from` and `to` (both included, in MAME version
/// order); a missing bound is open.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct VersionRange {
    pub from: Option<String>,
    pub to: Option<String>,
}

/// Machine filters. Within a filter, values are alternatives (OR); different
/// filters must all match (AND). `None` in a value list selects entries
/// without a value. Unset filters do not restrict the results.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct MachineFilters {
    pub manufacturers: Vec<Option<String>>,
    pub years: Vec<Option<String>>,
    pub clones: Option<CloneFilter>,
    /// Clones of this parent machine (short name).
    pub clone_of: Option<String>,
    pub bios: Option<bool>,
    pub device: Option<bool>,
    pub mechanical: Option<bool>,
    pub runnable: Option<bool>,
    /// Overall driver status (`good`, `imperfect`, `preliminary`).
    pub driver_statuses: Vec<Option<String>>,
    /// Driver emulation status (`good`, `imperfect`, `preliminary`).
    pub emulation_statuses: Vec<Option<String>>,
    /// Whether the catalog lists at least one ROM for the machine (any dump
    /// status). This is not the availability of ROM files on disk.
    pub has_roms: Option<bool>,
    /// Whether the catalog lists at least one disk (CHD) for the machine.
    pub has_disks: Option<bool>,
    /// Machines referencing at least one of these software lists.
    pub software_lists: Vec<String>,
    /// Machines of these genres (active support file source).
    pub genres: Vec<String>,
    /// Machines of these category nodes; a node selects its descendants.
    pub categories: Vec<String>,
    /// Machines of these series (a machine may belong to several).
    pub series: Vec<String>,
    /// Machines added in these versions; alternatives of `version_added_range`.
    pub versions_added: Vec<String>,
    pub version_added_range: Option<VersionRange>,
    /// Only machines of this user list (for example `favorites`).
    pub list: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct MachineSearchRequest {
    pub text: Option<String>,
    pub filters: MachineFilters,
    /// Default: `relevance` with search text, `name` otherwise.
    pub sort: Option<MachineSort>,
    pub direction: SortDirection,
    /// Zero-based page index.
    pub page: u32,
    pub page_size: Option<u32>,
    pub facets: Vec<FacetRequest<MachineFacet>>,
}

// Software

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SoftwareSort {
    /// Exact name first, then FTS5 bm25 rank. Without search text, `name`.
    Relevance,
    Name,
    Description,
    Year,
    Publisher,
    List,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SoftwareFacet {
    List,
    Publisher,
    Year,
    Supported,
    Interface,
}

/// Support status declared by MAME for a software (`supported` attribute).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SupportStatus {
    Yes,
    Partial,
    No,
}

impl SupportStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Yes => "yes",
            Self::Partial => "partial",
            Self::No => "no",
        }
    }
}

/// Software filters; same combination rules as [`MachineFilters`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct SoftwareFilters {
    pub lists: Vec<String>,
    pub publishers: Vec<Option<String>>,
    pub years: Vec<Option<String>>,
    pub clones: Option<CloneFilter>,
    /// Clones of this parent software (short name; combine with `lists` to
    /// select the parent's list, since names are unique per list only).
    pub clone_of: Option<String>,
    pub supported: Vec<SupportStatus>,
    /// Whether the catalog lists at least one ROM for the software (any dump
    /// status). This is not the availability of files on disk.
    pub has_roms: Option<bool>,
    /// Whether the catalog lists at least one disk (CHD) for the software.
    pub has_disks: Option<bool>,
    /// Software with at least one part using one of these interfaces.
    pub interfaces: Vec<String>,
    /// Only software of this user list (for example `favorites`).
    pub list: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct SoftwareSearchRequest {
    pub text: Option<String>,
    pub filters: SoftwareFilters,
    /// Default: `relevance` with search text, `name` otherwise.
    pub sort: Option<SoftwareSort>,
    pub direction: SortDirection,
    /// Zero-based page index.
    pub page: u32,
    pub page_size: Option<u32>,
    pub facets: Vec<FacetRequest<SoftwareFacet>>,
}

// Validation

/// Page parameters after validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageRequest {
    pub page: u32,
    pub page_size: u32,
}

impl PageRequest {
    pub fn offset(self) -> u64 {
        u64::from(self.page) * u64::from(self.page_size)
    }
}

pub(super) fn validate_page(page: u32, page_size: Option<u32>) -> Result<PageRequest, SearchError> {
    let page_size = page_size.unwrap_or(DEFAULT_PAGE_SIZE);
    if !(1..=MAX_PAGE_SIZE).contains(&page_size) {
        return Err(invalid(format!(
            "page size must be between 1 and {MAX_PAGE_SIZE}, got {page_size}"
        )));
    }
    if page > MAX_PAGE {
        return Err(invalid(format!(
            "page must be at most {MAX_PAGE}, got {page}"
        )));
    }
    Ok(PageRequest { page, page_size })
}

pub(super) fn validate_text(text: Option<&str>) -> Result<(), SearchError> {
    match text {
        Some(text) if text.chars().count() > MAX_TEXT_LENGTH => Err(invalid(format!(
            "search text must be at most {MAX_TEXT_LENGTH} characters"
        ))),
        _ => Ok(()),
    }
}

pub(super) fn validate_values<T>(name: &str, values: &[T]) -> Result<(), SearchError> {
    if values.len() > MAX_FILTER_VALUES {
        return Err(invalid(format!(
            "filter '{name}' accepts at most {MAX_FILTER_VALUES} values"
        )));
    }
    Ok(())
}

/// Validated facet request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct FacetQuery<F> {
    pub facet: F,
    pub search: Option<String>,
    pub limit: u32,
}

pub(super) fn validate_facets<F: Copy + Eq + Hash + std::fmt::Debug>(
    facets: &[FacetRequest<F>],
) -> Result<Vec<FacetQuery<F>>, SearchError> {
    let mut seen = HashSet::new();
    facets
        .iter()
        .map(|request| {
            if !seen.insert(request.facet) {
                return Err(invalid(format!(
                    "facet {:?} requested twice",
                    request.facet
                )));
            }
            let limit = request.limit.unwrap_or(DEFAULT_FACET_LIMIT);
            if !(1..=MAX_FACET_LIMIT).contains(&limit) {
                return Err(invalid(format!(
                    "facet limit must be between 1 and {MAX_FACET_LIMIT}, got {limit}"
                )));
            }
            let search = request
                .search
                .as_deref()
                .map(str::trim)
                .filter(|search| !search.is_empty());
            if search.is_some_and(|search| search.chars().count() > MAX_FACET_SEARCH_LENGTH) {
                return Err(invalid(format!(
                    "facet search must be at most {MAX_FACET_SEARCH_LENGTH} characters"
                )));
            }
            Ok(FacetQuery {
                facet: request.facet,
                search: search.map(str::to_owned),
                limit,
            })
        })
        .collect()
}

fn invalid(message: String) -> SearchError {
    SearchError::InvalidRequest(message)
}
