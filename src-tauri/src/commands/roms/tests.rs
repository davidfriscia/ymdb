//! Download jobs of the ROM manager state, with fake repositories: the job
//! thread, outcomes, retries, cancellation and Download & Play.

use std::fs;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use super::*;
use crate::launcher::session::SoftwareRef;
use crate::roms::download::TransferError;
use crate::roms::plan::PlanSummary;
use crate::roms::tests::{FakeConnector, FakeFile, NAS, connection, zip_bytes};

const INSTALLATION: &str = "0123456789abcdef0123456789abcdef";

fn stored(artifacts: Vec<PlanArtifact>, warnings: usize) -> Arc<StoredPlan> {
    let request = LaunchRequest {
        installation_id: INSTALLATION.to_owned(),
        machine: "pacman".to_owned(),
        software: None,
        profile_id: None,
    };
    Arc::new(StoredPlan {
        plan: DownloadPlan {
            id: new_id(),
            installation_id: INSTALLATION.to_owned(),
            installation_label: "MAME 0.280".to_owned(),
            request,
            target: LaunchTarget {
                machine: "pacman".to_owned(),
                software: None,
            },
            title: "Pac-Man".to_owned(),
            created_at: 1,
            artifacts,
            warnings: Vec::new(),
            connection_issues: Vec::new(),
            summary: PlanSummary {
                warnings,
                ..PlanSummary::default()
            },
        },
        connections: ConnectionsConfig {
            connections: vec![connection(NAS, "NAS", 22)],
            ..ConnectionsConfig::default()
        },
    })
}

fn wait_finished(state: &RomManagerState) -> DownloadJobStatus {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let status = state.status().expect("a job");
        if status.finished_at.is_some() {
            return status;
        }
        assert!(Instant::now() < deadline, "the download did not finish");
        thread::sleep(Duration::from_millis(10));
    }
}

/// A launch hook that reports the request it received.
fn hook(session_id: u64) -> (LaunchHook, mpsc::Receiver<LaunchRequest>) {
    let (sender, receiver) = mpsc::channel();
    let hook: LaunchHook = Box::new(move |request: &LaunchRequest| {
        sender.send(request.clone()).expect("send");
        LaunchOutcome::Started { session_id }
    });
    (hook, receiver)
}

#[test]
fn download_and_play_launches_the_plan_request_after_the_downloads() {
    let dir = tempfile::tempdir().expect("temp dir");
    let destination = dir.path().join("roms/pacman.zip");
    let plan = stored(
        vec![PlanArtifact::available(
            "pacman",
            NAS,
            "/roms/pacman.zip",
            destination.clone(),
        )],
        0,
    );
    let connector =
        FakeConnector::serving(NAS, &[("/roms/pacman.zip", FakeFile::Data(zip_bytes(100)))]);
    let state = RomManagerState::default();
    let (launch, launched) = hook(7);

    let started = state
        .start(Arc::clone(&plan), connector, Some(launch))
        .expect("start");
    assert_eq!(started.plan_id, plan.plan.id);
    assert!(started.play);

    let status = wait_finished(&state);
    assert_eq!(status.state, JobState::Completed);
    assert_eq!(status.installation_id, INSTALLATION);
    assert_eq!(status.transfers[0].status, TransferStatus::Downloaded);
    assert_eq!(status.bytes_transferred, 100);
    assert_eq!(status.bytes_total, Some(100));
    assert_eq!(
        status.launch,
        Some(LaunchOutcome::Started { session_id: 7 })
    );
    // The launch uses the request (and installation) of the plan.
    assert_eq!(launched.try_recv().expect("launched"), plan.plan.request);
    assert_eq!(fs::read(&destination).expect("read"), zip_bytes(100));
}

