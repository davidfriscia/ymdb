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
use super::{FacetResult, SearchError, SearchResponse};

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
) -> Result<MachineSearchResponse, SearchError> {
    let started = Instant::now();
    let page = validate_page(request.page, request.page_size)?;
    validate_text(request.text.as_deref())?;
    validate_filters(&request.filters)?;
    let facet_queries = validate_facets(&request.facets)?;
    let text = text::parse(request.text.as_deref())?;

    let from = from_clause(text.as_ref());
    let conditions = conditions(&request.filters, text.as_ref(), None);
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
        .map(|query| facet(connection, &request.filters, text.as_ref(), query))
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
    validate_values("softwareLists", &filters.software_lists)
}

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
    text: Option<&TextQuery>,
    excluded: Option<MachineFacet>,
) -> Conditions {
    let applies = |facet| excluded != Some(facet);
    let mut conditions = Conditions::default();
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

fn facet(
    connection: &Connection,
    filters: &MachineFilters,
    text: Option<&TextQuery>,
    query: &FacetQuery<MachineFacet>,
) -> Result<FacetResult<MachineFacet>, SearchError> {
    let mut conditions = conditions(filters, text, Some(query.facet));
    let (column, join) = match query.facet {
        MachineFacet::Manufacturer => ("m.manufacturer", ""),
        MachineFacet::Year => ("m.year", ""),
        MachineFacet::DriverStatus => ("m.driver_status", ""),
        MachineFacet::EmulationStatus => ("m.driver_emulation", ""),
        MachineFacet::SoftwareList => (
            "l.list_name",
            " JOIN machine_software_lists l ON l.machine_name = m.name",
        ),
    };
    conditions.push_contains(column, query.search.as_deref());
    // A machine may reference a list more than once (different tags).
    let counted = if join.is_empty() {
        "COUNT(*)"
    } else {
        "COUNT(DISTINCT m.id)"
    };
    // Years are listed chronologically, other facets by decreasing count.
    let order = if query.facet == MachineFacet::Year {
        nulls_last(column, false)
    } else {
        format!("2 DESC, {}", nulls_last(column, false))
    };
    let sql = format!(
        "SELECT {column}, {counted} FROM {}{join}{} GROUP BY {column} ORDER BY {order} LIMIT ?",
        from_clause(text),
        conditions.where_clause()
    );
    let mut params = conditions.params().to_vec();
    params.push(Value::Integer(i64::from(query.limit) + 1));
    let (values, has_more) = facet_values(connection, &sql, &params, query.limit)?;
    Ok(FacetResult {
        facet: query.facet,
        values,
        has_more,
    })
}
