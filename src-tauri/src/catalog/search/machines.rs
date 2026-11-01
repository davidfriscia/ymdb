//! Machine search.

use std::time::Instant;

use rusqlite::types::Value;
use rusqlite::{Connection, params_from_iter};
use serde::Serialize;

use super::request::{
    CloneFilter, FacetQuery, MachineFacet, MachineFilters, MachineSearchRequest, MachineSort,
    SortDirection, validate_facets, validate_page, validate_text, validate_values,
};
use super::sql::{Conditions, count, facet_values, nulls_last, placeholders, text_values};
use super::text::{self, TextQuery};
use super::{FacetResult, ListMembers, SearchError, SearchResponse, check_members};
use crate::support_files::version;

/// Lightweight machine data for result lists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MachineSummary {
    pub name: String,
    pub description: String,
    pub year: Option<String>,
    pub manufacturer: Option<String>,
    pub clone_of: Option<String>,
    pub is_bios: bool,
    pub is_device: bool,
    pub is_mechanical: bool,
    pub is_runnable: bool,
    pub driver_status: Option<String>,
    pub emulation_status: Option<String>,
}

pub type MachineSearchResponse = SearchResponse<MachineSummary, MachineFacet>;

/// Relevance: exact short name first, then bm25 with these column weights
/// (name, description, manufacturer, year).
const RANK: &str = "bm25(machines_fts, 10.0, 4.0, 2.0, 1.0)";

