//! ROM manager tests: the planner on a catalog imported from synthetic
//! fixtures, plan building with fake repositories, and downloads against
//! fake repositories and the in-process SFTP server. No real ROM is used.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::task::{Context, Poll};
use std::time::Duration;

use rusqlite::Connection;
use tempfile::TempDir;
use tokio::io::{AsyncRead, ReadBuf};

use super::download::{self, DownloadTask, Progress, TransferError, TransferStatus, temp_path};
use super::local::{SearchDirs, find, is_portable_name};
use super::plan::{
    Availability, PlanArtifact, PlanWarning, WarningKind, build_artifacts, check_remote, finish,
};
use super::planner::{Candidate, Organizations, PlanTarget, SoftwareTarget, plan};
use super::{Connector, ProviderConnector, Reason};
use crate::catalog::import::{ImportProgress, import_from_reader, import_software_from_reader};
use crate::catalog::store::{CatalogSource, CatalogStore};
use crate::remote::connections::{
    ApprovedHostKey, ConnectionsConfig, HostKeyVerification, ProviderConfig, RemoteConnection,
    SftpConfig, SftpSettings,
};
use crate::remote::path::RemotePath;
use crate::remote::provider::{
    Capabilities, ConnectOptions, Download, EntryKind, RemoteEntry, RemoteError, RemoteMetadata,
    RemoteRepository, Timeouts,
};
use crate::remote::sources::{
    CategorySource, CollectionOrganization, LocalDestination, RemoteSourcesConfig, SourceCategory,
};
use crate::remote::test_server::{ServerOptions, TestServer, private_key, testdata};

use CollectionOrganization::{Directories, Merged, NonMerged, Split, ZipArchives};
use SourceCategory::{MachineChds, MachineRoms, Samples, SoftwareListChds, SoftwareListRoms};

const MACHINES: &str = include_str!("../catalog/testdata/rom-manager-machines.xml");
const SOFTWARE: &str = include_str!("../catalog/testdata/rom-manager-software.xml");

struct Catalog {
    _dir: TempDir,
    store: CatalogStore,
}

impl Catalog {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = CatalogStore::in_dir(dir.path());
        let source = CatalogSource {
            mame_release: "0.280".to_owned(),
            mame_revision: "mame0280".to_owned(),
            executable_path: "/opt/mame/mame".into(),
            executable_size: Some(1),
            executable_modified_at: Some(1),
        };
        let cancel = AtomicBool::new(false);
        import_from_reader(
            MACHINES.as_bytes(),
            &store,
            source.clone(),
            &cancel,
            &ImportProgress::default(),
            || Ok(()),
        )
        .expect("machines");
        import_software_from_reader(
            SOFTWARE.as_bytes(),
            &store,
            source,
            &cancel,
            &ImportProgress::default(),
            || Ok(()),
        )
        .expect("software");
        Self { _dir: dir, store }
    }

    fn read<T>(&self, operation: impl FnOnce(&Connection) -> T) -> T {
        self.store
            .with_reader(|connection| Ok(operation(connection)))
            .expect("store")
            .expect("catalog")
    }

    fn machine(&self, machine: &str, organizations: Organizations) -> Vec<Candidate> {
        self.read(|connection| {
            plan(
                connection,
                &PlanTarget {
                    machine,
                    software: None,
                },
                organizations,
            )
            .expect("plan")
        })
    }

    fn software(&self, machine: &str, list: &str, name: &str, parts: &[&str]) -> Vec<Candidate> {
        let parts: Vec<String> = parts.iter().map(|&part| part.to_owned()).collect();
        self.read(|connection| {
            plan(
                connection,
                &PlanTarget {
                    machine,
                    software: Some(SoftwareTarget {
                        list,
                        name,
                        parts: &parts,
                    }),
                },
                Organizations::default(),
            )
            .expect("plan")
        })
    }
}

type Described = (
    SourceCategory,
    String,
    Reason,
    String,
    Vec<CollectionOrganization>,
);

fn describe(candidates: &[Candidate]) -> Vec<Described> {
    candidates
        .iter()
        .map(|candidate| {
            (
                candidate.category,
                candidate.set.clone(),
                candidate.reason,
                candidate.segments.join("/"),
                candidate
                    .absent_note
                    .as_ref()
                    .map(|note| note.organizations.clone())
                    .unwrap_or_default(),
            )
        })
        .collect()
}

fn expect(
    category: SourceCategory,
    set: &str,
    reason: Reason,
    path: &str,
    absent_in: &[CollectionOrganization],
) -> Described {
    (
        category,
        set.to_owned(),
        reason,
        path.to_owned(),
        absent_in.to_vec(),
    )
}

// ---------------------------------------------------------------- planner

#[test]
fn standalone_machine_with_devices_and_indirect_devices() {
    let catalog = Catalog::new();
    assert_eq!(
        describe(&catalog.machine("solo", Organizations::default())),
        [
            expect(MachineRoms, "solo", Reason::Selected, "solo.zip", &[]),
            // `cpu` has no ROM; `subcpu` is a device of the `sound` device.
            expect(MachineRoms, "sound", Reason::Device, "sound.zip", &[]),
            expect(MachineRoms, "subcpu", Reason::Device, "subcpu.zip", &[]),
        ]
    );
}

#[test]
fn clones_include_their_parent_whatever_the_organization() {
    let catalog = Catalog::new();
    let expected = [
        expect(
            MachineRoms,
            "pacman",
            Reason::Selected,
            "pacman.zip",
            &[Merged],
        ),
        expect(
            MachineRoms,
            "puckman",
            Reason::Parent,
            "puckman.zip",
            &[NonMerged],
        ),
    ];
    for organization in [None, Some(Merged), Some(Split), Some(NonMerged)] {
        let organizations = Organizations {
            machine_chds: organization,
            samples: None,
        };
        assert_eq!(
            describe(&catalog.machine("pacman", organizations)),
            expected,
            "{organization:?}"
        );
    }
    // A clone whose ROMs all come from its parent may have no archive in
    // merged and split collections.
    assert_eq!(
        describe(&catalog.machine("pacmerged", Organizations::default())),
        [
            expect(
                MachineRoms,
                "pacmerged",
                Reason::Selected,
                "pacmerged.zip",
                &[Merged, Split]
            ),
            expect(
                MachineRoms,
                "puckman",
                Reason::Parent,
                "puckman.zip",
                &[NonMerged]
            ),
        ]
    );
}

#[test]
fn follows_parents_and_bios_sets_indirectly() {
    let catalog = Catalog::new();
    assert_eq!(
        describe(&catalog.machine("fightera", Organizations::default())),
        [
            expect(
                MachineRoms,
                "fightera",
                Reason::Selected,
                "fightera.zip",
                &[Merged]
            ),
            expect(
                MachineRoms,
                "fighter",
                Reason::Parent,
                "fighter.zip",
                &[NonMerged]
            ),
            expect(MachineRoms, "bios", Reason::Bios, "bios.zip", &[]),
        ]
    );
    assert_eq!(
        describe(&catalog.machine("fighter", Organizations::default())),
        [
            expect(MachineRoms, "fighter", Reason::Selected, "fighter.zip", &[]),
            expect(MachineRoms, "bios", Reason::Bios, "bios.zip", &[]),
        ]
    );
}

