//! End-to-end check of the ROM manager with a real MAME executable: the
//! catalog is imported from `mame -listxml` into a temporary directory, a
//! synthetic repository is served by the in-process SFTP server, the plan
//! is built, the files are downloaded and the launch is prepared with the
//! download destination in `rompath`. MAME itself is only asked for its
//! machine list; nothing is written to its directory and no game is started.
//!
//! Run with:
//! `YMDB_E2E_MAME=/path/to/mame cargo test --lib rom_manager_end_to_end -- --ignored --nocapture`

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use super::Reason;
use super::download::{self, DownloadTask, Progress, TransferStatus};
use super::local::SearchDirs;
use super::plan::{Availability, WarningKind, build_artifacts, check_remote, finish};
use super::planner::{Organizations, PlanTarget, plan};
use super::tests::{NAS, connection, sftp_connector, zip_bytes};
use crate::catalog::import::{ImportProgress, import_catalog};
use crate::catalog::store::CatalogStore;
use crate::launcher::config::LaunchConfig;
use crate::launcher::search_paths::RequiredSearchPaths;
use crate::launcher::{LaunchRequest, prepare};
use crate::mame::ini::read_search_paths;
use crate::remote::connections::{ConnectionsConfig, ProviderConfig};
use crate::remote::path::RemotePath;
use crate::remote::sources::{
    CategorySource, CollectionOrganization, LocalDestination, RemoteSourcesConfig, SourceCategory,
};
use crate::remote::test_server::{ServerOptions, TestServer};

