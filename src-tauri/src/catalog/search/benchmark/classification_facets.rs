//! Benchmark of the machine facets with a classification projection.
//!
//! Run with:
//! `YMDB_BENCH_CATALOG=/path/to/catalog.sqlite
//! YMDB_BENCH_CLASSIFICATIONS=/path/to/classifications.sqlite cargo test --release
//! --lib classification_facet_benchmark -- --ignored --nocapture`
//!
//! Use copies of the files: the projection is attached read-only, but other
//! tests open it through a store. `YMDB_BENCH_TEXTS` (comma separated)
//! replaces the default search texts.
//!
//! - `classification_facet_benchmark`: the time of each stage of a machine
//!   search (results, count, every facet, the browser's facet request) and the
//!   query plans of the classification facets.
//! - `classification_facet_snapshot`: writes the results, counts and facets of
//!   a set of requests to `YMDB_BENCH_SNAPSHOT`, to compare two versions of the
//!   queries on a real catalog.
//! - `classification_typing_benchmark`: progressive typing through the store
//!   lock, as the browser sends a results and a facet request per search.

use std::fmt::Write as _;
use std::path::Path;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use rusqlite::types::Value;
use rusqlite::{Connection, OpenFlags, params_from_iter};

use super::super::machines::{self, MachineSearchResponse};
use super::super::request::{
    FacetRequest, MachineFacet, MachineFilters, MachineSearchRequest, VersionRange, validate_facets,
};
use super::super::{ListMembers, sql, text};
use super::{machine_members, millis};
use crate::catalog::classification;
use crate::catalog::store::CatalogStore;

/// Repetitions of fast stages; slower stages are repeated less (see `sample`).
const RUNS: usize = 9;

const TEXTS: &[&str] = &[
    "gal",
    "g",
    "ga",
    "gala",
    "galaga",
    "pac",
    "pacman",
    "mario",
    "1942",
    "zzzzunlikely",
    "the",
    "s",
];

/// Texts of the snapshot: no text, prefixes of several sizes, no result.
const SNAPSHOT_TEXTS: &[&str] = &["", "gal", "ga", "galaga", "pac", "1942", "zzzzunlikely"];

/// Texts whose classification facet plans are printed.
const EXPLAIN_TEXTS: &[&str] = &["gal", "ga", "g", "galaga", "zzzzunlikely"];

const CLASSIFICATION_FACETS: [MachineFacet; 4] = [
    MachineFacet::Genre,
    MachineFacet::Category,
    MachineFacet::Series,
    MachineFacet::VersionAdded,
];

const OTHER_FACETS: [MachineFacet; 5] = [
    MachineFacet::Manufacturer,
    MachineFacet::Year,
    MachineFacet::DriverStatus,
    MachineFacet::EmulationStatus,
    MachineFacet::SoftwareList,
];

/// The facets of the browser's facet request (`buildFacetRequest`) with
/// their default limits: trees and version ranges get every value.
fn browser_facets() -> Vec<FacetRequest<MachineFacet>> {
    [
        (MachineFacet::Genre, 50),
        (MachineFacet::Category, 1000),
        (MachineFacet::Series, 50),
        (MachineFacet::VersionAdded, 1000),
        (MachineFacet::Manufacturer, 50),
        (MachineFacet::Year, 50),
        (MachineFacet::DriverStatus, 50),
        (MachineFacet::EmulationStatus, 50),
        (MachineFacet::SoftwareList, 50),
    ]
    .into_iter()
    .map(|(facet, limit)| FacetRequest {
        facet,
        search: None,
        limit: Some(limit),
    })
    .collect()
}

fn browser_facet(facet: MachineFacet) -> FacetRequest<MachineFacet> {
    browser_facets()
        .into_iter()
        .find(|request| request.facet == facet)
        .expect("browser facet")
}

fn env_path(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} must be set"))
}

/// A read-only catalog connection with the projection attached as `cls`,
/// as the store opens it.
fn open_classified(catalog: &str, classifications: &str) -> Connection {
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY
        | OpenFlags::SQLITE_OPEN_NO_MUTEX
        | OpenFlags::SQLITE_OPEN_URI;
    let connection = Connection::open_with_flags(catalog, flags).expect("open catalog");
    classification::attach(&connection, Path::new(classifications)).expect("attach projection");
    connection
}

