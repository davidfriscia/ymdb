//! Search tests on catalogs imported from small controlled fixtures.

use std::sync::atomic::AtomicBool;

use rusqlite::types::Value;
use rusqlite::{Connection, params_from_iter};
use tempfile::TempDir;

use super::machines::{MachineSearchResponse, facet_query, search_machines};
use super::request::{
    CloneFilter, FacetRequest, MachineFacet, MachineFilters, MachineSearchRequest, MachineSort,
    SoftwareFacet, SoftwareFilters, SoftwareSearchRequest, SoftwareSort, SortDirection,
    SupportStatus, validate_facets,
};
use super::software::{SoftwareSearchResponse, search_software};
use super::{FacetResult, FacetValue, SearchError, text as search_text};
use crate::catalog::classification;
use crate::catalog::import::{ImportProgress, import_from_reader, import_software_from_reader};
use crate::catalog::store::{CatalogSource, CatalogStore};

const MACHINES: &str = include_str!("../testdata/search-machines.xml");
const SOFTWARE: &str = include_str!("../testdata/listsoftware-sample.xml");

fn source() -> CatalogSource {
    CatalogSource {
        mame_release: "0.280".to_owned(),
        mame_revision: "mame0280".to_owned(),
        executable_path: "/opt/mame/mame".into(),
        executable_size: Some(1),
        executable_modified_at: Some(1),
    }
}

struct Catalog {
    _dir: TempDir,
    store: CatalogStore,
}

impl Catalog {
    fn new(machines: &str, software: Option<&str>) -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = CatalogStore::in_dir(dir.path());
        let cancel = AtomicBool::new(false);
        import_from_reader(
            machines.as_bytes(),
            &store,
            source(),
            &cancel,
            &ImportProgress::default(),
            || Ok(()),
        )
        .expect("machine import");
        if let Some(software) = software {
            import_software_from_reader(
                software.as_bytes(),
                &store,
                source(),
                &cancel,
                &ImportProgress::default(),
                || Ok(()),
            )
            .expect("software import");
        }
        Self { _dir: dir, store }
    }

    fn full() -> Self {
        Self::new(MACHINES, Some(SOFTWARE))
    }

    fn machines(
        &self,
        request: &MachineSearchRequest,
    ) -> Result<MachineSearchResponse, SearchError> {
        self.store
            .with_reader(|connection| Ok(search_machines(connection, request, &[])))
            .expect("store")
            .expect("catalog")
    }

    fn software(
        &self,
        request: &SoftwareSearchRequest,
    ) -> Result<SoftwareSearchResponse, SearchError> {
        self.store
            .with_reader(|connection| Ok(search_software(connection, request, &[])))
            .expect("store")
            .expect("catalog")
    }

    fn connection(&self) -> Connection {
        Connection::open(self.store.catalog_path()).expect("open")
    }
}

fn text(text: &str) -> MachineSearchRequest {
    MachineSearchRequest {
        text: Some(text.to_owned()),
        ..MachineSearchRequest::default()
    }
}

fn filtered(filters: MachineFilters) -> MachineSearchRequest {
    MachineSearchRequest {
        filters,
        ..MachineSearchRequest::default()
    }
}

fn names(response: &MachineSearchResponse) -> Vec<&str> {
    response.items.iter().map(|m| m.name.as_str()).collect()
}

fn software_names(response: &SoftwareSearchResponse) -> Vec<String> {
    response
        .items
        .iter()
        .map(|s| format!("{}/{}", s.list_name, s.name))
        .collect()
}

fn facet_values<F>(facet: &FacetResult<F>) -> Vec<(Option<&str>, u64)> {
    facet
        .values
        .iter()
        .map(|FacetValue { value, count }| (value.as_deref(), *count))
        .collect()
}

fn some(value: &str) -> Option<String> {
    Some(value.to_owned())
}

fn is_invalid<T: std::fmt::Debug>(result: Result<T, SearchError>) -> bool {
    matches!(result, Err(SearchError::InvalidRequest(_)))
}

// Text search

