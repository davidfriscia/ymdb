//! Projection lifecycle and classification searches, on the search fixture
//! catalog (13 machines) and the support file fixtures.

use std::sync::atomic::AtomicBool;

use tempfile::TempDir;

use super::*;
use crate::catalog::import::{ImportProgress, import_from_reader};
use crate::catalog::search::machines::{MachineSearchResponse, search_machines};
use crate::catalog::search::request::{
    CloneFilter, FacetRequest, MachineFacet, MachineFilters, MachineSearchRequest, MachineSort,
    SortDirection, VersionRange,
};
use crate::catalog::search::{FacetValue, ListMembers, SearchError};
use crate::catalog::store::{CatalogSource, CatalogStore};
use crate::support_files::repository::SupportRepository;
use crate::support_files::tests::fixture as parsed;

const MACHINES: &str = include_str!("../testdata/search-machines.xml");
const OTHER_MACHINES: &str = include_str!("../testdata/listxml-sample.xml");

#[test]
fn encodes_paths_as_sqlite_uris() {
    assert_eq!(
        sqlite_uri(Path::new("/home/me/My Data #1/c?.sqlite"), "ro").expect("uri"),
        "file:///home/me/My%20Data%20%231/c%3F.sqlite?mode=ro"
    );
}

#[cfg(windows)]
#[test]
fn encodes_windows_paths_as_sqlite_uris() {
    assert_eq!(
        sqlite_uri(Path::new(r"C:\Users\me ✓\catalog.sqlite"), "ro").expect("uri"),
        "file:///C:/Users/me%20%E2%9C%93/catalog.sqlite?mode=ro"
    );
}

struct Fixture {
    _dir: TempDir,
    root: PathBuf,
    repository: SupportRepository,
}

impl Fixture {
    /// A repository (in a directory with spaces and non-ASCII characters)
    /// with the given sources imported.
    fn new(formats: &[SupportFileFormat]) -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = dir.path().join("data ✓ #1");
        let repository = SupportRepository::in_dir(&root);
        for &format in formats {
            repository
                .import(format.file_name(), &parsed(format), 1)
                .expect("import");
        }
        Self {
            _dir: dir,
            root,
            repository,
        }
    }

    fn catalog(&self, name: &str, machines: &str) -> CatalogStore {
        let store = CatalogStore::in_dir(&self.root.join(name));
        import_catalog(&store, machines, 1);
        store
    }

    fn target(&self) -> ProjectionTarget {
        ProjectionTarget::new(
            self.repository.path(),
            &self.repository.snapshot().expect("snapshot"),
        )
    }

    fn sync(&self, store: &CatalogStore) -> SyncOutcome {
        store.sync_classifications(&self.target()).expect("sync")
    }
}

fn import_catalog(store: &CatalogStore, machines: &str, modified_at: u64) {
    import_from_reader(
        machines.as_bytes(),
        store,
        CatalogSource {
            mame_release: "0.280".to_owned(),
            mame_revision: "mame0280".to_owned(),
            executable_path: "/opt/mame/mame".into(),
            executable_size: Some(1),
            executable_modified_at: Some(modified_at),
        },
        &AtomicBool::new(false),
        &ImportProgress::default(),
        || Ok(()),
    )
    .expect("catalog import");
}

fn read<T>(store: &CatalogStore, query: impl FnOnce(&Connection) -> rusqlite::Result<T>) -> T {
    store
        .with_reader(|connection| Ok(query(connection)?))
        .expect("read")
        .expect("catalog")
}

fn dimensions(store: &CatalogStore) -> Vec<(Dimension, SupportFileFormat, u64)> {
    read(store, classified_dimensions)
        .into_iter()
        .map(|entry| (entry.dimension, entry.source, entry.machine_count))
        .collect()
}

fn machine(store: &CatalogStore, name: &str) -> MachineClassifications {
    read(store, |connection| {
        machine_classifications(connection, name)
    })
}

const ALL: [SupportFileFormat; 3] = [
    SupportFileFormat::Catver,
    SupportFileFormat::Genre,
    SupportFileFormat::Series,
];

// Projection lifecycle

#[test]
fn first_access_projects_every_dimension_then_nothing_until_a_change() {
    let fixture = Fixture::new(&ALL);
    let store = fixture.catalog("catalog", MACHINES);

    assert_eq!(
        fixture.sync(&store),
        SyncOutcome::Synced(Dimension::ALL.to_vec())
    );
    assert_eq!(fixture.sync(&store), SyncOutcome::UpToDate);
    // `galaga` is in the support files but not in the catalog: not projected.
    assert_eq!(
        dimensions(&store),
        [
            (Dimension::Genre, SupportFileFormat::Genre, 10),
            (Dimension::Category, SupportFileFormat::Catver, 12),
            (Dimension::Series, SupportFileFormat::Series, 7),
            (Dimension::VersionAdded, SupportFileFormat::Catver, 12),
        ]
    );
    assert_eq!(
        machine(&store, "sf2"),
        MachineClassifications {
            genres: vec!["Fighter".to_owned()],
            categories: vec!["Fighter / Versus".to_owned()],
            series: vec!["Street Fighter".to_owned(), "Street Fighter II".to_owned()],
            version_added: Some("0.33rc1".to_owned()),
        }
    );
    assert_eq!(
        machine(&store, "unknown"),
        MachineClassifications::default()
    );
}

