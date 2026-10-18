//! Benchmark of representative searches on a real catalog.
//!
//! Run with:
//! `YMDB_BENCH_CATALOG=/path/to/catalog.sqlite cargo test --release --lib
//! search_benchmark -- --ignored --nocapture`
//!
//! For each case it reports the first execution on a new connection (empty
//! SQLite page cache; the operating system cache may still be warm) and the
//! median and maximum of repeated executions, then prints the query plans of
//! the result queries.

use std::time::{Duration, Instant};

use rusqlite::types::Value;
use rusqlite::{Connection, OpenFlags, params_from_iter};

use super::request::{
    CloneFilter, FacetRequest, MachineFacet, MachineFilters, MachineSearchRequest, MachineSort,
    SoftwareFacet, SoftwareFilters, SoftwareSearchRequest, SoftwareSort, SortDirection,
    SupportStatus,
};
use super::{ListMembers, machines, software, text};

mod classification_facets;

const RUNS: usize = 15;

enum Request {
    Machines(MachineSearchRequest),
    Software(SoftwareSearchRequest),
}

fn open(path: &str) -> Connection {
    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).expect("open catalog")
}

fn run(connection: &Connection, request: &Request) -> u64 {
    match request {
        Request::Machines(request) => {
            machines::search_machines(
                connection,
                request,
                machine_members(connection, &request.filters).as_ref(),
            )
            .expect("search")
            .total
        }
        Request::Software(request) => {
            software::search_software(
                connection,
                request,
                software_members(connection, &request.filters).as_ref(),
            )
            .expect("search")
            .total
        }
    }
}

/// Size of the sample list used by the `favorites` cases.
const LIST_SIZE: usize = 2000;

/// A sample list (every 10th machine by name) when the request has a list filter.
fn machine_members(connection: &Connection, filters: &MachineFilters) -> Option<ListMembers> {
    filters.list.as_ref()?;
    let names: Vec<String> = connection
        .prepare(&format!(
            "SELECT name FROM machines WHERE id % 10 = 0 ORDER BY name LIMIT {LIST_SIZE}"
        ))
        .expect("sample")
        .query_map([], |row| row.get(0))
        .expect("sample")
        .collect::<Result<_, _>>()
        .expect("rows");
    Some(ListMembers::machines(names.iter().map(String::as_str)))
}