#[test]
fn empty_text_returns_everything_sorted_by_name() {
    let catalog = Catalog::full();

    let response = catalog.machines(&text("   ")).expect("search");

    assert_eq!(response.total, 13);
    assert_eq!(names(&response).first(), Some(&"attache"));
    assert_eq!(names(&response).last(), Some(&"z80"));
}

#[test]
fn exact_short_name_ranks_first() {
    let catalog = Catalog::full();

    let response = catalog.machines(&text("SF2")).expect("search");

    assert_eq!(names(&response), ["sf2", "sf2ce"]);
}

#[test]
fn partial_words_match_as_prefixes() {
    let catalog = Catalog::full();

    let response = catalog.machines(&text("pac")).expect("search");

    let mut found = names(&response);
    found.sort_unstable();
    assert_eq!(found, ["mspacman", "pacman"]);
}

#[test]
fn every_word_must_match_in_any_column() {
    let catalog = Catalog::full();

    assert_eq!(
        names(
            &catalog
                .machines(&text("street fighter champion"))
                .expect("search")
        ),
        ["sf2ce"]
    );
    let mut street = names(&catalog.machines(&text("street fight")).expect("search"))
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    street.sort();
    assert_eq!(street, ["sf2", "sf2ce"]);
    // Manufacturer and year are indexed too.
    assert_eq!(catalog.machines(&text("capcom")).expect("search").total, 3);
    assert_eq!(
        catalog
            .machines(&text("capcom 1992"))
            .expect("search")
            .total,
        1
    );
    assert_eq!(catalog.machines(&text("1980")).expect("search").total, 2);
}

#[test]
fn matches_unicode_text_without_case_or_diacritics() {
    let catalog = Catalog::full();

    assert_eq!(
        names(&catalog.machines(&text("ATTACHE")).expect("search")),
        ["attache"]
    );
    assert_eq!(
        names(&catalog.machines(&text("problemlosungs")).expect("search")),
        ["bup"]
    );
    assert_eq!(
        names(&catalog.machines(&text("ストリート")).expect("search")),
        ["sfiii"]
    );
}

#[test]
fn treats_special_characters_as_separators() {
    let catalog = Catalog::full();

    let quoted = catalog.machines(&text("\"pac*")).expect("search");
    let symbols = catalog.machines(&text("\"*:-()^")).expect("search");
    let operators = catalog.machines(&text("pac OR puck")).expect("search");

    assert_eq!(quoted.total, 2);
    assert_eq!(symbols.total, 13, "symbols alone do not restrict");
    // `OR` is a word, not an operator: no machine contains all three words.
    assert_eq!(operators.total, 0);
}

#[test]
fn user_input_cannot_alter_sql() {
    let catalog = Catalog::full();
    let hostile = "x') OR 1=1; DROP TABLE machines; --";

    let by_text = catalog.machines(&text(hostile)).expect("search");
    let by_filter = catalog
        .machines(&filtered(MachineFilters {
            manufacturers: vec![some(hostile)],
            software_lists: vec![hostile.to_owned()],
            ..MachineFilters::default()
        }))
        .expect("search");
    let by_facet = catalog
        .machines(&MachineSearchRequest {
            facets: vec![FacetRequest {
                facet: MachineFacet::Manufacturer,
                search: Some("%' OR '1'='1".to_owned()),
                limit: None,
            }],
            ..MachineSearchRequest::default()
        })
        .expect("search");

    assert_eq!(by_text.total, 0);
    assert_eq!(by_filter.total, 0);
    assert!(by_facet.facets[0].values.is_empty());
    let machines: i64 = catalog
        .connection()
        .query_row("SELECT COUNT(*) FROM machines", [], |row| row.get(0))
        .expect("count");
    assert_eq!(machines, 13);
}

// Filters