fn texts() -> Vec<String> {
    match std::env::var("YMDB_BENCH_TEXTS") {
        Ok(texts) => texts.split(',').map(str::to_owned).collect(),
        Err(_) => TEXTS.iter().map(|text| (*text).to_owned()).collect(),
    }
}

fn scalar(connection: &Connection, sql: &str) -> String {
    connection
        .query_row(sql, [], |row| row.get::<_, Value>(0))
        .map(|value| match value {
            Value::Integer(value) => value.to_string(),
            Value::Text(value) => value,
            other => format!("{other:?}"),
        })
        .expect(sql)
}

fn print_environment(connection: &Connection, catalog: &str, classifications: &str) {
    let size = |path: &str| std::fs::metadata(path).map_or(0, |metadata| metadata.len());
    println!(
        "sqlite_version(): {}",
        scalar(connection, "SELECT sqlite_version()")
    );
    println!("catalog: {catalog} ({} bytes)", size(catalog));
    println!(
        "classifications: {classifications} ({} bytes)",
        size(classifications)
    );
    println!(
        "machines: {}",
        scalar(connection, "SELECT COUNT(*) FROM machines")
    );
    let mut statement = connection
        .prepare(
            "SELECT 'memberships ' || dimension, COUNT(*), COUNT(DISTINCT value)
             FROM cls.memberships GROUP BY dimension
             UNION ALL
             SELECT 'attributes ' || dimension, COUNT(*), COUNT(DISTINCT value)
             FROM cls.attributes GROUP BY dimension",
        )
        .expect("counts");
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })
        .expect("counts")
        .collect::<Result<Vec<_>, _>>()
        .expect("rows");
    for (table, count, values) in rows {
        println!("{table}: {count} rows, {values} values");
    }
}

/// Median, maximum and number of samples, in milliseconds. Stages slower
/// than 1 s run once, stages slower than 100 ms three times.
struct Stats {
    median: f64,
    max: f64,
    samples: usize,
}

fn sample(mut run: impl FnMut()) -> Stats {
    let measure = |run: &mut dyn FnMut()| {
        let started = Instant::now();
        run();
        millis(started.elapsed())
    };
    let first = measure(&mut run);
    let runs = if first > 1000.0 {
        1
    } else if first > 100.0 {
        3
    } else {
        RUNS
    };
    let mut times = vec![first];
    times.extend((1..runs).map(|_| measure(&mut run)));
    times.sort_by(f64::total_cmp);
    Stats {
        median: times[times.len() / 2],
        max: times[times.len() - 1],
        samples: times.len(),
    }
}

fn text_request(text: &str) -> MachineSearchRequest {
    MachineSearchRequest {
        text: (!text.is_empty()).then(|| text.to_owned()),
        ..MachineSearchRequest::default()
    }
}

/// The browser's facet request: one result and every facet.
fn facet_request(text: &str) -> MachineSearchRequest {
    MachineSearchRequest {
        page_size: Some(1),
        facets: browser_facets(),
        ..text_request(text)
    }
}

/// A measured stage: its label and how to run it for a text.
type Stage = (String, Box<dyn Fn(&Connection, &str)>);

fn run_facet(connection: &Connection, filters: &MachineFilters, text: &str, facet: MachineFacet) {
    let parsed = text::parse(Some(text)).expect("text");
    let query = validate_facets(&[browser_facet(facet)])
        .expect("facet")
        .remove(0);
    machines::facet(connection, filters, None, parsed.as_ref(), &query).expect("facet");
}