#[test]
fn only_changed_dimensions_are_rebuilt() {
    let fixture = Fixture::new(&ALL);
    let store = fixture.catalog("catalog", MACHINES);
    fixture.sync(&store);

    // Identical reimport: nothing to do.
    fixture
        .repository
        .import("series.ini", &parsed(SupportFileFormat::Series), 2)
        .expect("reimport");
    assert_eq!(fixture.sync(&store), SyncOutcome::UpToDate);

    let mut series = parsed(SupportFileFormat::Series);
    series
        .memberships
        .retain(|entry| entry.value != "Street Fighter II");
    fixture
        .repository
        .import("series.ini", &series, 3)
        .expect("reimport");
    assert_eq!(
        fixture.sync(&store),
        SyncOutcome::Synced(vec![Dimension::Series])
    );
    assert_eq!(machine(&store, "sf2").series, ["Street Fighter"]);
}

#[test]
fn source_changes_and_removals_are_projected() {
    let fixture = Fixture::new(&[SupportFileFormat::Catver, SupportFileFormat::Catlist]);
    let store = fixture.catalog("catalog", MACHINES);
    fixture.sync(&store);
    assert_eq!(machine(&store, "pacman").categories, ["Maze / Collect"]);

    fixture
        .repository
        .set_active(Dimension::Category, Some(SupportFileFormat::Catlist))
        .expect("switch");
    assert_eq!(
        fixture.sync(&store),
        SyncOutcome::Synced(vec![Dimension::Category])
    );
    // catlist.ini levels, `Arcade: ` included.
    assert_eq!(
        machine(&store, "pacman").categories,
        ["Arcade / Maze / Collect"]
    );

    fixture
        .repository
        .remove(SupportFileFormat::Catlist)
        .expect("remove");
    assert_eq!(
        fixture.sync(&store),
        SyncOutcome::Synced(vec![Dimension::Category])
    );
    assert!(machine(&store, "pacman").categories.is_empty());
    // No orphaned rows remain for the removed dimension.
    let rows: i64 = read(&store, |connection| {
        connection.query_row(
            "SELECT COUNT(*) FROM cls.memberships WHERE dimension = 'category'",
            [],
            |row| row.get(0),
        )
    });
    assert_eq!(rows, 0);
    assert_eq!(
        dimensions(&store),
        [(Dimension::VersionAdded, SupportFileFormat::Catver, 12)]
    );
}

#[test]
fn every_catalog_gets_its_own_projection_and_every_change() {
    let fixture = Fixture::new(&[SupportFileFormat::Series]);
    let first = fixture.catalog("first", MACHINES);
    let second = fixture.catalog("second", OTHER_MACHINES);
    fixture.sync(&first);
    fixture.sync(&second);
    assert_eq!(
        machine(&first, "sf2").series,
        ["Street Fighter", "Street Fighter II"]
    );
    // The second catalog has no `sf2`, but has `pacman`.
    assert!(machine(&second, "sf2").series.is_empty());
    assert_eq!(machine(&second, "pacman").series, ["Pac-Man"]);

    fixture
        .repository
        .remove(SupportFileFormat::Series)
        .expect("remove");
    for store in [&first, &second] {
        assert_eq!(
            fixture.sync(store),
            SyncOutcome::Synced(vec![Dimension::Series])
        );
        assert!(dimensions(store).is_empty());
    }
}

#[test]
fn a_catalog_reimport_rebuilds_the_projection_without_a_repository_change() {
    let fixture = Fixture::new(&ALL);
    let store = fixture.catalog("catalog", OTHER_MACHINES);
    fixture.sync(&store);
    assert!(machine(&store, "sf2").genres.is_empty());

    // The same repository revisions, but another catalog import.
    import_catalog(&store, MACHINES, 2);
    assert_eq!(
        fixture.sync(&store),
        SyncOutcome::Synced(Dimension::ALL.to_vec())
    );
    assert_eq!(machine(&store, "sf2").genres, ["Fighter"]);
}