#[test]
fn combines_filters() {
    let catalog = Catalog::full();
    let search = |filters| names(&catalog.machines(&filtered(filters)).expect("search")).join(",");

    assert_eq!(
        search(MachineFilters {
            manufacturers: vec![some("Capcom")],
            clones: Some(CloneFilter::Parents),
            ..MachineFilters::default()
        }),
        "sf2,sfiii"
    );
    assert_eq!(
        search(MachineFilters {
            manufacturers: vec![some("Capcom"), some("Namco")],
            clones: Some(CloneFilter::Clones),
            ..MachineFilters::default()
        }),
        "sf2ce"
    );
    assert_eq!(
        search(MachineFilters {
            clone_of: Some("sf2".to_owned()),
            ..MachineFilters::default()
        }),
        "sf2ce"
    );
    assert_eq!(
        search(MachineFilters {
            clone_of: Some("missing".to_owned()),
            ..MachineFilters::default()
        }),
        ""
    );
    assert_eq!(
        search(MachineFilters {
            bios: Some(true),
            ..MachineFilters::default()
        }),
        "neogeo"
    );
    assert_eq!(
        search(MachineFilters {
            device: Some(true),
            ..MachineFilters::default()
        }),
        "z80"
    );
    assert_eq!(
        search(MachineFilters {
            mechanical: Some(true),
            ..MachineFilters::default()
        }),
        "fruitbank"
    );
    assert_eq!(
        search(MachineFilters {
            runnable: Some(false),
            ..MachineFilters::default()
        }),
        "z80"
    );
    assert_eq!(
        search(MachineFilters {
            driver_statuses: vec![some("imperfect")],
            emulation_statuses: vec![some("good")],
            ..MachineFilters::default()
        }),
        "bup,mspacman"
    );
    // `None` selects entries without a value.
    assert_eq!(
        search(MachineFilters {
            manufacturers: vec![None],
            years: vec![None],
            ..MachineFilters::default()
        }),
        "z80"
    );
    assert_eq!(
        search(MachineFilters {
            years: vec![some("199?")],
            ..MachineFilters::default()
        }),
        "bup,fruitbank"
    );
}

#[test]
fn filters_on_declared_roms_and_disks() {
    let catalog = Catalog::full();
    let search = |filters| names(&catalog.machines(&filtered(filters)).expect("search")).join(",");

    // A nodump ROM is still a ROM entry of the catalog.
    assert_eq!(
        search(MachineFilters {
            has_roms: Some(false),
            ..MachineFilters::default()
        }),
        "pong,z80"
    );
    assert_eq!(
        search(MachineFilters {
            has_disks: Some(true),
            ..MachineFilters::default()
        }),
        "sfiii"
    );
}

#[test]
fn filters_by_software_list_relations_including_missing_lists() {
    let catalog = Catalog::full();
    let search = |list: &str| {
        names(
            &catalog
                .machines(&filtered(MachineFilters {
                    software_lists: vec![list.to_owned()],
                    ..MachineFilters::default()
                }))
                .expect("search"),
        )
        .join(",")
    };

    assert_eq!(search("c64_cart"), "c64");
    // `vic10` and `attache_flop` are referenced but not provided by MAME.
    assert_eq!(search("vic10"), "c64");
    assert_eq!(search("attache_flop"), "attache");
    assert_eq!(search("nes"), "");
}

#[test]
fn text_and_filters_combine() {
    let catalog = Catalog::full();

    let response = catalog
        .machines(&MachineSearchRequest {
            text: Some("street".to_owned()),
            filters: MachineFilters {
                clones: Some(CloneFilter::Clones),
                ..MachineFilters::default()
            },
            ..MachineSearchRequest::default()
        })
        .expect("search");

    assert_eq!(names(&response), ["sf2ce"]);
}

// Facets

fn facet(facet: MachineFacet) -> FacetRequest<MachineFacet> {
    FacetRequest {
        facet,
        search: None,
        limit: None,
    }
}