#[test]
fn failed_transfers_do_not_launch_and_a_retry_completes_the_plan() {
    let dir = tempfile::tempdir().expect("temp dir");
    let plan = stored(
        vec![
            PlanArtifact::available("a", NAS, "/roms/a.zip", dir.path().join("a.zip")),
            PlanArtifact::available("b", NAS, "/roms/b.zip", dir.path().join("b.zip")),
        ],
        1,
    );
    let state = RomManagerState::default();
    let (launch, launched) = hook(1);
    // `b` disappeared from the repository since the plan was built.
    let connector = FakeConnector::serving(NAS, &[("/roms/a.zip", FakeFile::Data(zip_bytes(10)))]);
    state
        .start(Arc::clone(&plan), connector, Some(launch))
        .expect("start");
    let status = wait_finished(&state);
    assert_eq!(status.state, JobState::Failed);
    assert_eq!(status.launch, Some(LaunchOutcome::NotStarted));
    assert!(launched.try_recv().is_err());
    assert!(matches!(
        status.transfers[1].error,
        Some(TransferError::Remote {
            error: crate::remote::provider::RemoteError::NotFound { .. }
        })
    ));

    // Retry: `a` is kept, only `b` is transferred.
    let connector = FakeConnector::serving(NAS, &[("/roms/b.zip", FakeFile::Data(zip_bytes(20)))]);
    state
        .start(Arc::clone(&plan), connector, None)
        .expect("retry");
    let status = wait_finished(&state);
    assert_eq!(
        status
            .transfers
            .iter()
            .map(|transfer| transfer.status)
            .collect::<Vec<_>>(),
        [TransferStatus::Downloaded, TransferStatus::Downloaded]
    );
    // The plan had a warning (for example a parent not found remotely).
    assert_eq!(status.state, JobState::CompletedWithWarnings);
    assert_eq!(status.launch, None);
    assert!(!status.play);
}

#[test]
fn one_download_at_a_time_and_cancellation() {
    let dir = tempfile::tempdir().expect("temp dir");
    let plan = stored(
        vec![PlanArtifact::available(
            "slow",
            NAS,
            "/roms/slow.zip",
            dir.path().join("slow.zip"),
        )],
        0,
    );
    let state = RomManagerState::default();
    let (launch, launched) = hook(1);
    state
        .start(
            Arc::clone(&plan),
            FakeConnector::serving(NAS, &[("/roms/slow.zip", FakeFile::Stalled)]),
            Some(launch),
        )
        .expect("start");
    assert_eq!(
        state
            .start(Arc::clone(&plan), FakeConnector::default(), None)
            .err(),
        Some(CommandError::DownloadInProgress)
    );
    assert_eq!(state.status().expect("job").state, JobState::Running);

    state.cancel();
    let status = wait_finished(&state);
    assert!(status.cancel_requested);
    assert_eq!(status.state, JobState::Cancelled);
    assert_eq!(status.transfers[0].status, TransferStatus::Cancelled);
    assert_eq!(status.launch, Some(LaunchOutcome::NotStarted));
    assert!(launched.try_recv().is_err());
    assert!(!dir.path().join("slow.zip").exists());
    assert!(fs::read_dir(dir.path()).expect("read").next().is_none());
}

#[test]
fn plans_without_downloads_are_refused() {
    let state = RomManagerState::default();
    let mut artifact = PlanArtifact::available("a", NAS, "/roms/a.zip", "a.zip".into());
    artifact.download = false;
    assert!(matches!(
        state.start(stored(vec![artifact], 0), FakeConnector::default(), None),
        Err(CommandError::InvalidRequest { .. })
    ));
    assert_eq!(state.status(), None);
}

#[test]
fn keeps_the_most_recent_plans() {
    let state = RomManagerState::default();
    let mut ids = Vec::new();
    for _ in 0..KEPT_PLANS + 2 {
        let plan = Arc::try_unwrap(stored(Vec::new(), 0)).ok().expect("plan");
        ids.push(plan.plan.id.clone());
        state.store(plan.plan, plan.connections);
    }
    assert!(state.plan(&ids[0]).is_err());
    assert!(state.plan(&ids[1]).is_err());
    assert!(state.plan(&ids[2]).is_ok());
    assert!(state.plan(ids.last().expect("last")).is_ok());
}