#[test]
fn machine_disks_of_the_machine_and_its_parent() {
    let catalog = Catalog::new();
    let roms = [
        expect(
            MachineRoms,
            "kinst2",
            Reason::Selected,
            "kinst2.zip",
            &[Merged],
        ),
        expect(
            MachineRoms,
            "kinst",
            Reason::Parent,
            "kinst.zip",
            &[NonMerged],
        ),
    ];
    // Merged and split: the merged disk is the parent's disk.
    for organization in [None, Some(Merged), Some(Split)] {
        let candidates = catalog.machine(
            "kinst2",
            Organizations {
                machine_chds: organization,
                samples: None,
            },
        );
        let mut expected = roms.to_vec();
        expected.extend([
            expect(
                MachineChds,
                "kinst2",
                Reason::Selected,
                "kinst2/kinst2.chd",
                &[],
            ),
            expect(
                MachineChds,
                "kinst",
                Reason::Parent,
                "kinst/kinst.chd",
                &[NonMerged],
            ),
        ]);
        assert_eq!(describe(&candidates), expected, "{organization:?}");
    }
    // Non-merged: every disk is also in the clone's directory.
    let candidates = catalog.machine(
        "kinst2",
        Organizations {
            machine_chds: Some(NonMerged),
            samples: None,
        },
    );
    let mut expected = roms.to_vec();
    expected.extend([
        expect(
            MachineChds,
            "kinst2",
            Reason::Selected,
            "kinst2/kinst.chd",
            &[],
        ),
        expect(
            MachineChds,
            "kinst2",
            Reason::Selected,
            "kinst2/kinst2.chd",
            &[],
        ),
        expect(
            MachineChds,
            "kinst",
            Reason::Parent,
            "kinst/kinst.chd",
            &[NonMerged],
        ),
    ]);
    assert_eq!(describe(&candidates), expected);
}

#[test]
fn samples_as_archives_or_directories() {
    let catalog = Catalog::new();
    let roms = [
        expect(
            MachineRoms,
            "galagao",
            Reason::Selected,
            "galagao.zip",
            &[Merged],
        ),
        expect(
            MachineRoms,
            "galaga",
            Reason::Parent,
            "galaga.zip",
            &[NonMerged],
        ),
    ];
    let mut expected = roms.to_vec();
    expected.push(expect(Samples, "galaga", Reason::Sample, "galaga.zip", &[]));
    for organization in [None, Some(ZipArchives)] {
        let organizations = Organizations {
            machine_chds: None,
            samples: organization,
        };
        assert_eq!(
            describe(&catalog.machine("galagao", organizations)),
            expected
        );
    }
    let mut expected = roms.to_vec();
    expected.extend([
        expect(Samples, "galaga", Reason::Sample, "galaga/bang.wav", &[]),
        expect(Samples, "galaga", Reason::Sample, "galaga/explode.wav", &[]),
    ]);
    let organizations = Organizations {
        machine_chds: None,
        samples: Some(Directories),
    };
    assert_eq!(
        describe(&catalog.machine("galagao", organizations)),
        expected
    );
}

#[test]
fn software_roms_with_parent_and_the_running_machine() {
    let catalog = Catalog::new();
    assert_eq!(
        describe(&catalog.software("console", "test_cart", "heroj", &["cart"])),
        [
            expect(
                SoftwareListRoms,
                "test_cart/heroj",
                Reason::Software,
                "test_cart/heroj.zip",
                &[Merged]
            ),
            expect(
                SoftwareListRoms,
                "test_cart/hero",
                Reason::SoftwareParent,
                "test_cart/hero.zip",
                &[NonMerged]
            ),
            // The console has no ROM of its own; its devices do.
            expect(MachineRoms, "sound", Reason::Device, "sound.zip", &[]),
            expect(MachineRoms, "subcpu", Reason::Device, "subcpu.zip", &[]),
        ]
    );
    // An undumped software has no archive to look for.
    assert_eq!(
        describe(&catalog.software("console", "test_cart", "lost", &["cart"]))[..1],
        [expect(
            MachineRoms,
            "sound",
            Reason::Device,
            "sound.zip",
            &[]
        )]
    );
}

#[test]
fn software_disks_of_the_mounted_parts_and_of_the_parent() {
    let catalog = Catalog::new();
    let devices = [
        expect(MachineRoms, "sound", Reason::Device, "sound.zip", &[]),
        expect(MachineRoms, "subcpu", Reason::Device, "subcpu.zip", &[]),
    ];
    let mut expected = vec![
        expect(
            SoftwareListChds,
            "test_cd/sagaj",
            Reason::Software,
            "test_cd/sagaj/sagaj disc 2.chd",
            &[],
        ),
        expect(
            SoftwareListChds,
            "test_cd/saga",
            Reason::SoftwareParent,
            "test_cd/saga/saga disc 2.chd",
            &[NonMerged],
        ),
    ];
    expected.extend(devices.clone());
    assert_eq!(
        describe(&catalog.software("console", "test_cd", "sagaj", &["cdrom2"])),
        expected
    );
    // A disk identical to the parent's may only exist in the parent's directory.
    let mut expected = vec![
        expect(
            SoftwareListChds,
            "test_cd/sagaj",
            Reason::Software,
            "test_cd/sagaj/saga disc 1.chd",
            &[Merged, Split],
        ),
        expect(
            SoftwareListChds,
            "test_cd/saga",
            Reason::SoftwareParent,
            "test_cd/saga/saga disc 1.chd",
            &[NonMerged],
        ),
    ];
    expected.extend(devices);
    assert_eq!(
        describe(&catalog.software("console", "test_cd", "sagaj", &["cdrom1"])),
        expected
    );
    // Without a selection, every part.
    assert_eq!(
        catalog
            .software("console", "test_cd", "saga", &[])
            .iter()
            .filter(|candidate| candidate.category == SoftwareListChds)
            .map(|candidate| candidate.segments.join("/"))
            .collect::<Vec<_>>(),
        [
            "test_cd/saga/saga disc 1.chd",
            "test_cd/saga/saga disc 2.chd"
        ]
    );
}

#[test]
fn unknown_targets_are_reported() {
    let catalog = Catalog::new();
    catalog.read(|connection| {
        assert!(
            plan(
                connection,
                &PlanTarget {
                    machine: "nothing",
                    software: None
                },
                Organizations::default()
            )
            .is_err()
        );
    });
}