#[test]
fn facet_counts_ignore_their_own_filter_and_apply_the_others() {
    let catalog = Catalog::full();

    let response = catalog
        .machines(&MachineSearchRequest {
            filters: MachineFilters {
                manufacturers: vec![some("Capcom")],
                ..MachineFilters::default()
            },
            facets: vec![facet(MachineFacet::Manufacturer), facet(MachineFacet::Year)],
            ..MachineSearchRequest::default()
        })
        .expect("search");

    assert_eq!(response.total, 3);
    // Manufacturer counts are computed without the manufacturer filter, so
    // other manufacturers remain selectable.
    let manufacturers = facet_values(&response.facets[0]);
    assert_eq!(manufacturers[0], (Some("Capcom"), 3));
    assert_eq!(manufacturers.len(), 11);
    assert!(manufacturers.contains(&(None, 1)));
    // Year counts apply the manufacturer filter, in chronological order.
    assert_eq!(
        facet_values(&response.facets[1]),
        [(Some("1991"), 1), (Some("1992"), 1), (Some("1997"), 1)]
    );
}

#[test]
fn facet_counts_follow_the_search_text() {
    let catalog = Catalog::full();

    let response = catalog
        .machines(&MachineSearchRequest {
            text: Some("pac".to_owned()),
            facets: vec![facet(MachineFacet::DriverStatus)],
            ..MachineSearchRequest::default()
        })
        .expect("search");

    assert_eq!(
        facet_values(&response.facets[0]),
        [(Some("good"), 1), (Some("imperfect"), 1)]
    );
}

#[test]
fn facets_support_limit_and_value_search() {
    let catalog = Catalog::full();

    let response = catalog
        .machines(&MachineSearchRequest {
            facets: vec![
                FacetRequest {
                    facet: MachineFacet::Manufacturer,
                    search: None,
                    limit: Some(2),
                },
                FacetRequest {
                    facet: MachineFacet::Year,
                    search: Some("19".to_owned()),
                    limit: Some(3),
                },
                FacetRequest {
                    facet: MachineFacet::SoftwareList,
                    search: Some("C64".to_owned()),
                    limit: None,
                },
            ],
            ..MachineSearchRequest::default()
        })
        .expect("search");

    assert_eq!(response.facets[0].values.len(), 2);
    assert!(response.facets[0].has_more);
    assert_eq!(
        response.facets[0].values[0],
        FacetValue {
            value: some("Capcom"),
            count: 3
        }
    );
    assert_eq!(
        facet_values(&response.facets[1]),
        [(Some("1972"), 1), (Some("1980"), 2), (Some("1981"), 1)]
    );
    assert!(response.facets[1].has_more);
    assert_eq!(facet_values(&response.facets[2]), [(Some("c64_cart"), 1)]);
    assert!(!response.facets[2].has_more);
}

// Sorting and pagination

#[test]
fn sorts_deterministically_with_nulls_last() {
    let catalog = Catalog::full();
    let sorted = |sort, direction| {
        names(
            &catalog
                .machines(&MachineSearchRequest {
                    sort: Some(sort),
                    direction,
                    page_size: Some(200),
                    ..MachineSearchRequest::default()
                })
                .expect("search"),
        )
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>()
    };

    let by_year = sorted(MachineSort::Year, SortDirection::Ascending);
    assert_eq!(&by_year[..4], ["pong", "pacman", "puckman", "mspacman"]);
    assert_eq!(by_year.last().map(String::as_str), Some("z80"));
    let by_year_desc = sorted(MachineSort::Year, SortDirection::Descending);
    // Ties (`199?`) are broken by description, ascending.
    assert_eq!(&by_year_desc[..2], ["fruitbank", "bup"]);
    assert_eq!(by_year_desc.last().map(String::as_str), Some("z80"));
    let by_manufacturer = sorted(MachineSort::Manufacturer, SortDirection::Ascending);
    assert_eq!(&by_manufacturer[..2], ["pong", "fruitbank"]);
    let by_description = sorted(MachineSort::Description, SortDirection::Ascending);
    assert_eq!(by_description.first().map(String::as_str), Some("attache"));
    let by_name_desc = sorted(MachineSort::Name, SortDirection::Descending);
    assert_eq!(by_name_desc.first().map(String::as_str), Some("z80"));
    // Relevance without text falls back to the name.
    assert_eq!(
        sorted(MachineSort::Relevance, SortDirection::Ascending),
        sorted(MachineSort::Name, SortDirection::Ascending)
    );
}