/// A sample list (every 10th software) when the request has a list filter.
fn software_members(connection: &Connection, filters: &SoftwareFilters) -> Option<ListMembers> {
    filters.list.as_ref()?;
    let keys: Vec<(String, String)> = connection
        .prepare(&format!(
            "SELECT list_name, name FROM software WHERE id % 10 = 0 LIMIT {LIST_SIZE}"
        ))
        .expect("sample")
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .expect("sample")
        .collect::<Result<_, _>>()
        .expect("rows");
    Some(ListMembers::software(
        keys.iter()
            .map(|(list, name)| (list.as_str(), name.as_str())),
    ))
}
fn millis(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

fn machine_text(text: &str) -> MachineSearchRequest {
    MachineSearchRequest {
        text: Some(text.to_owned()),
        ..MachineSearchRequest::default()
    }
}

fn software_text(text: &str) -> SoftwareSearchRequest {
    SoftwareSearchRequest {
        text: Some(text.to_owned()),
        ..SoftwareSearchRequest::default()
    }
}

fn machine_facets() -> Vec<FacetRequest<MachineFacet>> {
    [
        MachineFacet::Manufacturer,
        MachineFacet::Year,
        MachineFacet::DriverStatus,
        MachineFacet::EmulationStatus,
        MachineFacet::SoftwareList,
    ]
    .into_iter()
    .map(|facet| FacetRequest {
        facet,
        search: None,
        limit: None,
    })
    .collect()
}

fn software_facets() -> Vec<FacetRequest<SoftwareFacet>> {
    [
        SoftwareFacet::List,
        SoftwareFacet::Publisher,
        SoftwareFacet::Year,
        SoftwareFacet::Supported,
        SoftwareFacet::Interface,
    ]
    .into_iter()
    .map(|facet| FacetRequest {
        facet,
        search: None,
        limit: None,
    })
    .collect()
}

fn single_facet_cases() -> Vec<(&'static str, Request)> {
    let machine = |label, facet| {
        (
            label,
            Request::Machines(MachineSearchRequest {
                page_size: Some(1),
                facets: vec![FacetRequest {
                    facet,
                    search: None,
                    limit: None,
                }],
                ..MachineSearchRequest::default()
            }),
        )
    };
    let software = |label, facet| {
        (
            label,
            Request::Software(SoftwareSearchRequest {
                page_size: Some(1),
                facets: vec![FacetRequest {
                    facet,
                    search: None,
                    limit: None,
                }],
                ..SoftwareSearchRequest::default()
            }),
        )
    };
    vec![
        machine("machines: facet manufacturer", MachineFacet::Manufacturer),
        machine("machines: facet year", MachineFacet::Year),
        machine("machines: facet driver status", MachineFacet::DriverStatus),
        machine(
            "machines: facet emulation status",
            MachineFacet::EmulationStatus,
        ),
        machine("machines: facet software list", MachineFacet::SoftwareList),
        software("software: facet list", SoftwareFacet::List),
        software("software: facet publisher", SoftwareFacet::Publisher),
        software("software: facet year", SoftwareFacet::Year),
        software("software: facet supported", SoftwareFacet::Supported),
        software("software: facet interface", SoftwareFacet::Interface),
        (
            "software: facet publisher, value search 'nin'",
            Request::Software(SoftwareSearchRequest {
                page_size: Some(1),
                facets: vec![FacetRequest {
                    facet: SoftwareFacet::Publisher,
                    search: Some("nin".to_owned()),
                    limit: None,
                }],
                ..SoftwareSearchRequest::default()
            }),
        ),
    ]
}

fn cases() -> Vec<(&'static str, Request)> {
    let systems = MachineFilters {
        device: Some(false),
        bios: Some(false),
        ..MachineFilters::default()
    };
    let mut cases = vec![
        (
            "machines: no text, name order",
            Request::Machines(MachineSearchRequest::default()),
        ),
        (
            "machines: exact name 'pacman'",
            Request::Machines(machine_text("pacman")),
        ),
        ("machines: prefix 's'", Request::Machines(machine_text("s"))),
        (
            "machines: prefix 'str'",
            Request::Machines(machine_text("str")),
        ),
        (
            "machines: 'street fighter'",
            Request::Machines(machine_text("street fighter")),
        ),
        (
            "machines: 'super mario world'",
            Request::Machines(machine_text("super mario world")),
        ),
        (
            "machines: manufacturer 'capcom'",
            Request::Machines(machine_text("capcom")),
        ),
        (
            "machines: text + 6 filters",
            Request::Machines(MachineSearchRequest {
                text: Some("fighter".to_owned()),
                filters: MachineFilters {
                    clones: Some(CloneFilter::Parents),
                    runnable: Some(true),
                    driver_statuses: vec![Some("good".to_owned())],
                    has_roms: Some(true),
                    ..systems.clone()
                },
                ..MachineSearchRequest::default()
            }),
        ),
        (
            "machines: filters only (systems, has disks)",
            Request::Machines(MachineSearchRequest {
                filters: MachineFilters {
                    has_disks: Some(true),
                    ..systems.clone()
                },
                ..MachineSearchRequest::default()
            }),
        ),
        (
            "machines: software list filter",
            Request::Machines(MachineSearchRequest {
                filters: MachineFilters {
                    software_lists: vec!["ibm5150".to_owned()],
                    ..MachineFilters::default()
                },
                ..MachineSearchRequest::default()
            }),
        ),
        (
            "machines: sort year desc",
            Request::Machines(MachineSearchRequest {
                sort: Some(MachineSort::Year),
                direction: SortDirection::Descending,
                ..MachineSearchRequest::default()
            }),
        ),
        (
            "machines: sort description",
            Request::Machines(MachineSearchRequest {
                sort: Some(MachineSort::Description),
                ..MachineSearchRequest::default()
            }),
        ),
        (
            "machines: sort manufacturer",
            Request::Machines(MachineSearchRequest {
                sort: Some(MachineSort::Manufacturer),
                ..MachineSearchRequest::default()
            }),
        ),
        (
            "machines: 5 facets, no text",
            Request::Machines(MachineSearchRequest {
                facets: machine_facets(),
                ..MachineSearchRequest::default()
            }),
        ),
        (
            "machines: 5 facets, text 'fighter'",
            Request::Machines(MachineSearchRequest {
                text: Some("fighter".to_owned()),
                facets: machine_facets(),
                ..MachineSearchRequest::default()
            }),
        ),
        (
            "machines: deep page (offset 45000), name",
            Request::Machines(MachineSearchRequest {
                page: 900,
                ..MachineSearchRequest::default()
            }),
        ),
        (
            "machines: deep page (offset 45000), description",
            Request::Machines(MachineSearchRequest {
                page: 900,
                sort: Some(MachineSort::Description),
                ..MachineSearchRequest::default()
            }),
        ),
        (
            "software: no text, name order",
            Request::Software(SoftwareSearchRequest::default()),
        ),
        (
            "software: exact name 'smb'",
            Request::Software(software_text("smb")),
        ),
        (
            "software: prefix 's'",
            Request::Software(software_text("s")),
        ),
        (
            "software: 'super mario'",
            Request::Software(software_text("super mario")),
        ),
        (
            "software: publisher 'nintendo'",
            Request::Software(software_text("nintendo")),
        ),
        (
            "software: text + list + support filters",
            Request::Software(SoftwareSearchRequest {
                text: Some("mario".to_owned()),
                filters: SoftwareFilters {
                    lists: vec!["nes".to_owned(), "snes".to_owned()],
                    supported: vec![SupportStatus::Yes],
                    clones: Some(CloneFilter::Parents),
                    ..SoftwareFilters::default()
                },
                ..SoftwareSearchRequest::default()
            }),
        ),
        (
            "software: has roms = false",
            Request::Software(SoftwareSearchRequest {
                filters: SoftwareFilters {
                    has_roms: Some(false),
                    ..SoftwareFilters::default()
                },
                ..SoftwareSearchRequest::default()
            }),
        ),
        (
            "software: interface filter",
            Request::Software(SoftwareSearchRequest {
                filters: SoftwareFilters {
                    interfaces: vec!["floppy_5_25".to_owned()],
                    ..SoftwareFilters::default()
                },
                ..SoftwareSearchRequest::default()
            }),
        ),
        (
            "software: sort description",
            Request::Software(SoftwareSearchRequest {
                sort: Some(SoftwareSort::Description),
                ..SoftwareSearchRequest::default()
            }),
        ),
        (
            "software: 5 facets, no text",
            Request::Software(SoftwareSearchRequest {
                facets: software_facets(),
                ..SoftwareSearchRequest::default()
            }),
        ),
        (
            "software: 5 facets, text 'mario'",
            Request::Software(SoftwareSearchRequest {
                text: Some("mario".to_owned()),
                facets: software_facets(),
                ..SoftwareSearchRequest::default()
            }),
        ),
        (
            "software: deep page (offset 130000), name",
            Request::Software(SoftwareSearchRequest {
                page: 2600,
                ..SoftwareSearchRequest::default()
            }),
        ),
        (
            "software: deep page (offset 130000), description",
            Request::Software(SoftwareSearchRequest {
                page: 2600,
                sort: Some(SoftwareSort::Description),
                ..SoftwareSearchRequest::default()
            }),
        ),
        (
            "machines: favorites (2000), 5 facets",
            Request::Machines(MachineSearchRequest {
                filters: MachineFilters {
                    list: Some("favorites".to_owned()),
                    ..MachineFilters::default()
                },
                facets: machine_facets(),
                ..MachineSearchRequest::default()
            }),
        ),
        (
            "machines: favorites (2000), text 'pac'",
            Request::Machines(MachineSearchRequest {
                text: Some("pac".to_owned()),
                filters: MachineFilters {
                    list: Some("favorites".to_owned()),
                    ..MachineFilters::default()
                },
                ..MachineSearchRequest::default()
            }),
        ),
        (
            "software: favorites (2000), 5 facets",
            Request::Software(SoftwareSearchRequest {
                filters: SoftwareFilters {
                    list: Some("favorites".to_owned()),
                    ..SoftwareFilters::default()
                },
                facets: software_facets(),
                ..SoftwareSearchRequest::default()
            }),
        ),
    ];
    cases.extend(single_facet_cases());
    cases
}

fn explain(connection: &Connection, label: &str, sql: &str, params: &[Value]) {
    let mut statement = connection
        .prepare(&format!("EXPLAIN QUERY PLAN {sql}"))
        .expect("explain");
    let details: Vec<String> = statement
        .query_map(params_from_iter(params), |row| row.get::<_, String>(3))
        .expect("plan")
        .collect::<Result<_, _>>()
        .expect("rows");
    println!("{label}\n    {}", details.join("\n    "));
}

fn explain_plans(connection: &Connection) {
    println!("\nQuery plans of result queries:");
    for (label, request) in cases() {
        match request {
            Request::Machines(request) => {
                let text = text::parse(request.text.as_deref()).expect("text");
                let conditions = machines::conditions(
                    &request.filters,
                    machine_members(connection, &request.filters).as_ref(),
                    text.as_ref(),
                    None,
                );
                let (order, order_params) =
                    machines::order_by(request.sort, request.direction, text.as_ref());
                let mut params = conditions.params().to_vec();
                params.extend(order_params);
                let sql = format!(
                    "SELECT m.name FROM {}{} ORDER BY {order} LIMIT 50 OFFSET {}",
                    machines::from_clause(text.as_ref()),
                    conditions.where_clause(),
                    u64::from(request.page) * 50
                );
                explain(connection, label, &sql, &params);
            }
            Request::Software(request) => {
                let text = text::parse(request.text.as_deref()).expect("text");
                let conditions = software::conditions(
                    &request.filters,
                    software_members(connection, &request.filters).as_ref(),
                    text.as_ref(),
                    None,
                );
                let (order, order_params) =
                    software::order_by(request.sort, request.direction, text.as_ref());
                let mut params = conditions.params().to_vec();
                params.extend(order_params);
                let sql = format!(
                    "SELECT s.name FROM {}{} ORDER BY {order} LIMIT 50 OFFSET {}",
                    software::from_clause(text.as_ref()),
                    conditions.where_clause(),
                    u64::from(request.page) * 50
                );
                explain(connection, label, &sql, &params);
            }
        }
    }
}

#[test]
#[ignore = "requires a real catalog in YMDB_BENCH_CATALOG"]
fn search_benchmark() {
    let path = std::env::var("YMDB_BENCH_CATALOG").expect("YMDB_BENCH_CATALOG must be set");
    println!(
        "{:<52} {:>9} {:>10} {:>10} {:>9}",
        "case", "results", "first ms", "median ms", "max ms"
    );
    let warm = open(&path);
    for (label, request) in cases() {
        let cold = open(&path);
        let started = Instant::now();
        let total = run(&cold, &request);
        let first = millis(started.elapsed());
        drop(cold);

        let mut times: Vec<f64> = (0..RUNS)
            .map(|_| {
                let started = Instant::now();
                run(&warm, &request);
                millis(started.elapsed())
            })
            .collect();
        times.sort_by(f64::total_cmp);
        println!(
            "{label:<52} {total:>9} {first:>10.2} {:>10.2} {:>9.2}",
            times[RUNS / 2],
            times[RUNS - 1]
        );
    }
    explain_plans(&warm);
}