#[test]
fn serializes_the_status_to_the_frontend_contract() {
    let status = DownloadJobStatus {
        plan_id: "p".to_owned(),
        installation_id: INSTALLATION.to_owned(),
        installation_label: "MAME 0.280".to_owned(),
        target: LaunchTarget {
            machine: "nes".to_owned(),
            software: Some(SoftwareRef {
                list: "nes".to_owned(),
                name: "smb".to_owned(),
            }),
        },
        title: "Super Mario Bros.".to_owned(),
        play: true,
        state: JobState::CompletedWithWarnings,
        cancel_requested: false,
        transfers: vec![TransferState {
            artifact_id: "softwareListRoms:nes/smb.zip".to_owned(),
            status: TransferStatus::Failed,
            bytes_transferred: 5,
            size: Some(10),
            error: Some(TransferError::SizeMismatch {
                expected: 10,
                received: 5,
            }),
        }],
        bytes_transferred: 5,
        bytes_total: Some(10),
        started_at: 1,
        finished_at: Some(2),
        launch: Some(LaunchOutcome::Failed {
            error: CommandError::CatalogNotImported,
        }),
    };
    assert_eq!(
        serde_json::to_value(status).expect("serialize"),
        serde_json::json!({
            "planId": "p",
            "installationId": INSTALLATION,
            "installationLabel": "MAME 0.280",
            "target": { "machine": "nes", "software": { "list": "nes", "name": "smb" } },
            "title": "Super Mario Bros.",
            "play": true,
            "state": "completedWithWarnings",
            "cancelRequested": false,
            "transfers": [{
                "artifactId": "softwareListRoms:nes/smb.zip",
                "status": "failed",
                "bytesTransferred": 5,
                "size": 10,
                "error": { "kind": "sizeMismatch", "expected": 10, "received": 5 }
            }],
            "bytesTransferred": 5,
            "bytesTotal": 10,
            "startedAt": 1,
            "finishedAt": 2,
            "launch": { "status": "failed", "error": { "kind": "catalogNotImported" } }
        })
    );
    assert_eq!(
        serde_json::to_value(LaunchOutcome::Started { session_id: 3 }).expect("serialize"),
        serde_json::json!({ "status": "started", "sessionId": 3 })
    );
    assert_eq!(
        serde_json::to_value(CommandError::DownloadInProgress).expect("serialize"),
        serde_json::json!({ "kind": "downloadInProgress" })
    );
}

#[test]
fn serializes_plans_to_the_frontend_contract() {
    let plan = stored(
        vec![PlanArtifact::available(
            "pacman",
            NAS,
            "/roms/pacman.zip",
            "pacman.zip".into(),
        )],
        0,
    );
    let json = serde_json::to_value(&plan.plan).expect("serialize");
    assert_eq!(
        json["artifacts"][0],
        serde_json::json!({
            "id": "machineRoms:pacman.zip",
            "category": "machineRoms",
            "set": "pacman",
            "reason": "selected",
            "fileName": "pacman.zip",
            "relativePath": "pacman.zip",
            "connectionId": NAS,
            "remotePath": "/roms/pacman.zip",
            "destination": "pacman.zip",
            "local": null,
            "availability": "remoteAvailable",
            "remoteSize": null,
            "remoteError": null,
            "download": true,
            "warning": null
        })
    );
    assert_eq!(
        json["summary"],
        serde_json::json!({
            "present": 0, "toDownload": 0, "notFound": 0, "unresolved": 0,
            "warnings": 0, "downloadBytes": 0, "unknownSizes": 0
        })
    );
    assert_eq!(json["request"]["installationId"], INSTALLATION);
}

#[test]
fn searches_the_resolved_paths_of_the_launch() {
    use crate::launcher::config::OptionSource;
    let dir = tempfile::tempdir().expect("temp dir");
    let mame_dir = dir.path().join("MAME ✓");
    let extra = dir.path().join("extra ROMs");
    let options = vec![ResolvedOption {
        name: "rompath".to_owned(),
        value: OptionValue::List(vec!["roms".to_owned(), extra.display().to_string()]),
        source: OptionSource::RomManager,
    }];
    let search = effective_search_dirs(&mame_dir, &options);
    assert_eq!(search.rom, [mame_dir.join("roms"), extra.clone()]);
    // Without an option, MAME's own value (no mame.ini here: its default).
    let native = read_search_paths(&mame_dir);
    if native.ini_file.is_none() {
        assert_eq!(search.sample, [mame_dir.join("samples")]);
    }
}