#[test]
fn pages_cover_all_results_once() {
    let catalog = Catalog::full();
    let page = |page| {
        catalog
            .machines(&MachineSearchRequest {
                page,
                page_size: Some(5),
                ..MachineSearchRequest::default()
            })
            .expect("search")
    };

    let pages: Vec<_> = (0..4).map(page).collect();

    assert_eq!(pages[0].page_count, 3);
    assert!(pages.iter().all(|p| p.total == 13));
    assert_eq!(pages[2].items.len(), 3);
    assert!(pages[3].items.is_empty(), "a page beyond the end is empty");
    let all: Vec<_> = pages.iter().flat_map(|p| names(p)).collect();
    let mut unique = all.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!((all.len(), unique.len()), (13, 13));
}

#[test]
fn rejects_invalid_parameters() {
    let catalog = Catalog::full();
    let with = |change: fn(&mut MachineSearchRequest)| {
        let mut request = MachineSearchRequest::default();
        change(&mut request);
        catalog.machines(&request)
    };

    assert!(is_invalid(with(|r| r.page_size = Some(0))));
    assert!(is_invalid(with(|r| r.page_size = Some(201))));
    assert!(is_invalid(with(|r| r.page = 100_001)));
    assert!(is_invalid(with(|r| r.text = Some("x".repeat(201)))));
    assert!(is_invalid(with(|r| r.text = Some(vec!["w"; 17].join(" ")))));
    assert!(is_invalid(with(|r| r.filters.years = vec![None; 101])));
    assert!(is_invalid(with(
        |r| r.facets = vec![facet(MachineFacet::Year); 2]
    )));
    assert!(is_invalid(with(|r| {
        r.facets = vec![FacetRequest {
            facet: MachineFacet::Year,
            search: None,
            limit: Some(0),
        }];
    })));
    assert!(with(|r| r.page_size = Some(200)).is_ok());
}

#[test]
fn deserializes_requests_and_rejects_unknown_fields() {
    let request: MachineSearchRequest = serde_json::from_value(serde_json::json!({
        "text": "pac",
        "filters": { "manufacturers": ["Namco", null], "clones": "parents", "hasRoms": true },
        "sort": "year",
        "direction": "descending",
        "pageSize": 20,
        "facets": [{ "facet": "driverStatus", "limit": 5 }]
    }))
    .expect("valid request");

    assert_eq!(request.filters.manufacturers, [some("Namco"), None]);
    assert_eq!(request.sort, Some(MachineSort::Year));
    assert!(
        serde_json::from_value::<MachineSearchRequest>(serde_json::json!({ "sql": "x" })).is_err()
    );
}

// Query plans

/// The full catalog with a small classification projection written
/// directly (projection builds are tested in `classification`), read
/// through a new store so that the projection is attached.
fn classified_catalog() -> (Catalog, CatalogStore) {
    let catalog = Catalog::full();
    let projection =
        classification::open_writer(&catalog.store.projection_path()).expect("projection");
    projection
        .execute_batch(
            "INSERT INTO memberships VALUES
                 ('genre', 'Maze', 'pacman', 1), ('genre', 'Maze', 'puckman', 1),
                 ('category', 'Maze', 'pacman', 0), ('category', 'Maze / Collect', 'pacman', 1),
                 ('series', 'Pac-Man', 'pacman', 1), ('series', 'Pac-Man', 'mspacman', 1);
             INSERT INTO attributes VALUES
                 ('versionAdded', 'pacman', '0.1', 1), ('versionAdded', 'sf2', '0.2', 2);",
        )
        .expect("projection rows");
    drop(projection);
    let store = CatalogStore::in_dir(
        catalog
            .store
            .catalog_path()
            .parent()
            .expect("catalog directory"),
    );
    (catalog, store)
}

/// The top-level loops of a query plan, outermost first (subqueries of
/// filters are left out).
fn plan_loops(connection: &Connection, sql: &str, params: &[Value]) -> Vec<String> {
    let mut statement = connection
        .prepare(&format!("EXPLAIN QUERY PLAN {sql}"))
        .expect("explain");
    statement
        .query_map(params_from_iter(params), |row| {
            Ok((row.get::<_, i64>(1)?, row.get::<_, String>(3)?))
        })
        .expect("plan")
        .map(|row| row.expect("plan row"))
        .filter(|(parent, detail)| {
            *parent == 0 && (detail.starts_with("SCAN") || detail.starts_with("SEARCH"))
        })
        .map(|(_, detail)| detail)
        .collect()
}