#[test]
fn portable_names() {
    for name in ["pacman.zip", "saga disc 1.chd", "a-b_c.d+e"] {
        assert!(is_portable_name(name), "{name}");
    }
    for name in [
        "",
        "a:b",
        "a/b",
        "a\\b",
        ".hidden",
        "trailing.",
        "con",
        "NUL.zip",
        "a\u{1}",
    ] {
        assert!(!is_portable_name(name), "{name:?}");
    }
}

// ----------------------------------------------------------- plan building

pub(crate) const NAS: &str = "11111111111111111111111111111111";
const BACKUP: &str = "22222222222222222222222222222222";

pub(crate) fn connection(id: &str, name: &str, port: u16) -> RemoteConnection {
    RemoteConnection {
        id: id.to_owned(),
        name: name.to_owned(),
        provider: ProviderConfig::Sftp(SftpConfig {
            settings: SftpSettings {
                host: "127.0.0.1".to_owned(),
                port,
                username: "mame".to_owned(),
                private_key_path: testdata("client_ed25519"),
                host_key_verification: HostKeyVerification::Ymdb,
            },
            approved_host_key: None,
        }),
    }
}

fn connections() -> ConnectionsConfig {
    ConnectionsConfig {
        connections: vec![connection(NAS, "NAS", 22), connection(BACKUP, "Backup", 22)],
        ..ConnectionsConfig::default()
    }
}

fn source(
    connection: &str,
    base: &str,
    organization: CollectionOrganization,
    destination: &Path,
) -> CategorySource {
    CategorySource {
        enabled: true,
        connection_id: Some(connection.to_owned()),
        remote_base_path: Some(base.to_owned()),
        organization: Some(organization),
        destination: Some(LocalDestination::Custom {
            directory: destination.to_path_buf(),
        }),
    }
}

/// A local MAME layout: `mame/roms` and `mame/samples` are searched, and
/// downloads go to `downloads/...`.
struct Local {
    dir: TempDir,
}

impl Local {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        for sub in [
            "mame/roms",
            "mame/samples",
            "downloads/roms",
            "downloads/samples",
        ] {
            fs::create_dir_all(dir.path().join(sub)).expect("dirs");
        }
        Self { dir }
    }

    fn path(&self, relative: &str) -> PathBuf {
        self.dir.path().join(relative)
    }

    fn mame_dir(&self) -> PathBuf {
        self.path("mame")
    }

    fn search(&self) -> SearchDirs {
        SearchDirs::new(
            &self.mame_dir(),
            &[
                "roms".to_owned(),
                self.path("downloads/roms").display().to_string(),
            ],
            &["samples".to_owned()],
        )
    }

    fn write(&self, relative: &str, content: &[u8]) {
        let path = self.path(relative);
        fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
        fs::write(path, content).expect("write");
    }
}

pub(crate) fn zip_bytes(size: usize) -> Vec<u8> {
    let mut content = b"PK\x03\x04".to_vec();
    content.extend((0..size.saturating_sub(4)).map(|index| (index % 251) as u8));
    content
}

/// What a fake repository serves at a path.
#[derive(Clone)]
pub(crate) enum FakeFile {
    Data(Vec<u8>),
    /// Announces no size when opened.
    Unsized(Vec<u8>),
    /// Fails `stat`.
    Error(RemoteError),
    /// Sends the bytes, then the connection breaks.
    Broken(Vec<u8>),
    /// Never sends anything.
    Stalled,
}

#[derive(Clone)]
pub(crate) struct FakeRepository {
    files: Arc<HashMap<String, FakeFile>>,
}

impl RemoteRepository for FakeRepository {
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            list_directory: false,
            ranged_download: false,
        }
    }

    async fn stat(&self, path: &RemotePath) -> Result<RemoteMetadata, RemoteError> {
        // Directories are those containing a served file.
        let prefix = format!("{}/", path.as_str().trim_end_matches('/'));
        if !self.files.contains_key(path.as_str())
            && self.files.keys().any(|file| file.starts_with(&prefix))
        {
            return Ok(RemoteMetadata {
                kind: EntryKind::Directory,
                size: None,
                modified_at: None,
            });
        }
        let size = match self.files.get(path.as_str()) {
            None => {
                return Err(RemoteError::NotFound {
                    path: path.to_string(),
                });
            }
            Some(FakeFile::Error(error)) => return Err(error.clone()),
            Some(FakeFile::Data(data) | FakeFile::Unsized(data)) => data.len() as u64,
            Some(FakeFile::Broken(data)) => data.len() as u64 * 2,
            Some(FakeFile::Stalled) => 1000,
        };
        Ok(RemoteMetadata {
            kind: EntryKind::File,
            size: Some(size),
            modified_at: None,
        })
    }

    async fn list_directory(&self, _path: &RemotePath) -> Result<Vec<RemoteEntry>, RemoteError> {
        Err(RemoteError::Unsupported {
            operation: "directory listing".to_owned(),
        })
    }

    async fn open_download(
        &self,
        path: &RemotePath,
        _offset: u64,
    ) -> Result<Download, RemoteError> {
        let (size, reader): (Option<u64>, Pin<Box<dyn AsyncRead + Send>>) =
            match self.files.get(path.as_str()) {
                None | Some(FakeFile::Error(_)) => {
                    return Err(RemoteError::NotFound {
                        path: path.to_string(),
                    });
                }
                Some(FakeFile::Data(data)) => (
                    Some(data.len() as u64),
                    Box::pin(io::Cursor::new(data.clone())),
                ),
                Some(FakeFile::Unsized(data)) => (None, Box::pin(io::Cursor::new(data.clone()))),
                Some(FakeFile::Broken(data)) => (
                    Some(data.len() as u64 * 2),
                    Box::pin(BrokenReader(Some(data.clone()))),
                ),
                Some(FakeFile::Stalled) => (Some(1000), Box::pin(StalledReader)),
            };
        Ok(Download {
            size,
            offset: 0,
            reader,
        })
    }

    async fn close(self) {}
}

struct BrokenReader(Option<Vec<u8>>);

impl AsyncRead for BrokenReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match self.0.take() {
            Some(data) => {
                buf.put_slice(&data);
                Poll::Ready(Ok(()))
            }
            None => Poll::Ready(Err(io::Error::new(io::ErrorKind::ConnectionReset, "gone"))),
        }
    }
}

struct StalledReader;

impl AsyncRead for StalledReader {
    fn poll_read(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        _buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Poll::Pending
    }
}

#[derive(Default)]
pub(crate) struct FakeConnector {
    /// Files served by each connection id.
    files: HashMap<String, Arc<HashMap<String, FakeFile>>>,
    refused: HashMap<String, RemoteError>,
    connects: AtomicUsize,
}

impl FakeConnector {
    pub(crate) fn serving(connection: &str, files: &[(&str, FakeFile)]) -> Self {
        let mut connector = Self::default();
        connector.serve(connection, files);
        connector
    }

