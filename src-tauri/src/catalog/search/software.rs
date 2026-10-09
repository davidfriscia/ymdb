//! Software search.

use std::time::Instant;

use rusqlite::types::Value;
use rusqlite::{Connection, params_from_iter};
use serde::Serialize;

use super::request::{
    CloneFilter, FacetQuery, SoftwareFacet, SoftwareFilters, SoftwareSearchRequest, SoftwareSort,
    SortDirection, validate_facets, validate_page, validate_text, validate_values,
};
use super::sql::{Conditions, count, facet_values, nulls_last, placeholders, text_values};
use super::text::{self, TextQuery};
use super::{FacetResult, SearchError, SearchResponse};

/// Lightweight software data for result lists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SoftwareSummary {
    pub list_name: String,
    pub list_description: Option<String>,
    pub name: String,
    pub description: String,
    pub year: Option<String>,
    pub publisher: Option<String>,
    pub clone_of: Option<String>,
    /// Support status declared by MAME: `yes`, `partial` or `no`.
    pub supported: String,
}

pub type SoftwareSearchResponse = SearchResponse<SoftwareSummary, SoftwareFacet>;

/// Relevance: exact short name first, then bm25 with these column weights
/// (name, description, publisher, year, list name).
const RANK: &str = "bm25(software_fts, 10.0, 4.0, 2.0, 1.0, 1.0)";

// Uncorrelated subqueries: evaluated once instead of once per software.
const WITH_ROMS: &str = "SELECT p.software_id FROM software_parts p
    JOIN software_data_areas a ON a.part_id = p.id
    JOIN software_roms r ON r.data_area_id = a.id";
const WITH_DISKS: &str = "SELECT p.software_id FROM software_parts p
    JOIN software_disk_areas a ON a.part_id = p.id
    JOIN software_disks d ON d.disk_area_id = a.id";