#[test]
fn a_failed_sync_keeps_the_previous_projection_and_retries() {
    let fixture = Fixture::new(&[SupportFileFormat::Genre, SupportFileFormat::Series]);
    let store = fixture.catalog("catalog", MACHINES);
    fixture.sync(&store);
    let before = dimensions(&store);

    // Both dimensions change; the second write fails inside the transaction.
    fixture
        .repository
        .remove(SupportFileFormat::Genre)
        .expect("remove");
    fixture
        .repository
        .remove(SupportFileFormat::Series)
        .expect("remove");
    let projection = store.projection_path();
    let connection = Connection::open(&projection).expect("open");
    connection
        .execute_batch(
            "CREATE TRIGGER fail BEFORE INSERT ON projection_state
             WHEN NEW.dimension = 'series' BEGIN SELECT RAISE(ABORT, 'disk full'); END",
        )
        .expect("trigger");
    let failed = store.sync_classifications(&fixture.target());
    assert!(failed.is_err());
    // Nothing was committed: the genre projection and its revision are unchanged.
    assert_eq!(dimensions(&store), before);
    assert_eq!(machine(&store, "pacman").genres, ["Maze"]);

    connection.execute_batch("DROP TRIGGER fail").expect("drop");
    assert_eq!(
        fixture.sync(&store),
        SyncOutcome::Synced(vec![Dimension::Genre, Dimension::Series])
    );
    assert!(dimensions(&store).is_empty());
}

#[test]
fn an_unreadable_repository_fails_the_sync_but_not_the_catalog() {
    let fixture = Fixture::new(&[SupportFileFormat::Genre]);
    let store = fixture.catalog("catalog", MACHINES);
    let mut target = fixture.target();
    target.repository_path = fixture.root.join("missing.sqlite");
    assert!(store.sync_classifications(&target).is_err());
    // Searches without classification filters still work.
    let response = search(&store, &MachineSearchRequest::default()).expect("search");
    assert_eq!(response.total, 13);
    // And the next access with the right repository succeeds.
    assert!(matches!(fixture.sync(&store), SyncOutcome::Synced(_)));
}

#[test]
fn a_store_without_catalog_has_nothing_to_sync() {
    let fixture = Fixture::new(&ALL);
    let store = CatalogStore::in_dir(&fixture.root.join("empty"));
    assert_eq!(fixture.sync(&store), SyncOutcome::NoCatalog);
}

#[test]
fn deleting_the_catalog_files_deletes_the_projection() {
    let fixture = Fixture::new(&ALL);
    let store = fixture.catalog("catalog", MACHINES);
    fixture.sync(&store);
    assert!(store.projection_path().is_file());
    store.delete_files().expect("delete");
    assert!(!store.projection_path().exists());
    // The global repository is not touched.
    assert_eq!(fixture.repository.view().expect("view").sources.len(), 3);
}

// Searches

fn search(
    store: &CatalogStore,
    request: &MachineSearchRequest,
) -> Result<MachineSearchResponse, SearchError> {
    search_with(store, request, &[])
}

fn search_with(
    store: &CatalogStore,
    request: &MachineSearchRequest,
    members: &[ListMembers],
) -> Result<MachineSearchResponse, SearchError> {
    store
        .with_reader(|connection| Ok(search_machines(connection, request, members)))
        .expect("store")
        .expect("catalog")
}

fn names(response: &MachineSearchResponse) -> Vec<&str> {
    response
        .items
        .iter()
        .map(|item| item.name.as_str())
        .collect()
}

fn filtered(filters: MachineFilters) -> MachineSearchRequest {
    MachineSearchRequest {
        filters,
        ..MachineSearchRequest::default()
    }
}