    fn serve(&mut self, connection: &str, files: &[(&str, FakeFile)]) {
        self.files.insert(
            connection.to_owned(),
            Arc::new(
                files
                    .iter()
                    .map(|(path, file)| ((*path).to_owned(), file.clone()))
                    .collect(),
            ),
        );
    }
}

impl Connector for FakeConnector {
    type Repository = FakeRepository;

    async fn connect(&self, connection: &RemoteConnection) -> Result<FakeRepository, RemoteError> {
        self.connects.fetch_add(1, Ordering::Relaxed);
        if let Some(error) = self.refused.get(&connection.id) {
            return Err(error.clone());
        }
        Ok(FakeRepository {
            files: self.files.get(&connection.id).cloned().unwrap_or_default(),
        })
    }
}

fn run<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime")
        .block_on(future)
}

/// Builds a whole plan for `candidates`.
fn build(
    local: &Local,
    candidates: Vec<Candidate>,
    sources: &RemoteSourcesConfig,
    connector: &FakeConnector,
) -> (
    Vec<PlanArtifact>,
    Vec<super::plan::ConnectionIssue>,
    super::plan::PlanSummary,
) {
    let connections = connections();
    let mut artifacts = build_artifacts(
        candidates,
        &local.mame_dir(),
        &local.search(),
        sources,
        &connections,
    );
    let issues = run(check_remote(&mut artifacts, &connections, connector));
    let summary = finish(&mut artifacts, &[]);
    (artifacts, issues, summary)
}

fn artifact<'a>(artifacts: &'a [PlanArtifact], id: &str) -> &'a PlanArtifact {
    artifacts
        .iter()
        .find(|artifact| artifact.id == id)
        .unwrap_or_else(|| panic!("no artifact {id}"))
}

fn warning_kind(artifact: &PlanArtifact) -> Option<WarningKind> {
    artifact.warning.as_ref().map(|warning| warning.kind)
}

fn sources(entries: &[(SourceCategory, CategorySource)]) -> RemoteSourcesConfig {
    RemoteSourcesConfig {
        categories: entries.iter().cloned().collect(),
        ..RemoteSourcesConfig::default()
    }
}

#[test]
fn present_files_are_not_downloaded_and_available_ones_are() {
    let catalog = Catalog::new();
    let local = Local::new();
    // The parent is already in a directory MAME searches (not a download destination).
    local.write("mame/roms/puckman.zip", &zip_bytes(64));
    let sources = sources(&[(
        MachineRoms,
        source(NAS, "/mame/roms", Split, &local.path("downloads/roms")),
    )]);
    let connector = FakeConnector::serving(
        NAS,
        &[("/mame/roms/pacman.zip", FakeFile::Data(zip_bytes(100)))],
    );
    let (artifacts, issues, summary) = build(
        &local,
        catalog.machine("pacman", Organizations::default()),
        &sources,
        &connector,
    );
    assert!(issues.is_empty());
    let pacman = artifact(&artifacts, "machineRoms:pacman.zip");
    assert_eq!(pacman.availability, Availability::RemoteAvailable);
    assert!(pacman.download);
    assert_eq!(pacman.remote_size, Some(100));
    assert_eq!(pacman.remote_path.as_deref(), Some("/mame/roms/pacman.zip"));
    assert_eq!(
        pacman.destination.as_deref(),
        Some(local.path("downloads/roms/pacman.zip").as_path())
    );
    assert_eq!(pacman.connection_id.as_deref(), Some(NAS));
    let puckman = artifact(&artifacts, "machineRoms:puckman.zip");
    assert_eq!(puckman.availability, Availability::Present);
    assert!(!puckman.download);
    assert_eq!(
        puckman.local.as_ref().map(|file| file.path.clone()),
        Some(local.path("mame/roms/puckman.zip"))
    );
    assert_eq!(puckman.warning, None);
    assert_eq!(artifacts.len(), 2);
    assert_eq!(
        (
            summary.present,
            summary.to_download,
            summary.not_found,
            summary.unresolved,
            summary.warnings,
            summary.download_bytes
        ),
        (1, 1, 0, 0, 0, 100)
    );
}

#[test]
fn missing_candidates_are_warnings_qualified_by_the_organization() {
    let catalog = Catalog::new();
    let local = Local::new();
    let connector = FakeConnector::serving(
        NAS,
        &[("/mame/roms/puckman.zip", FakeFile::Data(zip_bytes(10)))],
    );
    // Merged: the clone archive is expected to be absent.
    let merged = sources(&[(
        MachineRoms,
        source(NAS, "/mame/roms", Merged, &local.path("downloads/roms")),
    )]);
    let (artifacts, _, summary) = build(
        &local,
        catalog.machine("pacman", Organizations::default()),
        &merged,
        &connector,
    );
    let pacman = artifact(&artifacts, "machineRoms:pacman.zip");
    assert_eq!(pacman.availability, Availability::RemoteMissing);
    assert_eq!(warning_kind(pacman), Some(WarningKind::Informational));
    assert!(artifact(&artifacts, "machineRoms:puckman.zip").download);
    assert_eq!(
        (summary.not_found, summary.warnings, summary.to_download),
        (1, 0, 1)
    );

    // Split: the same absence is an availability warning, and does not
    // prevent downloading the parent.
    let split = sources(&[(
        MachineRoms,
        source(NAS, "/mame/roms", Split, &local.path("downloads/roms")),
    )]);
    let (artifacts, _, summary) = build(
        &local,
        catalog.machine("pacman", Organizations::default()),
        &split,
        &connector,
    );
    let pacman = artifact(&artifacts, "machineRoms:pacman.zip");
    assert_eq!(warning_kind(pacman), Some(WarningKind::Availability));
    assert!(artifact(&artifacts, "machineRoms:puckman.zip").download);
    assert_eq!(
        (summary.not_found, summary.warnings, summary.to_download),
        (1, 1, 1)
    );

    // A missing parent in a non-merged collection is expected.
    let connector = FakeConnector::serving(
        NAS,
        &[("/mame/roms/pacman.zip", FakeFile::Data(zip_bytes(10)))],
    );
    let non_merged = sources(&[(
        MachineRoms,
        source(NAS, "/mame/roms", NonMerged, &local.path("downloads/roms")),
    )]);
    let (artifacts, _, _) = build(
        &local,
        catalog.machine("pacman", Organizations::default()),
        &non_merged,
        &connector,
    );
    let puckman = artifact(&artifacts, "machineRoms:puckman.zip");
    assert_eq!(puckman.availability, Availability::RemoteMissing);
    assert_eq!(warning_kind(puckman), Some(WarningKind::Informational));
}