pub fn search_machines(
    connection: &Connection,
    request: &MachineSearchRequest,
    members: &[ListMembers],
) -> Result<MachineSearchResponse, SearchError> {
    let started = Instant::now();
    let page = validate_page(request.page, request.page_size)?;
    validate_text(request.text.as_deref())?;
    validate_filters(&request.filters)?;
    check_members(&request.filters.user_lists, members)?;
    let facet_queries = validate_facets(&request.facets)?;
    let text = text::parse(request.text.as_deref())?;

    let from = from_clause(text.as_ref());
    let conditions = conditions(&request.filters, members, text.as_ref(), None);
    let total = count(connection, from, &conditions)?;

    let (order, order_params) = order_by(request.sort, request.direction, text.as_ref());
    let sql = format!(
        "SELECT m.name, m.description, m.year, m.manufacturer, m.clone_of, m.is_bios,
                m.is_device, m.is_mechanical, m.is_runnable, m.driver_status, m.driver_emulation
         FROM {from}{} ORDER BY {order} LIMIT ? OFFSET ?",
        conditions.where_clause()
    );
    let mut params = conditions.params().to_vec();
    params.extend(order_params);
    params.push(Value::Integer(i64::from(page.page_size)));
    params.push(Value::Integer(
        i64::try_from(page.offset()).unwrap_or(i64::MAX),
    ));
    let items = connection
        .prepare_cached(&sql)?
        .query_map(params_from_iter(&params), |row| {
            Ok(MachineSummary {
                name: row.get(0)?,
                description: row.get(1)?,
                year: row.get(2)?,
                manufacturer: row.get(3)?,
                clone_of: row.get(4)?,
                is_bios: row.get(5)?,
                is_device: row.get(6)?,
                is_mechanical: row.get(7)?,
                is_runnable: row.get(8)?,
                driver_status: row.get(9)?,
                emulation_status: row.get(10)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let facets = facet_queries
        .iter()
        .map(|query| facet(connection, &request.filters, members, text.as_ref(), query))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(SearchResponse {
        items,
        total,
        page: page.page,
        page_size: page.page_size,
        page_count: total.div_ceil(u64::from(page.page_size)),
        facets,
        elapsed_ms: started.elapsed().as_secs_f64() * 1000.0,
    })
}

fn validate_filters(filters: &MachineFilters) -> Result<(), SearchError> {
    validate_values("manufacturers", &filters.manufacturers)?;
    validate_values("years", &filters.years)?;
    validate_values("driverStatuses", &filters.driver_statuses)?;
    validate_values("emulationStatuses", &filters.emulation_statuses)?;
    validate_values("softwareLists", &filters.software_lists)?;
    validate_values("genres", &filters.genres)?;
    validate_values("categories", &filters.categories)?;
    validate_values("series", &filters.series)?;
    validate_values("versionsAdded", &filters.versions_added)?;
    if let Some(range) = &filters.version_added_range {
        for bound in [&range.from, &range.to].into_iter().flatten() {
            if version::sort_key(bound).is_none() {
                return Err(SearchError::InvalidRequest(format!(
                    "'{bound}' is not a MAME version"
                )));
            }
        }
    }
    Ok(())
}

/// Classification filters: projection table, dimension and selected values.
fn membership_filters(filters: &MachineFilters) -> [(MachineFacet, &'static str, &[String]); 3] {
    [
        (MachineFacet::Genre, "genre", &filters.genres),
        (MachineFacet::Category, "category", &filters.categories),
        (MachineFacet::Series, "series", &filters.series),
    ]
}

/// The text search `FROM` with a fixed join order: full-text matches first,
/// then their machines (see `facet_query`).
const FTS_FIRST_FROM: &str = "machines_fts CROSS JOIN machines m ON m.id = machines_fts.rowid";

pub(super) fn from_clause(text: Option<&TextQuery>) -> &'static str {
    if text.is_some() {
        "machines m JOIN machines_fts ON machines_fts.rowid = m.id"
    } else {
        "machines m"
    }
}

/// Conditions for the text and all filters, except those of `excluded`
/// (used to count the values of that facet).
pub(super) fn conditions(
    filters: &MachineFilters,
    members: &[ListMembers],
    text: Option<&TextQuery>,
    excluded: Option<MachineFacet>,
) -> Conditions {
    let applies = |facet| excluded != Some(facet);
    let mut conditions = Conditions::default();
    for members in members {
        // Uncorrelated: the list is read once, then matched on the unique name.
        conditions.push(
            "m.name IN (SELECT value FROM json_each(?))",
            [Value::Text(members.json().to_owned())],
        );
    }
    if let Some(text) = text {
        conditions.push(
            "machines_fts MATCH ?",
            [Value::Text(text.match_expression.clone())],
        );
    }
    if applies(MachineFacet::Manufacturer) {
        conditions.push_values("m.manufacturer", &filters.manufacturers);
    }
    if applies(MachineFacet::Year) {
        conditions.push_values("m.year", &filters.years);
    }
    match filters.clones {
        Some(CloneFilter::Parents) => conditions.push("m.clone_of IS NULL", []),
        Some(CloneFilter::Clones) => conditions.push("m.clone_of IS NOT NULL", []),
        None => {}
    }
    if let Some(parent) = &filters.clone_of {
        conditions.push("m.clone_of = ?", [Value::Text(parent.clone())]);
    }
    conditions.push_flag("m.is_bios", filters.bios);
    conditions.push_flag("m.is_device", filters.device);
    conditions.push_flag("m.is_mechanical", filters.mechanical);
    conditions.push_flag("m.is_runnable", filters.runnable);
    if applies(MachineFacet::DriverStatus) {
        conditions.push_values("m.driver_status", &filters.driver_statuses);
    }
    if applies(MachineFacet::EmulationStatus) {
        conditions.push_values("m.driver_emulation", &filters.emulation_statuses);
    }
    conditions.push_exists(
        "SELECT 1 FROM machine_roms r WHERE r.machine_name = m.name",
        filters.has_roms,
    );
    conditions.push_exists(
        "SELECT 1 FROM machine_disks d WHERE d.machine_name = m.name",
        filters.has_disks,
    );
    if applies(MachineFacet::SoftwareList) && !filters.software_lists.is_empty() {
        conditions.push(
            &format!(
                // Uncorrelated: evaluated once, using the list name index.
                "m.name IN (SELECT l.machine_name FROM machine_software_lists l
                            WHERE l.list_name IN ({}))",
                placeholders(filters.software_lists.len())
            ),
            text_values(&filters.software_lists),
        );
    }
    // Classifications live in the catalog's projection (`cls`), keyed by
    // short name. Values missing from the projection match nothing, so a
    // criterion that cannot be resolved never widens the results.
    for (facet, dimension, values) in membership_filters(filters) {
        if applies(facet) && !values.is_empty() {
            conditions.push(
                &format!(
                    // Uncorrelated; category ancestors have their own rows,
                    // so a parent node selects its descendants by equality.
                    "m.name IN (SELECT c.machine_name FROM cls.memberships c
                                WHERE c.dimension = '{dimension}' AND c.value IN ({}))",
                    placeholders(values.len())
                ),
                text_values(values),
            );
        }
    }
    if applies(MachineFacet::VersionAdded) {
        let mut alternatives = Vec::new();
        let mut params = Vec::new();
        if !filters.versions_added.is_empty() {
            alternatives.push(format!(
                "a.value IN ({})",
                placeholders(filters.versions_added.len())
            ));
            params.extend(text_values(&filters.versions_added));
        }
        if let Some(range) = &filters.version_added_range
            && (range.from.is_some() || range.to.is_some())
        {
            let key = |bound: &Option<String>, open: i64| {
                bound.as_deref().and_then(version::sort_key).unwrap_or(open)
            };
            alternatives.push("a.sort_key BETWEEN ? AND ?".to_owned());
            params.push(Value::Integer(key(&range.from, i64::MIN)));
            params.push(Value::Integer(key(&range.to, i64::MAX)));
        }
        if !alternatives.is_empty() {
            conditions.push(
                &format!(
                    "m.name IN (SELECT a.machine_name FROM cls.attributes a
                                WHERE a.dimension = 'versionAdded' AND ({}))",
                    alternatives.join(" OR ")
                ),
                params,
            );
        }
    }
    conditions
}

/// Deterministic order: every order ends with the unique short name.
pub(super) fn order_by(
    sort: Option<MachineSort>,
    direction: SortDirection,
    text: Option<&TextQuery>,
) -> (String, Vec<Value>) {
    let descending = direction == SortDirection::Descending;
    let dir = if descending { " DESC" } else { "" };
    let sort = match (sort, text) {
        (Some(MachineSort::Relevance) | None, Some(text)) => {
            return (
                format!("(m.name = ?) DESC, {RANK}, m.name"),
                vec![Value::Text(text.exact_name.clone())],
            );
        }
        (Some(MachineSort::Relevance) | None, None) => MachineSort::Name,
        (Some(sort), _) => sort,
    };
    let order = match sort {
        MachineSort::Relevance | MachineSort::Name => format!("m.name{dir}"),
        MachineSort::Description => format!("m.description COLLATE NOCASE{dir}, m.name{dir}"),
        MachineSort::Year => format!(
            "{}, m.description COLLATE NOCASE, m.name",
            nulls_last("m.year", descending)
        ),
        MachineSort::Manufacturer => format!(
            "{}, m.description COLLATE NOCASE, m.name",
            nulls_last("m.manufacturer", descending)
        ),
    };
    (order, Vec::new())
}

pub(super) fn facet(
    connection: &Connection,
    filters: &MachineFilters,
    members: &[ListMembers],
    text: Option<&TextQuery>,
    query: &FacetQuery<MachineFacet>,
) -> Result<FacetResult<MachineFacet>, SearchError> {
    let (sql, params) = facet_query(filters, members, text, query);
    let (values, has_more) = facet_values(connection, &sql, &params, query.limit)?;
    Ok(FacetResult {
        facet: query.facet,
        values,
        has_more,
    })
}

/// SQL and parameters counting the values of one facet; they fetch one row
/// more than the limit.
pub(super) fn facet_query(
    filters: &MachineFilters,
    members: &[ListMembers],
    text: Option<&TextQuery>,
    query: &FacetQuery<MachineFacet>,
) -> (String, Vec<Value>) {
    let mut conditions = conditions(filters, members, text, Some(query.facet));
    // A machine may reference a software list more than once (different
    // tags); projection rows are unique per machine and value.
    let (column, join, counted) = match query.facet {
        MachineFacet::Manufacturer => ("m.manufacturer", None, "COUNT(*)"),
        MachineFacet::Year => ("m.year", None, "COUNT(*)"),
        MachineFacet::DriverStatus => ("m.driver_status", None, "COUNT(*)"),
        MachineFacet::EmulationStatus => ("m.driver_emulation", None, "COUNT(*)"),
        MachineFacet::SoftwareList => (
            "l.list_name",
            Some("machine_software_lists l ON l.machine_name = m.name"),
            "COUNT(DISTINCT m.id)",
        ),
        MachineFacet::Genre => (
            "c.value",
            Some("cls.memberships c ON c.dimension = 'genre' AND c.machine_name = m.name"),
            "COUNT(*)",
        ),
        MachineFacet::Category => (
            "c.value",
            Some("cls.memberships c ON c.dimension = 'category' AND c.machine_name = m.name"),
            "COUNT(*)",
        ),
        MachineFacet::Series => (
            "c.value",
            Some("cls.memberships c ON c.dimension = 'series' AND c.machine_name = m.name"),
            "COUNT(*)",
        ),
        MachineFacet::VersionAdded => (
            "a.value",
            Some("cls.attributes a ON a.dimension = 'versionAdded' AND a.machine_name = m.name"),
            "COUNT(*)",
        ),
    };
    conditions.push_contains(column, query.search.as_deref());
    // Years are listed chronologically, versions in MAME version order (not
    // as text), category nodes in path order (parents before children), other
    // facets by decreasing count.
    let order = match query.facet {
        MachineFacet::Year => nulls_last(column, false),
        MachineFacet::VersionAdded => {
            "MIN(a.sort_key) IS NULL, MIN(a.sort_key), a.value".to_owned()
        }
        MachineFacet::Category => "c.value".to_owned(),
        _ => format!("2 DESC, {}", nulls_last(column, false)),
    };
    // With search text, classification facets start from the full-text
    // index: otherwise SQLite may scan every projection row of the dimension
    // (already in value order, which saves the GROUP BY sort) and evaluate the
    // MATCH once per row, which takes seconds on a full catalog. SQLite never
    // reorders the tables of a CROSS JOIN, so the plan stays: FTS5 matches,
    // machines by rowid, projection rows by (dimension, machine_name).
    let classified = matches!(
        query.facet,
        MachineFacet::Genre
            | MachineFacet::Category
            | MachineFacet::Series
            | MachineFacet::VersionAdded
    );
    let (from, join) = match (join, text) {
        (Some(join), Some(_)) if classified => (FTS_FIRST_FROM, format!(" CROSS JOIN {join}")),
        (Some(join), _) => (from_clause(text), format!(" JOIN {join}")),
        (None, _) => (from_clause(text), String::new()),
    };
    let sql = format!(
        "SELECT {column}, {counted} FROM {from}{join}{} GROUP BY {column} ORDER BY {order} LIMIT ?",
        conditions.where_clause()
    );
    let mut params = conditions.params().to_vec();
    params.push(Value::Integer(i64::from(query.limit) + 1));
    (sql, params)
}