pub fn search_software(
    connection: &Connection,
    request: &SoftwareSearchRequest,
) -> Result<SoftwareSearchResponse, SearchError> {
    let started = Instant::now();
    let page = validate_page(request.page, request.page_size)?;
    validate_text(request.text.as_deref())?;
    validate_filters(&request.filters)?;
    let facet_queries = validate_facets(&request.facets)?;
    let text = text::parse(request.text.as_deref())?;
    let imported: bool = connection.query_row(
        "SELECT EXISTS (SELECT 1 FROM software_catalog_metadata)",
        [],
        |row| row.get(0),
    )?;
    if !imported {
        return Err(SearchError::SoftwareListsNotImported);
    }

    let from = from_clause(text.as_ref());
    let conditions = conditions(&request.filters, text.as_ref(), None);
    let total = count(connection, from, &conditions)?;

    let (order, order_params) = order_by(request.sort, request.direction, text.as_ref());
    let sql = format!(
        "SELECT s.list_name, sl.description, s.name, s.description, s.year, s.publisher,
                s.clone_of, s.supported
         FROM {from} LEFT JOIN software_lists sl ON sl.name = s.list_name{}
         ORDER BY {order} LIMIT ? OFFSET ?",
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
            Ok(SoftwareSummary {
                list_name: row.get(0)?,
                list_description: row.get(1)?,
                name: row.get(2)?,
                description: row.get(3)?,
                year: row.get(4)?,
                publisher: row.get(5)?,
                clone_of: row.get(6)?,
                supported: row.get(7)?,
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

fn validate_filters(filters: &SoftwareFilters) -> Result<(), SearchError> {
    validate_values("lists", &filters.lists)?;
    validate_values("publishers", &filters.publishers)?;
    validate_values("years", &filters.years)?;
    validate_values("supported", &filters.supported)?;
    validate_values("interfaces", &filters.interfaces)
}

pub(super) fn from_clause(text: Option<&TextQuery>) -> &'static str {
    if text.is_some() {
        "software s JOIN software_fts ON software_fts.rowid = s.id"
    } else {
        "software s"
    }
}

/// Conditions for the text and all filters, except those of `excluded`
/// (used to count the values of that facet).
pub(super) fn conditions(
    filters: &SoftwareFilters,
    text: Option<&TextQuery>,
    excluded: Option<SoftwareFacet>,
) -> Conditions {
    let applies = |facet| excluded != Some(facet);
    let mut conditions = Conditions::default();
    if let Some(text) = text {
        conditions.push(
            "software_fts MATCH ?",
            [Value::Text(text.match_expression.clone())],
        );
    }
    if applies(SoftwareFacet::List) && !filters.lists.is_empty() {
        conditions.push(
            &format!("s.list_name IN ({})", placeholders(filters.lists.len())),
            text_values(&filters.lists),
        );
    }
    if applies(SoftwareFacet::Publisher) {
        conditions.push_values("s.publisher", &filters.publishers);
    }
    if applies(SoftwareFacet::Year) {
        conditions.push_values("s.year", &filters.years);
    }
    match filters.clones {
        Some(CloneFilter::Parents) => conditions.push("s.clone_of IS NULL", []),
        Some(CloneFilter::Clones) => conditions.push("s.clone_of IS NOT NULL", []),
        None => {}
    }
    if applies(SoftwareFacet::Supported) && !filters.supported.is_empty() {
        conditions.push(
            &format!("s.supported IN ({})", placeholders(filters.supported.len())),
            filters
                .supported
                .iter()
                .map(|status| Value::Text(status.as_str().to_owned())),
        );
    }
    conditions.push_in("s.id", WITH_ROMS, filters.has_roms);
    conditions.push_in("s.id", WITH_DISKS, filters.has_disks);
    if applies(SoftwareFacet::Interface) && !filters.interfaces.is_empty() {
        conditions.push(
            &format!(
                // Uncorrelated: evaluated once, using the interface index.
                "s.id IN (SELECT p.software_id FROM software_parts p
                          WHERE p.interface IN ({}))",
                placeholders(filters.interfaces.len())
            ),
            text_values(&filters.interfaces),
        );
    }
    conditions
}

/// Deterministic order: every order ends with the unique (list, name) pair.
pub(super) fn order_by(
    sort: Option<SoftwareSort>,
    direction: SortDirection,
    text: Option<&TextQuery>,
) -> (String, Vec<Value>) {
    let descending = direction == SortDirection::Descending;
    let dir = if descending { " DESC" } else { "" };
    let sort = match (sort, text) {
        (Some(SoftwareSort::Relevance) | None, Some(text)) => {
            return (
                format!("(s.name = ?) DESC, {RANK}, s.list_name, s.name"),
                vec![Value::Text(text.exact_name.clone())],
            );
        }
        (Some(SoftwareSort::Relevance) | None, None) => SoftwareSort::Name,
        (Some(sort), _) => sort,
    };
    let order = match sort {
        SoftwareSort::Relevance | SoftwareSort::Name => {
            format!("s.name{dir}, s.list_name{dir}")
        }
        SoftwareSort::Description => {
            format!("s.description COLLATE NOCASE{dir}, s.list_name{dir}, s.name{dir}")
        }
        SoftwareSort::Year => format!(
            "{}, s.description COLLATE NOCASE, s.list_name, s.name",
            nulls_last("s.year", descending)
        ),
        SoftwareSort::Publisher => format!(
            "{}, s.description COLLATE NOCASE, s.list_name, s.name",
            nulls_last("s.publisher", descending)
        ),
        SoftwareSort::List => format!("s.list_name{dir}, s.name{dir}"),
    };
    (order, Vec::new())
}

fn facet(
    connection: &Connection,
    filters: &SoftwareFilters,
    text: Option<&TextQuery>,
    query: &FacetQuery<SoftwareFacet>,
) -> Result<FacetResult<SoftwareFacet>, SearchError> {
    let mut conditions = conditions(filters, text, Some(query.facet));
    let (column, join) = match query.facet {
        SoftwareFacet::List => ("s.list_name", ""),
        SoftwareFacet::Publisher => ("s.publisher", ""),
        SoftwareFacet::Year => ("s.year", ""),
        SoftwareFacet::Supported => ("s.supported", ""),
        SoftwareFacet::Interface => (
            "p.interface",
            " JOIN software_parts p ON p.software_id = s.id",
        ),
    };
    conditions.push_contains(column, query.search.as_deref());
    // A software may have several parts with the same interface.
    let counted = if join.is_empty() {
        "COUNT(*)"
    } else {
        "COUNT(DISTINCT s.id)"
    };
    // Years are listed chronologically, other facets by decreasing count.
    let order = if query.facet == SoftwareFacet::Year {
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