#[test]
fn categories_without_source_are_configuration_warnings() {
    let catalog = Catalog::new();
    let local = Local::new();
    let sources = sources(&[(
        MachineRoms,
        source(NAS, "/mame/roms", Merged, &local.path("downloads/roms")),
    )]);
    let connector = FakeConnector::serving(
        NAS,
        &[
            ("/mame/roms/kinst2.zip", FakeFile::Data(zip_bytes(10))),
            ("/mame/roms/kinst.zip", FakeFile::Data(zip_bytes(10))),
        ],
    );
    let (artifacts, issues, summary) = build(
        &local,
        catalog.machine("kinst2", Organizations::default()),
        &sources,
        &connector,
    );
    assert!(issues.is_empty());
    let disk = artifact(&artifacts, "machineChds:kinst2/kinst2.chd");
    assert_eq!(disk.availability, Availability::Missing);
    assert_eq!(warning_kind(disk), Some(WarningKind::Configuration));
    assert_eq!(disk.remote_path, None);
    assert_eq!(summary.to_download, 2);
    assert_eq!(summary.unresolved, 2);

    // A source whose connection was removed.
    let mut orphan = source(NAS, "/mame/roms", Merged, &local.path("downloads/roms"));
    orphan.connection_id = Some("33333333333333333333333333333333".to_owned());
    let (artifacts, _, _) = build(
        &local,
        catalog.machine("solo", Organizations::default()),
        &sources_with(MachineRoms, orphan),
        &connector,
    );
    let solo = artifact(&artifacts, "machineRoms:solo.zip");
    assert_eq!(solo.availability, Availability::Missing);
    assert_eq!(warning_kind(solo), Some(WarningKind::Configuration));
    assert!(!solo.download);
    // Nothing was looked up for it.
    assert_eq!(connector.connects.load(Ordering::Relaxed), 1);
}

fn sources_with(category: SourceCategory, source: CategorySource) -> RemoteSourcesConfig {
    sources(&[(category, source)])
}

#[test]
fn technical_failures_leave_files_unresolved() {
    let catalog = Catalog::new();
    let local = Local::new();
    let sources = sources(&[
        (
            MachineRoms,
            source(NAS, "/mame/roms", Split, &local.path("downloads/roms")),
        ),
        (
            MachineChds,
            source(BACKUP, "/mame/chds", Split, &local.path("downloads/roms")),
        ),
    ]);
    let mut connector = FakeConnector::serving(
        NAS,
        &[
            (
                "/mame/roms/kinst2.zip",
                FakeFile::Error(RemoteError::PermissionDenied {
                    path: "/mame/roms/kinst2.zip".to_owned(),
                }),
            ),
            ("/mame/roms/kinst.zip", FakeFile::Data(zip_bytes(10))),
        ],
    );
    connector
        .refused
        .insert(BACKUP.to_owned(), RemoteError::PassphraseRequired);
    let (artifacts, issues, summary) = build(
        &local,
        catalog.machine("kinst2", Organizations::default()),
        &sources,
        &connector,
    );
    // A refused file is not a missing file.
    let kinst2 = artifact(&artifacts, "machineRoms:kinst2.zip");
    assert_eq!(kinst2.availability, Availability::Unresolved);
    assert_eq!(warning_kind(kinst2), Some(WarningKind::Unresolved));
    assert!(matches!(
        kinst2.remote_error,
        Some(RemoteError::PermissionDenied { .. })
    ));
    assert!(artifact(&artifacts, "machineRoms:kinst.zip").download);
    // A connection that needs a passphrase is reported once, its files unresolved.
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].connection_id, BACKUP);
    assert_eq!(issues[0].error, RemoteError::PassphraseRequired);
    for id in [
        "machineChds:kinst2/kinst2.chd",
        "machineChds:kinst/kinst.chd",
    ] {
        let disk = artifact(&artifacts, id);
        assert_eq!(disk.availability, Availability::Unresolved);
        assert_eq!(disk.remote_error, Some(RemoteError::PassphraseRequired));
    }
    assert_eq!(
        (summary.unresolved, summary.not_found, summary.to_download),
        (3, 0, 1)
    );

    // After a timeout, the remaining files are not looked up again.
    let connector = FakeConnector::serving(
        NAS,
        &[
            (
                "/mame/roms/kinst2.zip",
                FakeFile::Error(RemoteError::TimedOut),
            ),
            ("/mame/roms/kinst.zip", FakeFile::Data(zip_bytes(10))),
        ],
    );
    let (artifacts, _, _) = build(
        &local,
        catalog.machine("kinst2", Organizations::default()),
        &sources_with(
            MachineRoms,
            source(NAS, "/mame/roms", Split, &local.path("downloads/roms")),
        ),
        &connector,
    );
    let kinst = artifact(&artifacts, "machineRoms:kinst.zip");
    assert_eq!(kinst.availability, Availability::Unresolved);
    assert_eq!(kinst.remote_error, Some(RemoteError::TimedOut));
}

#[test]
fn destination_conflicts_are_never_downloaded() {
    let catalog = Catalog::new();
    let local = Local::new();
    // Samples and ROMs downloaded into the same directory: galaga.zip twice.
    let shared = local.path("downloads/roms");
    let sources = sources(&[
        (MachineRoms, source(NAS, "/mame/roms", Split, &shared)),
        (Samples, source(NAS, "/mame/samples", ZipArchives, &shared)),
    ]);
    let connector = FakeConnector::serving(
        NAS,
        &[
            ("/mame/roms/galaga.zip", FakeFile::Data(zip_bytes(10))),
            ("/mame/roms/galagao.zip", FakeFile::Data(zip_bytes(10))),
            ("/mame/samples/galaga.zip", FakeFile::Data(zip_bytes(20))),
        ],
    );
    let (artifacts, _, summary) = build(
        &local,
        catalog.machine("galagao", Organizations::default()),
        &sources,
        &connector,
    );
    for id in ["machineRoms:galaga.zip", "samples:galaga.zip"] {
        let conflicting = artifact(&artifacts, id);
        assert_eq!(conflicting.availability, Availability::RemoteAvailable);
        assert!(!conflicting.download, "{id}");
        assert_eq!(warning_kind(conflicting), Some(WarningKind::Conflict));
    }
    assert!(artifact(&artifacts, "machineRoms:galagao.zip").download);
    assert_eq!((summary.to_download, summary.warnings), (1, 2));
}

#[test]
fn checks_local_files_shallowly() {
    let catalog = Catalog::new();
    let local = Local::new();
    local.write("mame/roms/solo.zip", b"not a zip archive");
    local.write("mame/roms/sound.7z", b"7z\xBC\xAF\x27\x1C rest");
    local.write("mame/roms/subcpu.zip", b"");
    let (artifacts, _, summary) = build(
        &local,
        catalog.machine("solo", Organizations::default()),
        &RemoteSourcesConfig::default(),
        &FakeConnector::default(),
    );
    let solo = artifact(&artifacts, "machineRoms:solo.zip");
    assert_eq!(solo.availability, Availability::Present);
    assert_eq!(warning_kind(solo), Some(WarningKind::Informational));
    let sound = artifact(&artifacts, "machineRoms:sound.zip");
    assert_eq!(sound.availability, Availability::Present);
    assert_eq!(
        sound.local.as_ref().map(|file| file.path.clone()),
        Some(local.path("mame/roms/sound.7z"))
    );
    assert_eq!(sound.warning, None);
    let subcpu = artifact(&artifacts, "machineRoms:subcpu.zip");
    assert_eq!(subcpu.availability, Availability::Present);
    assert_eq!(warning_kind(subcpu), Some(WarningKind::Informational));
    assert_eq!(summary.present, 3);
    // A directory with the name of a file is not the file.
    fs::create_dir_all(local.path("mame/roms/kinst.zip")).expect("dir");
    assert_eq!(find(&local.search().rom, &["kinst.zip".to_owned()]), None);
}