#[tokio::test]
#[ignore = "requires a MAME executable in YMDB_E2E_MAME"]
async fn rom_manager_end_to_end() {
    let executable =
        PathBuf::from(std::env::var("YMDB_E2E_MAME").expect("YMDB_E2E_MAME must be set"));
    let mame_dir = executable.parent().expect("MAME directory").to_path_buf();
    let work = tempfile::tempdir().expect("temp dir");
    let store = CatalogStore::in_dir(&work.path().join("catalog"));
    let started = std::time::Instant::now();
    let imported = import_catalog(
        &executable,
        &store,
        &Arc::new(AtomicBool::new(false)),
        &ImportProgress::default(),
    )
    .expect("import");
    println!(
        "imported {} machines of MAME {} in {:?}",
        imported.statistics.machines,
        imported.mame_release,
        started.elapsed()
    );

    // The candidates of real machines: a clone, a game on a BIOS, a disk.
    let candidates = |machine: &str, chds: Option<CollectionOrganization>| {
        store
            .with_reader(|connection| {
                Ok(plan(
                    connection,
                    &PlanTarget {
                        machine,
                        software: None,
                    },
                    Organizations {
                        machine_chds: chds,
                        samples: None,
                    },
                ))
            })
            .expect("reader")
            .expect("catalog")
            .expect("plan")
    };
    for machine in ["pacman", "kof98a", "kinst"] {
        let found = candidates(machine, Some(CollectionOrganization::Split));
        println!("{machine}:");
        for candidate in &found {
            println!(
                "  {:?} {:?} {}",
                candidate.category,
                candidate.reason,
                candidate.segments.join("/")
            );
        }
    }
    let kof98a = candidates("kof98a", None);
    let sets: Vec<(&str, Reason)> = kof98a
        .iter()
        .take(3)
        .map(|candidate| (candidate.set.as_str(), candidate.reason))
        .collect();
    assert_eq!(
        sets,
        [
            ("kof98a", Reason::Selected),
            ("kof98", Reason::Parent),
            ("neogeo", Reason::Bios)
        ]
    );
    assert!(
        candidates("kinst", Some(CollectionOrganization::Split))
            .iter()
            .any(|candidate| candidate.category == SourceCategory::MachineChds)
    );

    // A synthetic repository with every candidate of pacman but its parent.
    let pacman = candidates("pacman", None);
    let served = work.path().join("served");
    for candidate in &pacman {
        if candidate.set == "puckman" {
            continue;
        }
        let mut path = served.join("roms");
        path.extend(&candidate.segments);
        fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
        fs::write(&path, zip_bytes(1024)).expect("write");
    }
    let server = TestServer::start(ServerOptions {
        host_key: "host_a",
        authorized: vec!["client_ed25519"],
        root: served.clone(),
        sftp: true,
    })
    .await;
    let mut nas = connection(NAS, "NAS", server.port);
    let ProviderConfig::Sftp(config) = &mut nas.provider;
    config.approved_host_key = Some(super::tests::approved("host_a"));
    let connections = ConnectionsConfig {
        connections: vec![nas.clone()],
        ..ConnectionsConfig::default()
    };
    let destination = work.path().join("downloads");
    let sources = RemoteSourcesConfig {
        categories: [(
            SourceCategory::MachineRoms,
            CategorySource {
                enabled: true,
                connection_id: Some(NAS.to_owned()),
                remote_base_path: Some("/roms".to_owned()),
                organization: Some(CollectionOrganization::Split),
                destination: Some(LocalDestination::Custom {
                    directory: destination.clone(),
                }),
            },
        )]
        .into_iter()
        .collect(),
        ..RemoteSourcesConfig::default()
    };
    // Only the destination is searched, so the user's ROMs play no part.
    let search = SearchDirs::new(&mame_dir, &[destination.display().to_string()], &[]);
    let mut artifacts = build_artifacts(pacman, &mame_dir, &search, &sources, &connections);
    let connector = sftp_connector();
    let issues = check_remote(&mut artifacts, &connections, &connector).await;
    assert!(issues.is_empty(), "{issues:?}");
    let summary = finish(&mut artifacts, &[]);
    let parent = artifacts
        .iter()
        .find(|artifact| artifact.id == "machineRoms:puckman.zip")
        .expect("parent");
    assert_eq!(parent.availability, Availability::RemoteMissing);
    assert_eq!(
        parent.warning.as_ref().map(|warning| warning.kind),
        Some(WarningKind::Availability)
    );
    assert!(summary.to_download >= 1);

    let tasks: Vec<DownloadTask> = artifacts
        .iter()
        .filter(|artifact| artifact.download)
        .map(|artifact| DownloadTask {
            artifact_id: artifact.id.clone(),
            connection: nas.clone(),
            remote_path: RemotePath::parse(artifact.remote_path.as_deref().expect("remote"))
                .expect("path"),
            destination: artifact.destination.clone().expect("destination"),
            expected_size: artifact.remote_size,
        })
        .collect();
    let progress = Progress::new(&tasks, &[]);
    download::run(&tasks, &connector, &progress, &AtomicBool::new(false)).await;
    for state in progress.snapshot() {
        println!("{} {:?}", state.artifact_id, state.status);
        assert_eq!(state.status, TransferStatus::Downloaded);
    }
    assert!(destination.join("pacman.zip").is_file());

    // The launch searches the destination, composed with MAME's own rompath.
    let required = RequiredSearchPaths {
        rompath: vec![destination.display().to_string()],
        samplepath: Vec::new(),
        native: read_search_paths(&mame_dir),
    };
    let request = LaunchRequest {
        installation_id: "0123456789abcdef0123456789abcdef".to_owned(),
        machine: "pacman".to_owned(),
        software: None,
        profile_id: None,
    };
    let prepared = store
        .with_reader(|connection| {
            Ok(prepare(
                connection,
                &executable,
                &LaunchConfig::default(),
                &required,
                &request,
            ))
        })
        .expect("reader")
        .expect("catalog")
        .expect("prepare");
    let command_line = prepared.plan.command_line();
    println!("{command_line:?}");
    let rompath = command_line
        .iter()
        .position(|token| token == "-rompath")
        .and_then(|index| command_line.get(index + 1))
        .expect("rompath");
    assert!(rompath.contains(&destination.display().to_string()));
    assert_eq!(prepared.plan.working_directory, mame_dir);
}