fn classified() -> (Fixture, CatalogStore) {
    let fixture = Fixture::new(&ALL);
    let store = fixture.catalog("catalog", MACHINES);
    fixture.sync(&store);
    (fixture, store)
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

#[test]
fn filters_by_genre_category_series_and_version() {
    let (_fixture, store) = classified();

    let genre = search(
        &store,
        &filtered(MachineFilters {
            genres: strings(&["Maze"]),
            ..MachineFilters::default()
        }),
    )
    .expect("search");
    assert_eq!(names(&genre), ["mspacman", "pacman", "puckman"]);

    // A parent node selects its descendants.
    let category = search(
        &store,
        &filtered(MachineFilters {
            categories: strings(&["Fighter"]),
            ..MachineFilters::default()
        }),
    )
    .expect("search");
    assert_eq!(names(&category), ["sf2", "sf2ce", "sfiii"]);
    let leaf = search(
        &store,
        &filtered(MachineFilters {
            categories: strings(&["Fighter / Versus * Mature *"]),
            ..MachineFilters::default()
        }),
    )
    .expect("search");
    assert_eq!(names(&leaf), ["sfiii"]);

    let series = search(
        &store,
        &filtered(MachineFilters {
            series: strings(&["Street Fighter II"]),
            ..MachineFilters::default()
        }),
    )
    .expect("search");
    assert_eq!(names(&series), ["sf2", "sf2ce"]);

    let version = search(
        &store,
        &filtered(MachineFilters {
            versions_added: strings(&["0.10"]),
            ..MachineFilters::default()
        }),
    )
    .expect("search");
    assert_eq!(names(&version), ["mspacman"]);
}

#[test]
fn version_ranges_follow_mame_version_order() {
    let (_fixture, store) = classified();
    let range = |from: Option<&str>, to: Option<&str>| {
        search(
            &store,
            &filtered(MachineFilters {
                version_added_range: Some(VersionRange {
                    from: from.map(str::to_owned),
                    to: to.map(str::to_owned),
                }),
                ..MachineFilters::default()
            }),
        )
        .expect("search")
    };
    // 0.1 pacman, 0.2 puckman, 0.10 mspacman, 0.33b1 neogeo, 0.33rc1 sf2,
    // 0.34b3 sf2ce: as text, "0.10" would sort before "0.2".
    assert_eq!(
        names(&range(Some("0.2"), Some("0.33rc1"))),
        ["mspacman", "neogeo", "puckman", "sf2"]
    );
    assert_eq!(names(&range(None, Some("0.2"))), ["pacman", "puckman"]);
    assert_eq!(names(&range(Some("0.200"), None)), ["z80"]);
    // Points and range of one dimension are alternatives.
    let either = search(
        &store,
        &filtered(MachineFilters {
            versions_added: strings(&["0.142"]),
            version_added_range: Some(VersionRange {
                from: Some("0.200".to_owned()),
                to: None,
            }),
            ..MachineFilters::default()
        }),
    )
    .expect("search");
    assert_eq!(names(&either), ["attache", "z80"]);

    let invalid = search(
        &store,
        &filtered(MachineFilters {
            version_added_range: Some(VersionRange {
                from: Some("latest".to_owned()),
                to: None,
            }),
            ..MachineFilters::default()
        }),
    );
    assert!(matches!(invalid, Err(SearchError::InvalidRequest(_))));
}

#[test]
fn combines_values_with_or_and_dimensions_with_and() {
    let (_fixture, store) = classified();
    let request = filtered(MachineFilters {
        genres: strings(&["Maze", "Fighter"]),
        series: strings(&["Pac-Man", "Street Fighter II"]),
        categories: strings(&["Maze", "Fighter / Versus"]),
        clones: Some(CloneFilter::Clones),
        ..MachineFilters::default()
    });
    let response = search(&store, &request).expect("search");
    assert_eq!(names(&response), ["mspacman", "pacman", "sf2ce"]);

    // With the search text and a user list.
    let text = MachineSearchRequest {
        text: Some("pac".to_owned()),
        sort: Some(MachineSort::Name),
        ..request.clone()
    };
    assert_eq!(
        names(&search(&store, &text).expect("search")),
        ["mspacman", "pacman"]
    );
    let mut favorites = request;
    favorites.filters.user_lists = vec!["favorites".to_owned()];
    let members = ListMembers::machines(["pacman", "sf2ce", "galaga"]);
    let response = search_with(&store, &favorites, &[members]).expect("search");
    assert_eq!(names(&response), ["pacman", "sf2ce"]);
}

#[test]
fn unresolvable_values_match_nothing() {
    let (fixture, store) = classified();
    let missing = filtered(MachineFilters {
        series: strings(&["194X"]),
        ..MachineFilters::default()
    });
    assert_eq!(search(&store, &missing).expect("search").total, 0);
    // One missing value among others does not widen or break the selection.
    let some = filtered(MachineFilters {
        series: strings(&["194X", "Pong"]),
        ..MachineFilters::default()
    });
    assert_eq!(names(&search(&store, &some).expect("search")), ["pong"]);

    // A removed source: its filters match nothing instead of everything.
    fixture
        .repository
        .remove(SupportFileFormat::Series)
        .expect("remove");
    fixture.sync(&store);
    assert_eq!(search(&store, &some).expect("search").total, 0);
}

fn facet(response: &MachineSearchResponse, facet: MachineFacet) -> Vec<(String, u64)> {
    response
        .facets
        .iter()
        .find(|entry| entry.facet == facet)
        .expect("facet")
        .values
        .iter()
        .map(|FacetValue { value, count }| (value.clone().unwrap_or_default(), *count))
        .collect()
}

fn facets(filters: MachineFilters) -> MachineSearchRequest {
    MachineSearchRequest {
        filters,
        facets: [
            MachineFacet::Genre,
            MachineFacet::Category,
            MachineFacet::Series,
            MachineFacet::VersionAdded,
        ]
        .into_iter()
        .map(|facet| FacetRequest {
            facet,
            search: None,
            limit: Some(1000),
        })
        .collect(),
        ..MachineSearchRequest::default()
    }
}

#[test]
fn facets_count_classifications_disjunctively() {
    let (_fixture, store) = classified();
    let response = search(
        &store,
        &facets(MachineFilters {
            genres: strings(&["Maze"]),
            ..MachineFilters::default()
        }),
    )
    .expect("search");

    // Genre ignores its own selection, so alternatives stay visible.
    let genres = facet(&response, MachineFacet::Genre);
    assert_eq!(genres[0], ("Fighter".to_owned(), 3));
    assert!(genres.contains(&("Maze".to_owned(), 3)));
    // The other facets count only the matching machines.
    assert_eq!(
        facet(&response, MachineFacet::Series),
        [("Pac-Man".to_owned(), 3)]
    );
    assert_eq!(
        facet(&response, MachineFacet::Category),
        [("Maze".to_owned(), 3), ("Maze / Collect".to_owned(), 3)]
    );
    // Versions in MAME version order.
    assert_eq!(
        facet(&response, MachineFacet::VersionAdded),
        [
            ("0.1".to_owned(), 1),
            ("0.2".to_owned(), 1),
            ("0.10".to_owned(), 1)
        ]
    );

    let all = search(&store, &facets(MachineFilters::default())).expect("search");
    let categories = facet(&all, MachineFacet::Category);
    // Parents before children, each machine counted once per node.
    let fighter = categories
        .iter()
        .position(|(value, _)| value == "Fighter")
        .expect("parent");
    assert_eq!(categories[fighter], ("Fighter".to_owned(), 3));
    assert_eq!(categories[fighter + 1], ("Fighter / Versus".to_owned(), 2));
    assert_eq!(
        categories[fighter + 2],
        ("Fighter / Versus * Mature *".to_owned(), 1)
    );
    let versions: Vec<String> = facet(&all, MachineFacet::VersionAdded)
        .into_iter()
        .map(|(value, _)| value)
        .collect();
    assert_eq!(
        versions,
        [
            "0.1", "0.2", "0.10", "0.33b1", "0.33rc1", "0.34b3", "0.70u1", "0.110", "0.125",
            "0.142", "0.150", "0.200"
        ]
    );
    // Machines in two series are counted in both.
    let series = facet(&all, MachineFacet::Series);
    assert!(series.contains(&("Street Fighter".to_owned(), 3)));
    assert!(series.contains(&("Street Fighter II".to_owned(), 2)));
}

/// Facet counts computed without the facet SQL: the machines matching the
/// text and every other filter, grouped by their projection rows.
fn expected_facet(
    store: &CatalogStore,
    request: &MachineSearchRequest,
    facet: MachineFacet,
) -> Vec<(String, u64)> {
    let mut filters = request.filters.clone();
    let (table, dimension) = match facet {
        MachineFacet::Genre => {
            filters.genres.clear();
            ("memberships", "genre")
        }
        MachineFacet::Category => {
            filters.categories.clear();
            ("memberships", "category")
        }
        MachineFacet::Series => {
            filters.series.clear();
            ("memberships", "series")
        }
        _ => {
            filters.versions_added.clear();
            filters.version_added_range = None;
            ("attributes", "versionAdded")
        }
    };
    let matching = search(
        store,
        &MachineSearchRequest {
            text: request.text.clone(),
            filters,
            page_size: Some(200),
            ..MachineSearchRequest::default()
        },
    )
    .expect("search");
    let names: Vec<&str> = names(&matching);
    let rows: Vec<(String, String)> = read(store, |connection| {
        connection
            .prepare(&format!(
                "SELECT value, machine_name FROM cls.{table} WHERE dimension = ?1"
            ))?
            .query_map([dimension], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect()
    });
    let mut counts = std::collections::BTreeMap::<String, u64>::new();
    for (value, machine) in rows {
        if names.contains(&machine.as_str()) {
            *counts.entry(value).or_default() += 1;
        }
    }
    counts.into_iter().collect()
}

#[test]
fn classification_facets_with_text_count_the_matching_machines() {
    let (_fixture, store) = classified();
    let filter_sets = [
        MachineFilters::default(),
        MachineFilters {
            genres: strings(&["Maze", "Fighter"]),
            ..MachineFilters::default()
        },
        MachineFilters {
            categories: strings(&["Fighter"]),
            series: strings(&["Street Fighter"]),
            ..MachineFilters::default()
        },
        MachineFilters {
            version_added_range: Some(VersionRange {
                from: Some("0.10".to_owned()),
                to: None,
            }),
            manufacturers: vec![Some("Capcom".to_owned()), None],
            ..MachineFilters::default()
        },
    ];
    let mut counted = 0;
    for text in ["pac", "s", "street fighter", "zzzzunlikely"] {
        for filters in &filter_sets {
            let request = MachineSearchRequest {
                text: Some(text.to_owned()),
                ..facets(filters.clone())
            };
            let response = search(&store, &request).expect("search");
            for kind in [
                MachineFacet::Genre,
                MachineFacet::Category,
                MachineFacet::Series,
                MachineFacet::VersionAdded,
            ] {
                let mut actual = facet(&response, kind);
                actual.sort();
                counted += actual.len();
                assert_eq!(
                    actual,
                    expected_facet(&store, &request, kind),
                    "{kind:?}, text '{text}', {filters:?}"
                );
            }
        }
    }
    assert!(counted > 20, "the cases count classified machines");
}

#[test]
fn facet_value_search_and_limits_apply_to_classifications() {
    let (_fixture, store) = classified();
    let mut request = facets(MachineFilters::default());
    request.facets[2].search = Some("fighter".to_owned());
    request.facets[2].limit = Some(1);
    let response = search(&store, &request).expect("search");
    let series = &response.facets[2];
    assert_eq!(series.values.len(), 1);
    assert!(series.has_more);
}

#[test]
fn classification_filters_keep_sorting_counts_and_pagination() {
    let (_fixture, store) = classified();
    let request = MachineSearchRequest {
        filters: MachineFilters {
            categories: strings(&["Maze", "Fighter"]),
            ..MachineFilters::default()
        },
        sort: Some(MachineSort::Name),
        direction: SortDirection::Descending,
        page: 1,
        page_size: Some(4),
        ..MachineSearchRequest::default()
    };
    let response = search(&store, &request).expect("search");
    assert_eq!(response.total, 6);
    assert_eq!(response.page_count, 2);
    assert_eq!(names(&response), ["pacman", "mspacman"]);
}

#[test]
fn search_plans_use_the_projection_indexes() {
    let (_fixture, store) = classified();
    let plan: Vec<String> = read(&store, |connection| {
        let mut statement = connection.prepare(
            "EXPLAIN QUERY PLAN SELECT COUNT(*) FROM machines m
             WHERE m.name IN (SELECT c.machine_name FROM cls.memberships c
                              WHERE c.dimension = 'series' AND c.value IN ('Pong'))",
        )?;
        statement
            .query_map([], |row| row.get::<_, String>(3))?
            .collect()
    });
    let plan = plan.join("\n");
    assert!(plan.contains("USING PRIMARY KEY"), "{plan}");
}

/// Imports the real support files into a temporary repository and projects
/// them into copies of real catalogs, measuring each step. Run with:
/// `YMDB_SUPPORT_FILES=<pS_SupportFiles_288> YMDB_BENCH_CATALOGS=<a.sqlite;b.sqlite>
/// cargo test --release --lib real_catalog_classifications -- --ignored --nocapture`
#[test]
#[ignore = "needs real catalogs and support files"]
fn real_catalog_classifications() {
    use std::time::Instant;

    let files = PathBuf::from(std::env::var("YMDB_SUPPORT_FILES").expect("YMDB_SUPPORT_FILES"));
    let catalogs = std::env::var("YMDB_BENCH_CATALOGS").expect("YMDB_BENCH_CATALOGS");
    let dir = tempfile::tempdir().expect("temp dir");
    let repository = SupportRepository::in_dir(dir.path());
    for path in [
        "CatVer/catver.ini",
        "CatVer/genre.ini",
        "CatVer/catlist.ini",
        "Version/version.ini",
        "Series/series.ini",
    ] {
        let started = Instant::now();
        let parsed = crate::support_files::parser::read_file(&files.join(path)).expect(path);
        let parse_ms = started.elapsed().as_secs_f64() * 1000.0;
        let started = Instant::now();
        repository.import(path, &parsed, 1).expect("import");
        println!(
            "import {path}: parse {parse_ms:.0} ms, store {:.0} ms",
            started.elapsed().as_secs_f64() * 1000.0
        );
    }
    let started = Instant::now();
    repository
        .import(
            "CatVer/catver.ini",
            &crate::support_files::parser::read_file(&files.join("CatVer/catver.ini"))
                .expect("catver"),
            2,
        )
        .expect("reimport");
    println!(
        "identical reimport of catver.ini: {:.0} ms",
        started.elapsed().as_secs_f64() * 1000.0
    );
    let target =
        ProjectionTarget::new(repository.path(), &repository.snapshot().expect("snapshot"));

    for (index, source) in catalogs.split(';').enumerate() {
        let catalog_dir = dir.path().join(format!("catalog-{index}"));
        std::fs::create_dir_all(&catalog_dir).expect("dir");
        // A copy: the real catalog is never opened for writing.
        std::fs::copy(source, catalog_dir.join("catalog.sqlite")).expect("copy catalog");
        let store = CatalogStore::in_dir(&catalog_dir);

        let started = Instant::now();
        let outcome = store.sync_classifications(&target).expect("sync");
        println!(
            "\n{source}\nfirst sync {outcome:?}: {:.0} ms",
            started.elapsed().as_secs_f64() * 1000.0
        );
        let started = Instant::now();
        assert_eq!(
            store.sync_classifications(&target).expect("sync"),
            SyncOutcome::UpToDate
        );
        println!(
            "unchanged sync: {:.2} ms",
            started.elapsed().as_secs_f64() * 1000.0
        );
        for entry in read(&store, classified_dimensions) {
            println!(
                "  {:?}: {} machines, {} values ({:?})",
                entry.dimension, entry.machine_count, entry.value_count, entry.source
            );
        }
        println!("  1942: {:?}", machine(&store, "1942"));

        let all_facets = facets(MachineFilters::default());
        let cases = [
            ("4 classification facets, no filter", all_facets.clone()),
            (
                "genre Shooter",
                filtered(MachineFilters {
                    genres: strings(&["Shooter"]),
                    ..MachineFilters::default()
                }),
            ),
            (
                "category node Shooter (descendants)",
                filtered(MachineFilters {
                    categories: strings(&["Shooter"]),
                    ..MachineFilters::default()
                }),
            ),
            (
                "series 194X",
                filtered(MachineFilters {
                    series: strings(&["194X"]),
                    ..MachineFilters::default()
                }),
            ),
            (
                "version 0.200..0.280",
                filtered(MachineFilters {
                    version_added_range: Some(VersionRange {
                        from: Some("0.200".to_owned()),
                        to: Some("0.280".to_owned()),
                    }),
                    ..MachineFilters::default()
                }),
            ),
            (
                "genre + category + series + text",
                MachineSearchRequest {
                    text: Some("1942".to_owned()),
                    ..filtered(MachineFilters {
                        genres: strings(&["Shooter"]),
                        categories: strings(&["Shooter / Flying Vertical"]),
                        series: strings(&["194X"]),
                        ..MachineFilters::default()
                    })
                },
            ),
            (
                "4 facets with genre Shooter",
                facets(MachineFilters {
                    genres: strings(&["Shooter"]),
                    ..MachineFilters::default()
                }),
            ),
        ];
        for (label, request) in cases {
            let mut times = Vec::new();
            let mut total = 0;
            for _ in 0..7 {
                let started = Instant::now();
                total = search(&store, &request).expect(label).total;
                times.push(started.elapsed().as_secs_f64() * 1000.0);
            }
            times.sort_by(f64::total_cmp);
            println!(
                "  {label}: {total} results, first {:.1} ms, median {:.1} ms",
                times[0].max(times[6]),
                times[3]
            );
        }
        for single in &all_facets.facets {
            let request = MachineSearchRequest {
                facets: vec![single.clone()],
                page_size: Some(1),
                ..MachineSearchRequest::default()
            };
            search(&store, &request).expect("facet");
            let started = Instant::now();
            search(&store, &request).expect("facet");
            println!(
                "  facet {:?} alone: {:.1} ms",
                single.facet,
                started.elapsed().as_secs_f64() * 1000.0
            );
        }
        let plan: Vec<String> = read(&store, |connection| {
            let mut statement = connection.prepare(
                "EXPLAIN QUERY PLAN SELECT c.value, COUNT(*) FROM machines m JOIN cls.memberships c ON c.dimension = 'category' AND c.machine_name = m.name GROUP BY c.value ORDER BY c.value",
            )?;
            statement
                .query_map([], |row| row.get::<_, String>(3))?
                .collect()
        });
        println!("  category facet plan: {plan:?}");
        let facets_response = search(&store, &all_facets).expect("facets");
        for result in &facets_response.facets {
            println!(
                "  facet {:?}: {} values{}",
                result.facet,
                result.values.len(),
                if result.has_more { "+" } else { "" }
            );
        }
    }
}

// Custom lists combined with the other criteria

/// Every result of `request`, page after page, by name.
fn all_names(
    store: &CatalogStore,
    request: &MachineSearchRequest,
    members: &[ListMembers],
) -> Vec<String> {
    let mut names = Vec::new();
    for page in 0.. {
        let response = search_with(
            store,
            &MachineSearchRequest {
                page,
                page_size: Some(2),
                sort: Some(MachineSort::Name),
                facets: Vec::new(),
                ..request.clone()
            },
            members,
        )
        .expect("search");
        if response.items.is_empty() {
            assert_eq!(names.len() as u64, response.total, "pages cover the total");
            break;
        }
        names.extend(response.items.into_iter().map(|item| item.name));
    }
    names
}

#[test]
fn lists_restrict_results_counts_pages_and_facets_like_a_reference() {
    let (_fixture, store) = classified();
    // A custom list with a machine of no catalog, and Favorites.
    let custom = ["pacman", "mspacman", "sf2ce", "pong", "notinthiscatalog"];
    let favorites = ["pacman", "sf2ce", "galaga", "puckman"];
    let combinations: [(&[&str], &[&[&str]]); 4] = [
        (&["custom"], &[&custom]),
        (&["favorites", "custom"], &[&favorites, &custom]),
        (&["empty"], &[&[]]),
        (&["custom", "empty"], &[&custom, &[]]),
    ];
    let requests = [
        MachineFilters::default(),
        MachineFilters {
            genres: strings(&["Maze", "Fighter"]),
            ..MachineFilters::default()
        },
        MachineFilters {
            categories: strings(&["Maze"]),
            ..MachineFilters::default()
        },
        MachineFilters {
            series: strings(&["Pac-Man", "Street Fighter II"]),
            ..MachineFilters::default()
        },
        MachineFilters {
            version_added_range: Some(VersionRange {
                from: Some("0.1".to_owned()),
                to: None,
            }),
            ..MachineFilters::default()
        },
    ];
    let facet_kinds = [
        MachineFacet::Genre,
        MachineFacet::Category,
        MachineFacet::Series,
        MachineFacet::VersionAdded,
        MachineFacet::Manufacturer,
    ];
    for text in [None, Some("pac"), Some("fighter")] {
        for filters in &requests {
            let unrestricted = MachineSearchRequest {
                text: text.map(str::to_owned),
                filters: filters.clone(),
                ..MachineSearchRequest::default()
            };
            let everything = all_names(&store, &unrestricted, &[]);
            for (ids, lists) in &combinations {
                let members: Vec<ListMembers> = lists
                    .iter()
                    .map(|names| ListMembers::machines(names.iter().copied()))
                    .collect();
                let restricted = MachineSearchRequest {
                    filters: MachineFilters {
                        user_lists: strings(ids),
                        ..filters.clone()
                    },
                    ..unrestricted.clone()
                };
                // Reference: the unrestricted results that are in every list.
                let expected: Vec<String> = everything
                    .iter()
                    .filter(|name| lists.iter().all(|list| list.contains(&name.as_str())))
                    .cloned()
                    .collect();
                let context = format!("{text:?} {filters:?} {ids:?}");
                assert_eq!(
                    all_names(&store, &restricted, &members),
                    expected,
                    "{context}"
                );

                // Facets count only list members: each value's count is the
                // total of the restricted search selecting that value.
                let response = search_with(
                    &store,
                    &MachineSearchRequest {
                        page_size: Some(1),
                        facets: facet_kinds
                            .iter()
                            .map(|&facet| FacetRequest {
                                facet,
                                search: None,
                                limit: Some(1000),
                            })
                            .collect(),
                        ..restricted.clone()
                    },
                    &members,
                )
                .expect("facets");
                assert_eq!(response.total, expected.len() as u64, "{context}");
                for result in &response.facets {
                    for value in &result.values {
                        let Some(selected) = value.value.clone() else {
                            continue;
                        };
                        let mut single = restricted.filters.clone();
                        match result.facet {
                            MachineFacet::Genre => single.genres = vec![selected],
                            MachineFacet::Category => single.categories = vec![selected],
                            MachineFacet::Series => single.series = vec![selected],
                            MachineFacet::VersionAdded => {
                                single.version_added_range = None;
                                single.versions_added = vec![selected];
                            }
                            MachineFacet::Manufacturer => {
                                single.manufacturers = vec![Some(selected)];
                            }
                            _ => unreachable!(),
                        }
                        let count = search_with(
                            &store,
                            &MachineSearchRequest {
                                filters: single,
                                page_size: Some(1),
                                ..restricted.clone()
                            },
                            &members,
                        )
                        .expect("count")
                        .total;
                        assert_eq!(value.count, count, "{context} {:?} {value:?}", result.facet);
                    }
                }
            }
        }
    }
}

#[test]
fn an_empty_list_matches_nothing_and_unresolved_lists_are_refused() {
    let (_fixture, store) = classified();
    let request = filtered(MachineFilters {
        user_lists: strings(&["empty"]),
        ..MachineFilters::default()
    });
    let empty = ListMembers::machines([]);
    assert_eq!(
        search_with(&store, &request, &[empty])
            .expect("search")
            .total,
        0
    );
    // A list named in the request must be resolved by the caller.
    assert!(matches!(
        search_with(&store, &request, &[]),
        Err(SearchError::InvalidRequest(_))
    ));
    let too_many = filtered(MachineFilters {
        user_lists: strings(&["a", "b", "c", "d", "e"]),
        ..MachineFilters::default()
    });
    let members = vec![ListMembers::machines(["pacman"]); 5];
    assert!(matches!(
        search_with(&store, &too_many, &members),
        Err(SearchError::InvalidRequest(_))
    ));
}