/// With search text, SQLite preferred to scan a dimension's projection rows
/// and evaluate the full-text MATCH once per row (seconds on a full catalog).
/// Classification facets must start from the full-text index, then look up
/// machines by rowid and projection rows by machine name.
#[test]
fn classification_facets_with_text_start_from_the_full_text_index() {
    let (_catalog, store) = classified_catalog();
    let filter_sets = [
        MachineFilters::default(),
        MachineFilters {
            genres: vec!["Maze".to_owned()],
            series: vec!["Pac-Man".to_owned()],
            manufacturers: vec![some("Namco")],
            ..MachineFilters::default()
        },
    ];
    let parsed = search_text::parse(Some("pac")).expect("text");
    store
        .with_reader(|connection| {
            for facet in [
                MachineFacet::Genre,
                MachineFacet::Category,
                MachineFacet::Series,
                MachineFacet::VersionAdded,
            ] {
                let table = if facet == MachineFacet::VersionAdded {
                    "SEARCH a "
                } else {
                    "SEARCH c "
                };
                for filters in &filter_sets {
                    let query = validate_facets(&[FacetRequest {
                        facet,
                        search: None,
                        limit: Some(1000),
                    }])
                    .expect("facet")
                    .remove(0);
                    let (sql, params) = facet_query(filters, &[], parsed.as_ref(), &query);
                    let loops = plan_loops(connection, &sql, &params);
                    let context = format!("{facet:?} {filters:?}: {loops:#?}");
                    assert_eq!(loops.len(), 3, "{context}");
                    // The MATCH runs once, without a per-row rowid constraint.
                    assert!(
                        loops[0].starts_with("SCAN machines_fts VIRTUAL TABLE")
                            && !loops[0].contains(":="),
                        "{context}"
                    );
                    assert!(
                        loops[1].starts_with("SEARCH m USING INTEGER PRIMARY KEY"),
                        "{context}"
                    );
                    assert!(
                        loops[2].starts_with(table) && loops[2].contains("machine_name=?"),
                        "{context}"
                    );
                }
            }
            Ok(())
        })
        .expect("store")
        .expect("catalog");

    // The search still sees the projection.
    let response = store
        .with_reader(|connection| {
            Ok(search_machines(
                connection,
                &MachineSearchRequest {
                    facets: vec![facet(MachineFacet::Genre)],
                    ..text("pac")
                },
                &[],
            ))
        })
        .expect("store")
        .expect("catalog")
        .expect("search");
    assert_eq!(facet_values(&response.facets[0]), [(Some("Maze"), 1)]);
}

// Index consistency and imports

fn check_fts_integrity(connection: &Connection) {
    connection
        .execute_batch(
            "INSERT INTO machines_fts(machines_fts, rank) VALUES ('integrity-check', 1);
             INSERT INTO software_fts(software_fts, rank) VALUES ('integrity-check', 1);",
        )
        .expect("FTS5 indexes match their content tables");
}

#[test]
fn search_indexes_stay_consistent_after_reimports() {
    let catalog = Catalog::full();
    check_fts_integrity(&catalog.connection());

    // Reimport machines without `pong`, then software lists without `nes`.
    let machines = MACHINES.replace("name=\"pong\"", "name=\"pong2\"");
    let start = SOFTWARE.find("\t<softwarelist name=\"nes\"").expect("nes");
    let end = SOFTWARE[start..].find("</softwarelist>").expect("end") + start + 16;
    let software = format!("{}{}", &SOFTWARE[..start], &SOFTWARE[end..]);
    let cancel = AtomicBool::new(false);
    import_from_reader(
        machines.as_bytes(),
        &catalog.store,
        source(),
        &cancel,
        &ImportProgress::default(),
        || Ok(()),
    )
    .expect("machine reimport");
    import_software_from_reader(
        software.as_bytes(),
        &catalog.store,
        source(),
        &cancel,
        &ImportProgress::default(),
        || Ok(()),
    )
    .expect("software reimport");

    check_fts_integrity(&catalog.connection());
    // `pong` was renamed: the prefix search now finds only the new entry.
    assert_eq!(
        names(&catalog.machines(&text("pong")).expect("search")),
        ["pong2"]
    );
    let smb = catalog
        .software(&SoftwareSearchRequest {
            text: Some("super mario".to_owned()),
            ..SoftwareSearchRequest::default()
        })
        .expect("search");
    assert_eq!(smb.total, 0);
}