#[test]
#[ignore = "requires a real catalog and projection in YMDB_BENCH_CATALOG and YMDB_BENCH_CLASSIFICATIONS"]
fn classification_facet_benchmark() {
    let catalog = env_path("YMDB_BENCH_CATALOG");
    let classifications = env_path("YMDB_BENCH_CLASSIFICATIONS");
    let connection = open_classified(&catalog, &classifications);
    print_environment(&connection, &catalog, &classifications);

    let filters = MachineFilters::default();
    let mut stages: Vec<Stage> = vec![
        (
            "search".to_owned(),
            Box::new(|connection, text| {
                machines::search_machines(connection, &text_request(text), None).expect("search");
            }),
        ),
        (
            "count".to_owned(),
            Box::new(|connection, text| {
                let parsed = text::parse(Some(text)).expect("text");
                let conditions =
                    machines::conditions(&MachineFilters::default(), None, parsed.as_ref(), None);
                sql::count(
                    connection,
                    machines::from_clause(parsed.as_ref()),
                    &conditions,
                )
                .expect("count");
            }),
        ),
    ];
    for facet in CLASSIFICATION_FACETS {
        let filters = filters.clone();
        stages.push((
            format!("{facet:?}"),
            Box::new(move |connection, text| run_facet(connection, &filters, text, facet)),
        ));
    }
    stages.push((
        "other facets".to_owned(),
        Box::new(|connection, text| {
            for facet in OTHER_FACETS {
                run_facet(connection, &MachineFilters::default(), text, facet);
            }
        }),
    ));
    stages.push((
        "facet request".to_owned(),
        Box::new(|connection, text| {
            machines::search_machines(connection, &facet_request(text), None).expect("facets");
        }),
    ));

    println!(
        "\n{:<14} {:<14} {:>8} {:>12} {:>12} {:>4}",
        "text", "stage", "results", "median ms", "max ms", "n"
    );
    let mut texts = vec![String::new()];
    texts.extend(self::texts());
    for text in &texts {
        // The first execution of a text on a new connection (empty SQLite
        // page cache; the operating system cache may be warm).
        let cold = open_classified(&catalog, &classifications);
        let started = Instant::now();
        let total = machines::search_machines(&cold, &facet_request(text), None)
            .expect("facets")
            .total;
        let first = millis(started.elapsed());
        drop(cold);
        let label = if text.is_empty() { "(no text)" } else { text };
        println!(
            "{label:<14} {:<14} {total:>8} {first:>12.2} {:>12} {:>4}",
            "first request", "", 1
        );
        for (stage, run) in &stages {
            let stats = sample(|| run(&connection, text));
            println!(
                "{label:<14} {stage:<14} {total:>8} {:>12.2} {:>12.2} {:>4}",
                stats.median, stats.max, stats.samples
            );
        }
    }

    println!("\nQuery plans of the classification facets:");
    for text in EXPLAIN_TEXTS {
        let parsed = text::parse(Some(text)).expect("text");
        for facet in CLASSIFICATION_FACETS {
            let query = validate_facets(&[browser_facet(facet)])
                .expect("facet")
                .remove(0);
            let (sql, params) = machines::facet_query(&filters, None, parsed.as_ref(), &query);
            if *text == EXPLAIN_TEXTS[0] {
                println!("\n{facet:?} SQL: {sql}\n  params: {params:?}");
            }
            println!(
                "\n{facet:?}, text '{text}':\n{}",
                plan_tree(&connection, &sql, &params)
            );
        }
    }
}

/// `EXPLAIN QUERY PLAN` as an indented tree.
fn plan_tree(connection: &Connection, sql: &str, params: &[Value]) -> String {
    let mut statement = connection
        .prepare(&format!("EXPLAIN QUERY PLAN {sql}"))
        .expect("explain");
    let rows: Vec<(i64, i64, String)> = statement
        .query_map(params_from_iter(params), |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(3)?))
        })
        .expect("plan")
        .collect::<Result<_, _>>()
        .expect("rows");
    let mut depths = std::collections::HashMap::new();
    let mut tree = String::new();
    for (id, parent, detail) in rows {
        let depth = depths.get(&parent).map_or(0, |depth| depth + 1);
        depths.insert(id, depth);
        let _ = writeln!(tree, "  {}{detail}", "  ".repeat(depth));
    }
    tree
}

// Snapshot

/// The first values of a classification dimension by decreasing count.
fn top_values(connection: &Connection, table: &str, dimension: &str, count: usize) -> Vec<String> {
    connection
        .prepare(&format!(
            "SELECT value FROM cls.{table} WHERE dimension = ?1
             GROUP BY value ORDER BY COUNT(*) DESC, value LIMIT {count}"
        ))
        .expect("values")
        .query_map([dimension], |row| row.get(0))
        .expect("values")
        .collect::<Result<_, _>>()
        .expect("rows")
}