#[test]
fn unusable_names_are_unresolved() {
    let catalog = Catalog::new();
    let local = Local::new();
    let (artifacts, _, summary) = build(
        &local,
        catalog.machine("badname", Organizations::default()),
        &sources_with(
            MachineChds,
            source(NAS, "/mame/chds", Merged, &local.path("downloads/roms")),
        ),
        &FakeConnector::serving(NAS, &[]),
    );
    let disk = artifact(&artifacts, "machineChds:badname/a:b.chd");
    assert_eq!(disk.availability, Availability::Unresolved);
    assert_eq!(warning_kind(disk), Some(WarningKind::Unresolved));
    assert_eq!(disk.remote_path, None);
    assert_eq!(summary.to_download, 0);
}

#[test]
fn plan_warnings_count_in_the_summary() {
    let summary = finish(
        &mut [],
        &[
            PlanWarning {
                kind: WarningKind::Configuration,
                message: "x".to_owned(),
            },
            PlanWarning {
                kind: WarningKind::Informational,
                message: "y".to_owned(),
            },
        ],
    );
    assert_eq!(summary.warnings, 1);
}

// --------------------------------------------------------------- downloads

fn task(id: &str, connection_id: &str, remote: &str, destination: PathBuf) -> DownloadTask {
    DownloadTask {
        artifact_id: id.to_owned(),
        connection: connection(connection_id, connection_id, 22),
        remote_path: RemotePath::parse(remote).expect("remote path"),
        destination,
        expected_size: None,
    }
}

fn statuses(progress: &Progress) -> Vec<TransferStatus> {
    progress
        .snapshot()
        .iter()
        .map(|state| state.status)
        .collect()
}

/// Files in a directory tree, relative, sorted.
fn tree(root: &Path) -> Vec<String> {
    let mut files = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir).expect("read dir") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                pending.push(path);
            } else {
                files.push(
                    path.strip_prefix(root)
                        .expect("prefix")
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }
    files.sort();
    files
}

#[test]
fn downloads_into_temporary_files_and_finalizes() {
    let dir = tempfile::tempdir().expect("temp dir");
    let big = zip_bytes(5 * 1024 * 1024 + 7);
    let connector = FakeConnector::serving(
        NAS,
        &[
            ("/roms/pacman.zip", FakeFile::Data(zip_bytes(1000))),
            ("/chds/kinst/kinst.chd", FakeFile::Data(big.clone())),
        ],
    );
    let tasks = [
        task(
            "machineRoms:pacman.zip",
            NAS,
            "/roms/pacman.zip",
            dir.path().join("roms/pacman.zip"),
        ),
        task(
            "machineChds:kinst/kinst.chd",
            NAS,
            "/chds/kinst/kinst.chd",
            dir.path().join("roms/kinst/kinst.chd"),
        ),
    ];
    let progress = Progress::new(&tasks, &[]);
    run(download::run(
        &tasks,
        &connector,
        &progress,
        &AtomicBool::new(false),
    ));
    assert_eq!(
        statuses(&progress),
        [TransferStatus::Downloaded, TransferStatus::Downloaded]
    );
    let snapshot = progress.snapshot();
    assert_eq!(snapshot[1].bytes_transferred, big.len() as u64);
    assert_eq!(snapshot[1].size, Some(big.len() as u64));
    assert_eq!(
        fs::read(dir.path().join("roms/kinst/kinst.chd")).expect("read"),
        big
    );
    // One connection for both files; no temporary file left.
    assert_eq!(connector.connects.load(Ordering::Relaxed), 1);
    assert_eq!(
        tree(dir.path()),
        ["roms/kinst/kinst.chd", "roms/pacman.zip"]
    );
}

#[test]
fn never_replaces_an_existing_file() {
    let dir = tempfile::tempdir().expect("temp dir");
    let destination = dir.path().join("pacman.zip");
    fs::write(&destination, b"user file").expect("write");
    let connector =
        FakeConnector::serving(NAS, &[("/roms/pacman.zip", FakeFile::Data(zip_bytes(10)))]);
    let tasks = [task("a", NAS, "/roms/pacman.zip", destination.clone())];
    let progress = Progress::new(&tasks, &[]);
    run(download::run(
        &tasks,
        &connector,
        &progress,
        &AtomicBool::new(false),
    ));
    assert_eq!(statuses(&progress), [TransferStatus::Skipped]);
    assert_eq!(fs::read(&destination).expect("read"), b"user file");
    assert_eq!(connector.connects.load(Ordering::Relaxed), 0);
}

#[test]
fn failed_transfers_clean_up_and_do_not_stop_the_others() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut connector = FakeConnector::serving(
        NAS,
        &[
            ("/roms/broken.zip", FakeFile::Broken(zip_bytes(100))),
            ("/roms/short.zip", FakeFile::Unsized(zip_bytes(10))),
            ("/roms/ok.zip", FakeFile::Data(zip_bytes(10))),
        ],
    );
    connector
        .refused
        .insert(BACKUP.to_owned(), RemoteError::AuthenticationFailed);
    // A file where a directory is needed: the destination cannot be created.
    fs::write(dir.path().join("blocker"), b"file").expect("write");
    let mut short = task(
        "short",
        NAS,
        "/roms/short.zip",
        dir.path().join("out/short.zip"),
    );
    short.expected_size = Some(11);
    let tasks = [
        task(
            "broken",
            NAS,
            "/roms/broken.zip",
            dir.path().join("out/new dir/broken.zip"),
        ),
        short,
        task(
            "unwritable",
            NAS,
            "/roms/ok.zip",
            dir.path().join("blocker/ok.zip"),
        ),
        task(
            "missing",
            NAS,
            "/roms/gone.zip",
            dir.path().join("out/gone.zip"),
        ),
        task(
            "refused1",
            BACKUP,
            "/roms/ok.zip",
            dir.path().join("out/r1.zip"),
        ),
        task(
            "refused2",
            BACKUP,
            "/roms/ok.zip",
            dir.path().join("out/r2.zip"),
        ),
        task("ok", NAS, "/roms/ok.zip", dir.path().join("out/ok.zip")),
    ];
    let progress = Progress::new(&tasks, &[]);
    run(download::run(
        &tasks,
        &connector,
        &progress,
        &AtomicBool::new(false),
    ));
    let snapshot = progress.snapshot();
    assert_eq!(
        statuses(&progress),
        [
            TransferStatus::Failed,
            TransferStatus::Failed,
            TransferStatus::Failed,
            TransferStatus::Failed,
            TransferStatus::Failed,
            TransferStatus::Failed,
            TransferStatus::Downloaded,
        ]
    );
    assert_eq!(
        snapshot[0].error,
        Some(TransferError::Remote {
            error: RemoteError::ConnectionLost
        })
    );
    assert_eq!(
        snapshot[1].error,
        Some(TransferError::SizeMismatch {
            expected: 11,
            received: 10
        })
    );
    assert!(matches!(
        snapshot[2].error,
        Some(TransferError::Local { .. })
    ));
    assert!(matches!(
        snapshot[3].error,
        Some(TransferError::Remote {
            error: RemoteError::NotFound { .. }
        })
    ));
    for state in &snapshot[4..6] {
        assert_eq!(
            state.error,
            Some(TransferError::Remote {
                error: RemoteError::AuthenticationFailed
            })
        );
    }
    // Only the completed file remains: no temporary file, no directory
    // created for a failed transfer.
    assert_eq!(tree(dir.path()), ["blocker", "out/ok.zip"]);
    // NAS: initial connection, reconnection after the broken stream, then
    // again after switching back from BACKUP (one attempt for it).
    assert_eq!(connector.connects.load(Ordering::Relaxed), 4);
}