#[test]
fn migration_indexes_an_existing_catalog() {
    let catalog = Catalog::full();
    // Turn the catalog into a version 2 catalog, without search indexes.
    catalog
        .connection()
        .execute_batch(
            "DROP TABLE machines_fts; DROP TABLE software_fts;
             DROP INDEX machines_description; DROP INDEX software_name;
             DROP INDEX software_description; PRAGMA user_version = 2;",
        )
        .expect("downgrade");

    let response = catalog
        .machines(&text("street"))
        .expect("search after upgrade");

    assert_eq!(response.total, 2);
    check_fts_integrity(&catalog.connection());
}

// Software

fn software_text(text: &str) -> SoftwareSearchRequest {
    SoftwareSearchRequest {
        text: Some(text.to_owned()),
        ..SoftwareSearchRequest::default()
    }
}

#[test]
fn searches_software_separately_from_machines() {
    let catalog = Catalog::full();

    let tetris = catalog.software(&software_text("tetris")).expect("search");

    // Both are exact name matches; their order is decided by bm25.
    let mut found = software_names(&tetris);
    found.sort();
    assert_eq!(found, ["c64_cart/tetris", "nes/tetris"]);
    let nes = tetris
        .items
        .iter()
        .find(|s| s.list_name == "nes")
        .expect("nes");
    assert_eq!(
        nes.list_description.as_deref(),
        Some("Nintendo Entertainment System cartridges")
    );
    assert_eq!(catalog.machines(&text("tetris")).expect("search").total, 0);
    assert_eq!(
        catalog
            .software(&software_text("pacman"))
            .expect("search")
            .total,
        0
    );
}

#[test]
fn software_text_matches_names_metadata_and_list_names() {
    let catalog = Catalog::full();

    assert_eq!(
        software_names(&catalog.software(&software_text("nes smb")).expect("search")),
        ["nes/smb"]
    );
    assert_eq!(
        software_names(
            &catalog
                .software(&software_text("origin ultima"))
                .expect("search")
        ),
        ["apple2_flop_orig/ultima4"]
    );
    assert_eq!(
        software_names(&catalog.software(&software_text("ys i ii")).expect("search")),
        ["pcecd/ys12"]
    );
    // Exact short name first.
    assert_eq!(
        software_names(
            &catalog
                .software(&software_text("gamebase"))
                .expect("search")
        )[0],
        "c64_cart/gamebase"
    );
}