fn snapshot_filters(connection: &Connection) -> Vec<(&'static str, MachineFilters)> {
    let genres = top_values(connection, "memberships", "genre", 2);
    let series = top_values(connection, "memberships", "series", 2);
    let versions = top_values(connection, "attributes", "versionAdded", 1);
    // A parent node (its descendants are selected too) and a leaf.
    let category_parent: String = connection
        .query_row(
            "SELECT value FROM cls.memberships WHERE dimension = 'category' AND direct = 0
             GROUP BY value ORDER BY COUNT(*) DESC, value LIMIT 1",
            [],
            |row| row.get(0),
        )
        .expect("category");
    let category_leaf: String = connection
        .query_row(
            "SELECT value FROM cls.memberships
             WHERE dimension = 'category' AND value LIKE '% / %'
             GROUP BY value ORDER BY COUNT(*) DESC, value LIMIT 1",
            [],
            |row| row.get(0),
        )
        .expect("category");
    let none = MachineFilters::default();
    vec![
        ("no filter", none.clone()),
        (
            "genre",
            MachineFilters {
                genres: genres[..1].to_vec(),
                ..none.clone()
            },
        ),
        (
            "two genres (OR)",
            MachineFilters {
                genres: genres.clone(),
                ..none.clone()
            },
        ),
        (
            "category parent node",
            MachineFilters {
                categories: vec![category_parent.clone()],
                ..none.clone()
            },
        ),
        (
            "category leaf",
            MachineFilters {
                categories: vec![category_leaf],
                ..none.clone()
            },
        ),
        (
            "two series (OR)",
            MachineFilters {
                series: series.clone(),
                ..none.clone()
            },
        ),
        (
            "version added value",
            MachineFilters {
                versions_added: versions,
                ..none.clone()
            },
        ),
        (
            "version added range",
            MachineFilters {
                version_added_range: Some(VersionRange {
                    from: Some("0.100".to_owned()),
                    to: Some("0.200".to_owned()),
                }),
                ..none.clone()
            },
        ),
        (
            "open version range",
            MachineFilters {
                version_added_range: Some(VersionRange {
                    from: Some("0.250".to_owned()),
                    to: None,
                }),
                ..none.clone()
            },
        ),
        (
            "manufacturer, year, unknown year (OR)",
            MachineFilters {
                manufacturers: vec![Some("Namco".to_owned())],
                years: vec![Some("1981".to_owned()), None],
                ..none.clone()
            },
        ),
        (
            "driver and emulation status",
            MachineFilters {
                driver_statuses: vec![Some("good".to_owned())],
                emulation_statuses: vec![Some("good".to_owned()), None],
                ..none.clone()
            },
        ),
        (
            "software list",
            MachineFilters {
                software_lists: vec!["nes".to_owned(), "ibm5150".to_owned()],
                ..none.clone()
            },
        ),
        (
            "genre + category + manufacturer (AND)",
            MachineFilters {
                genres: genres[..1].to_vec(),
                categories: vec![category_parent],
                manufacturers: vec![Some("Namco".to_owned())],
                ..none.clone()
            },
        ),
        (
            "favorites",
            MachineFilters {
                list: Some("favorites".to_owned()),
                ..none.clone()
            },
        ),
        (
            "favorites + genre + version range",
            MachineFilters {
                list: Some("favorites".to_owned()),
                genres: genres[..1].to_vec(),
                version_added_range: Some(VersionRange {
                    from: Some("0.100".to_owned()),
                    to: Some("0.280".to_owned()),
                }),
                ..none
            },
        ),
    ]
}

fn snapshot_line(response: &MachineSearchResponse) -> String {
    let mut response = response.clone();
    response.elapsed_ms = 0.0;
    serde_json::to_string(&response).expect("json")
}