#[test]
fn cancellation_stops_waiting_transfers_and_keeps_completed_files() {
    let dir = tempfile::tempdir().expect("temp dir");
    let connector = FakeConnector::serving(
        NAS,
        &[
            ("/roms/ok.zip", FakeFile::Data(zip_bytes(10))),
            ("/roms/stalled.zip", FakeFile::Stalled),
        ],
    );
    let tasks = [
        task("ok", NAS, "/roms/ok.zip", dir.path().join("ok.zip")),
        task(
            "stalled",
            NAS,
            "/roms/stalled.zip",
            dir.path().join("sub/stalled.zip"),
        ),
        task("later", NAS, "/roms/ok.zip", dir.path().join("later.zip")),
    ];
    let progress = Progress::new(&tasks, &[]);
    let cancel = AtomicBool::new(false);
    run(async {
        let canceller = async {
            while progress.snapshot()[1].status != TransferStatus::Downloading {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            tokio::time::sleep(Duration::from_millis(150)).await;
            cancel.store(true, Ordering::Relaxed);
        };
        tokio::join!(
            download::run(&tasks, &connector, &progress, &cancel),
            canceller
        );
    });
    assert_eq!(
        statuses(&progress),
        [
            TransferStatus::Downloaded,
            TransferStatus::Cancelled,
            TransferStatus::Cancelled
        ]
    );
    assert_eq!(tree(dir.path()), ["ok.zip"]);
    assert!(!temp_path(&dir.path().join("sub/stalled.zip")).exists());
}

#[test]
fn a_retry_keeps_completed_files_and_replaces_stale_temporary_files() {
    let dir = tempfile::tempdir().expect("temp dir");
    let connector = FakeConnector::serving(
        NAS,
        &[
            ("/roms/a.zip", FakeFile::Data(zip_bytes(10))),
            ("/roms/b.zip", FakeFile::Data(zip_bytes(20))),
        ],
    );
    let tasks = [
        task("a", NAS, "/roms/a.zip", dir.path().join("a.zip")),
        task("b", NAS, "/roms/b.zip", dir.path().join("b.zip")),
    ];
    // An earlier attempt downloaded `a` and failed on `b`, leaving a
    // temporary file behind (as when YMDB is closed during a transfer).
    let mut previous = Progress::new(&tasks, &[]).snapshot();
    previous[0].status = TransferStatus::Downloaded;
    previous[1].status = TransferStatus::Failed;
    fs::write(dir.path().join("a.zip"), zip_bytes(10)).expect("write");
    fs::write(temp_path(&dir.path().join("b.zip")), b"stale partial data").expect("write");
    let progress = Progress::new(&tasks, &previous);
    assert_eq!(
        statuses(&progress),
        [TransferStatus::Downloaded, TransferStatus::Pending]
    );
    run(download::run(
        &tasks,
        &connector,
        &progress,
        &AtomicBool::new(false),
    ));
    assert_eq!(
        statuses(&progress),
        [TransferStatus::Downloaded, TransferStatus::Downloaded]
    );
    assert_eq!(
        fs::read(dir.path().join("b.zip")).expect("read"),
        zip_bytes(20)
    );
    assert_eq!(tree(dir.path()), ["a.zip", "b.zip"]);
}

// --------------------------------------------------- through the SFTP server

pub(crate) fn approved(host_key: &str) -> ApprovedHostKey {
    let key = private_key(host_key).public_key().clone();
    ApprovedHostKey {
        algorithm: key.algorithm().as_str().to_owned(),
        fingerprint: crate::remote::sftp::fingerprint(&key),
        public_key: key.to_openssh().expect("openssh"),
        approved_at: 1,
    }
}

pub(crate) fn sftp_connector() -> ProviderConnector {
    ProviderConnector {
        defaults: ConnectOptions {
            timeouts: Timeouts {
                connect: Duration::from_secs(10),
                request: Duration::from_secs(5),
            },
            ..ConnectOptions::default()
        },
        per_connection: HashMap::new(),
    }
}

/// Modification times and contents of every served file.
fn snapshot_tree(root: &Path) -> Vec<(String, Vec<u8>, std::time::SystemTime)> {
    tree(root)
        .into_iter()
        .map(|relative| {
            let path = root.join(&relative);
            let modified = fs::metadata(&path)
                .and_then(|metadata| metadata.modified())
                .expect("modified");
            (relative, fs::read(&path).expect("read"), modified)
        })
        .collect()
}

#[tokio::test]
async fn plans_and_downloads_through_the_sftp_provider() {
    let catalog = Catalog::new();
    let local = Local::new();
    let served = tempfile::tempdir().expect("temp dir");
    let write = |relative: &str, content: &[u8]| {
        let path = served.path().join(relative);
        fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
        fs::write(path, content).expect("write");
    };
    write("mame/roms/kinst2.zip", &zip_bytes(3000));
    write("mame/roms/kinst.zip", &zip_bytes(2000));
    let disk = {
        let mut content = b"MComprHD".to_vec();
        content.extend(zip_bytes(3 * 1024 * 1024));
        content
    };
    write("mame/chds/kinst/kinst.chd", &disk);
    // kinst2/kinst2.chd is not on the server.
    let before = snapshot_tree(served.path());

    let server = TestServer::start(ServerOptions {
        host_key: "host_a",
        authorized: vec!["client_ed25519"],
        root: served.path().to_path_buf(),
        sftp: true,
    })
    .await;
    let mut nas = connection(NAS, "NAS", server.port);
    let ProviderConfig::Sftp(config) = &mut nas.provider;
    config.approved_host_key = Some(approved("host_a"));
    let connections = ConnectionsConfig {
        connections: vec![nas],
        ..ConnectionsConfig::default()
    };
    let destination = local.path("downloads/roms");
    let sources = sources(&[
        (MachineRoms, source(NAS, "/mame/roms", Split, &destination)),
        (MachineChds, source(NAS, "/mame/chds", Split, &destination)),
    ]);
    let candidates = catalog.machine(
        "kinst2",
        Organizations {
            machine_chds: Some(Split),
            samples: None,
        },
    );
    let mut artifacts = build_artifacts(
        candidates,
        &local.mame_dir(),
        &local.search(),
        &sources,
        &connections,
    );
    let connector = sftp_connector();
    let issues = check_remote(&mut artifacts, &connections, &connector).await;
    assert!(issues.is_empty(), "{issues:?}");
    let summary = finish(&mut artifacts, &[]);
    assert_eq!(
        artifacts
            .iter()
            .map(|artifact| (artifact.id.as_str(), artifact.availability))
            .collect::<Vec<_>>(),
        [
            ("machineRoms:kinst2.zip", Availability::RemoteAvailable),
            ("machineRoms:kinst.zip", Availability::RemoteAvailable),
            ("machineChds:kinst2/kinst2.chd", Availability::RemoteMissing),
            ("machineChds:kinst/kinst.chd", Availability::RemoteAvailable),
        ]
    );
    assert_eq!(
        warning_kind(artifact(&artifacts, "machineChds:kinst2/kinst2.chd")),
        Some(WarningKind::Availability)
    );
    assert_eq!(summary.to_download, 3);
    assert_eq!(summary.download_bytes, 3000 + 2000 + disk.len() as u64);

    let tasks: Vec<DownloadTask> = artifacts
        .iter()
        .filter(|artifact| artifact.download)
        .map(|artifact| DownloadTask {
            artifact_id: artifact.id.clone(),
            connection: connections.connections[0].clone(),
            remote_path: RemotePath::parse(artifact.remote_path.as_deref().expect("remote"))
                .expect("path"),
            destination: artifact.destination.clone().expect("destination"),
            expected_size: artifact.remote_size,
        })
        .collect();
    let progress = Progress::new(&tasks, &[]);
    download::run(&tasks, &connector, &progress, &AtomicBool::new(false)).await;
    assert_eq!(
        statuses(&progress),
        [TransferStatus::Downloaded; 3],
        "{:?}",
        progress.snapshot()
    );
    assert_eq!(
        tree(&destination),
        ["kinst.zip", "kinst/kinst.chd", "kinst2.zip"]
    );
    assert_eq!(
        fs::read(destination.join("kinst/kinst.chd")).expect("read"),
        disk
    );
    // The repository was only read.
    assert_eq!(snapshot_tree(served.path()), before);

    // The downloaded files are now present for a new plan.
    let mut artifacts = build_artifacts(
        catalog.machine("kinst2", Organizations::default()),
        &local.mame_dir(),
        &local.search(),
        &sources,
        &connections,
    );
    check_remote(&mut artifacts, &connections, &connector).await;
    let summary = finish(&mut artifacts, &[]);
    assert_eq!((summary.present, summary.to_download), (3, 0));
}

#[tokio::test]
async fn an_interrupted_sftp_transfer_fails_without_a_partial_file() {
    let served = tempfile::tempdir().expect("temp dir");
    let size = 24 * 1024 * 1024;
    fs::write(served.path().join("big.chd"), zip_bytes(size)).expect("write");
    let server = TestServer::start(ServerOptions {
        host_key: "host_a",
        authorized: vec!["client_ed25519"],
        root: served.path().to_path_buf(),
        sftp: true,
    })
    .await;
    let mut nas = connection(NAS, "NAS", server.port);
    let ProviderConfig::Sftp(config) = &mut nas.provider;
    config.approved_host_key = Some(approved("host_a"));
    let local = tempfile::tempdir().expect("temp dir");
    let destination = local.path().join("big/big.chd");
    let tasks = [DownloadTask {
        artifact_id: "big".to_owned(),
        connection: nas,
        remote_path: RemotePath::parse("/big.chd").expect("path"),
        destination: destination.clone(),
        expected_size: Some(size as u64),
    }];
    let progress = Progress::new(&tasks, &[]);
    let connector = sftp_connector();
    let cancel = AtomicBool::new(false);
    let interrupt = async {
        while progress.snapshot()[0].bytes_transferred == 0 {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        server.disconnect_all().await;
    };
    tokio::join!(
        download::run(&tasks, &connector, &progress, &cancel),
        interrupt
    );
    let state = &progress.snapshot()[0];
    assert_eq!(state.status, TransferStatus::Failed, "{state:?}");
    assert!(state.bytes_transferred < size as u64);
    assert!(!destination.exists());
    assert!(tree(local.path()).is_empty());
}

#[test]
fn a_missing_base_path_is_a_configuration_warning_not_missing_files() {
    let catalog = Catalog::new();
    let local = Local::new();
    // The files exist under /files/..., as an SFTP service shows them; the
    // source uses the shell path /volume1/files/... (Synology).
    let connector = FakeConnector::serving(
        NAS,
        &[
            ("/files/mame/roms/pacman.zip", FakeFile::Data(zip_bytes(10))),
            (
                "/files/mame/roms/puckman.zip",
                FakeFile::Data(zip_bytes(10)),
            ),
        ],
    );
    let wrong = sources_with(
        MachineRoms,
        source(
            NAS,
            "/volume1/files/mame/roms",
            Split,
            &local.path("downloads/roms"),
        ),
    );
    let (artifacts, issues, summary) = build(
        &local,
        catalog.machine("pacman", Organizations::default()),
        &wrong,
        &connector,
    );
    assert!(issues.is_empty());
    for artifact in &artifacts {
        assert_eq!(
            artifact.availability,
            Availability::Unresolved,
            "{}",
            artifact.id
        );
        assert_eq!(warning_kind(artifact), Some(WarningKind::Configuration));
        assert!(
            artifact
                .warning
                .as_ref()
                .is_some_and(|warning| warning.message.contains("/volume1/files/mame/roms"))
        );
    }
    assert_eq!(
        (summary.not_found, summary.unresolved, summary.to_download),
        (0, 2, 0)
    );

    // With the path as the SFTP service shows it, both are found.
    let right = sources_with(
        MachineRoms,
        source(
            NAS,
            "/files/mame/roms",
            Split,
            &local.path("downloads/roms"),
        ),
    );
    let (_, _, summary) = build(
        &local,
        catalog.machine("pacman", Organizations::default()),
        &right,
        &connector,
    );
    assert_eq!(summary.to_download, 2);
}