#[test]
fn filters_software() {
    let catalog = Catalog::full();
    let search = |filters| {
        software_names(
            &catalog
                .software(&SoftwareSearchRequest {
                    filters,
                    ..SoftwareSearchRequest::default()
                })
                .expect("search"),
        )
        .join(",")
    };

    assert_eq!(
        search(SoftwareFilters {
            lists: vec!["nes".to_owned()],
            ..SoftwareFilters::default()
        }),
        "nes/smb,nes/tetris"
    );
    assert_eq!(
        search(SoftwareFilters {
            supported: vec![SupportStatus::Partial, SupportStatus::No],
            ..SoftwareFilters::default()
        }),
        "c64_cart/gameb2,c64_cart/tetris"
    );
    assert_eq!(
        search(SoftwareFilters {
            clones: Some(CloneFilter::Clones),
            ..SoftwareFilters::default()
        }),
        "c64_cart/gameb2"
    );
    assert_eq!(
        search(SoftwareFilters {
            lists: vec!["c64_cart".to_owned()],
            clone_of: Some("gamebase".to_owned()),
            ..SoftwareFilters::default()
        }),
        "c64_cart/gameb2"
    );
    assert_eq!(
        search(SoftwareFilters {
            lists: vec!["nes".to_owned()],
            clone_of: Some("gamebase".to_owned()),
            ..SoftwareFilters::default()
        }),
        ""
    );
    assert_eq!(
        search(SoftwareFilters {
            has_disks: Some(true),
            ..SoftwareFilters::default()
        }),
        "pcecd/ys12"
    );
    assert_eq!(
        search(SoftwareFilters {
            has_roms: Some(false),
            ..SoftwareFilters::default()
        }),
        "pcecd/ys12"
    );
    assert_eq!(
        search(SoftwareFilters {
            interfaces: vec!["floppy_5_25".to_owned()],
            publishers: vec![some("Origin Systems")],
            years: vec![some("1985")],
            ..SoftwareFilters::default()
        }),
        "apple2_flop_orig/ultima4"
    );
    assert_eq!(
        search(SoftwareFilters {
            lists: vec!["vic10".to_owned()],
            ..SoftwareFilters::default()
        }),
        "",
        "a list that is referenced but not provided has no software"
    );
}

#[test]
fn software_facets() {
    let catalog = Catalog::full();
    let request = |facet| FacetRequest {
        facet,
        search: None,
        limit: None,
    };

    let response = catalog
        .software(&SoftwareSearchRequest {
            filters: SoftwareFilters {
                supported: vec![SupportStatus::Yes],
                ..SoftwareFilters::default()
            },
            facets: vec![
                request(SoftwareFacet::List),
                request(SoftwareFacet::Supported),
                request(SoftwareFacet::Interface),
            ],
            sort: Some(SoftwareSort::List),
            ..SoftwareSearchRequest::default()
        })
        .expect("search");

    assert_eq!(response.total, 5);
    assert_eq!(
        facet_values(&response.facets[0]),
        [
            (Some("nes"), 2),
            (Some("apple2_flop_orig"), 1),
            (Some("c64_cart"), 1),
            (Some("pcecd"), 1)
        ]
    );
    // The support status facet ignores the support status filter.
    assert_eq!(
        facet_values(&response.facets[1]),
        [(Some("yes"), 5), (Some("no"), 1), (Some("partial"), 1)]
    );
    // Software are counted once per interface, even with several parts.
    assert_eq!(
        facet_values(&response.facets[2]),
        [
            (Some("nes_cart"), 2),
            (Some("c64_cart"), 1),
            (Some("floppy_5_25"), 1),
            (Some("pce_cdrom"), 1),
            (Some("pce_hdd"), 1)
        ]
    );
}

#[test]
fn software_search_requires_imported_software_lists() {
    let catalog = Catalog::new(MACHINES, None);

    let result = catalog.software(&SoftwareSearchRequest::default());

    assert!(matches!(result, Err(SearchError::SoftwareListsNotImported)));
    assert_eq!(
        catalog
            .machines(&MachineSearchRequest::default())
            .expect("search")
            .total,
        13
    );
}

#[test]
fn serializes_responses_to_frontend_contract() {
    let catalog = Catalog::full();
    let response = catalog
        .machines(&MachineSearchRequest {
            text: Some("pong".to_owned()),
            facets: vec![facet(MachineFacet::Year)],
            ..MachineSearchRequest::default()
        })
        .expect("search");

    let json = serde_json::to_value(&response).expect("serialize");

    assert_eq!(json["total"], 1);
    assert_eq!(json["pageSize"], 50);
    assert_eq!(json["pageCount"], 1);
    assert_eq!(json["items"][0]["name"], "pong");
    assert_eq!(json["items"][0]["isRunnable"], true);
    assert_eq!(json["items"][0]["emulationStatus"], "good");
    assert_eq!(json["facets"][0]["facet"], "year");
    assert_eq!(
        json["facets"][0]["values"][0],
        serde_json::json!({ "value": "1972", "count": 1 })
    );
    assert!(json["elapsedMs"].is_number());
}