#[test]
#[ignore = "requires a real catalog and projection in YMDB_BENCH_CATALOG and YMDB_BENCH_CLASSIFICATIONS"]
fn classification_facet_snapshot() {
    let catalog = env_path("YMDB_BENCH_CATALOG");
    let classifications = env_path("YMDB_BENCH_CLASSIFICATIONS");
    let output = env_path("YMDB_BENCH_SNAPSHOT");
    let connection = open_classified(&catalog, &classifications);
    let texts: Vec<String> = match std::env::var("YMDB_BENCH_TEXTS") {
        Ok(_) => self::texts(),
        Err(_) => SNAPSHOT_TEXTS
            .iter()
            .map(|text| (*text).to_owned())
            .collect(),
    };
    let mut lines = String::new();
    let mut record =
        |label: String, request: &MachineSearchRequest, members: Option<&ListMembers>| {
            let started = Instant::now();
            let response = machines::search_machines(&connection, request, members).expect(&label);
            println!(
                "{label}: {} results, {:.1} ms",
                response.total,
                millis(started.elapsed())
            );
            let _ = writeln!(
                lines,
                "{label}\t{}\t{}",
                serde_json::to_string(request).expect("json"),
                snapshot_line(&response)
            );
        };
    for (name, filters) in snapshot_filters(&connection) {
        let members = machine_members(&connection, &filters);
        for text in &texts {
            let base = MachineSearchRequest {
                filters: filters.clone(),
                ..text_request(text)
            };
            record(
                format!("{name} / '{text}' / facets"),
                &MachineSearchRequest {
                    page_size: Some(1),
                    facets: browser_facets(),
                    ..base.clone()
                },
                members.as_ref(),
            );
            record(
                format!("{name} / '{text}' / page 1"),
                &MachineSearchRequest {
                    page: 1,
                    ..base.clone()
                },
                members.as_ref(),
            );
            // Small limits (has_more) and value searches.
            let small: Vec<FacetRequest<MachineFacet>> = browser_facets()
                .into_iter()
                .map(|request| FacetRequest {
                    limit: Some(3),
                    ..request
                })
                .collect();
            record(
                format!("{name} / '{text}' / limit 3"),
                &MachineSearchRequest {
                    page_size: Some(1),
                    facets: small,
                    ..base.clone()
                },
                members.as_ref(),
            );
            let searched: Vec<FacetRequest<MachineFacet>> = browser_facets()
                .into_iter()
                .map(|request| FacetRequest {
                    search: Some("a".to_owned()),
                    limit: Some(1000),
                    ..request
                })
                .collect();
            record(
                format!("{name} / '{text}' / value search"),
                &MachineSearchRequest {
                    page_size: Some(1),
                    facets: searched,
                    ..base
                },
                members.as_ref(),
            );
        }
    }
    std::fs::write(&output, lines).expect("write snapshot");
    println!("snapshot written to {output}");
}

// Typing

/// One request of the typing scenario, in milliseconds from the start.
struct Timing {
    label: String,
    sent: f64,
    lock: f64,
    done: f64,
}

#[test]
#[ignore = "requires a directory with catalog.sqlite and classifications.sqlite in YMDB_BENCH_STORE"]
fn classification_typing_benchmark() {
    // A store on copies: opening it may create or upgrade the projection.
    let store = Arc::new(CatalogStore::in_dir(Path::new(&env_path(
        "YMDB_BENCH_STORE",
    ))));
    store
        .with_reader(|_| Ok(()))
        .expect("store")
        .expect("catalog");
    // Searches applied after the browser's 300 ms debounce, 400 ms apart.
    let steps = ["gal", "gala", "galag"];
    let interval = Duration::from_millis(400);
    let started = Instant::now();
    let mut handles = Vec::new();
    for (index, text) in steps.iter().enumerate() {
        let at = interval * u32::try_from(index).expect("index");
        if let Some(wait) = at.checked_sub(started.elapsed()) {
            thread::sleep(wait);
        }
        // The browser sends the results request, then the facet request.
        for (kind, request) in [
            ("results", text_request(text)),
            ("facets", facet_request(text)),
        ] {
            let store = Arc::clone(&store);
            let label = format!("{text} {kind}");
            let sent = millis(started.elapsed());
            handles.push(thread::spawn(move || {
                let mut lock = 0.0;
                store
                    .with_reader(|connection| {
                        lock = millis(started.elapsed());
                        machines::search_machines(connection, &request, None).expect("search");
                        Ok(())
                    })
                    .expect("store");
                Timing {
                    label,
                    sent,
                    lock,
                    done: millis(started.elapsed()),
                }
            }));
            thread::sleep(Duration::from_millis(1));
        }
    }
    let mut timings: Vec<Timing> = handles
        .into_iter()
        .map(|handle| handle.join().expect("thread"))
        .collect();
    timings.sort_by(|a, b| a.sent.total_cmp(&b.sent));
    println!(
        "{:<16} {:>9} {:>11} {:>9} {:>10} {:>10}",
        "request", "sent ms", "lock wait", "run ms", "done ms", "latency"
    );
    for timing in timings {
        println!(
            "{:<16} {:>9.1} {:>11.1} {:>9.1} {:>10.1} {:>10.1}",
            timing.label,
            timing.sent,
            timing.lock - timing.sent,
            timing.done - timing.lock,
            timing.done,
            timing.done - timing.sent
        );
    }
}
