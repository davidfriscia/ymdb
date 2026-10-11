//! Remote connection and source command tests without Tauri: connections
//! shared by installations, reference checks, per-installation isolation,
//! destinations and the host key approval flow against the test SSH server.

use std::fs;
use std::path::PathBuf;

use tempfile::TempDir;

use super::*;
use crate::installations::AppLayout;
use crate::launcher::config::LaunchConfig;
use crate::mame::validation::{ValidationReport, ValidationStatus};
use crate::mame::version::MameVersion;
use crate::remote::connections::{HostKeyVerification, ProviderInput, SftpSettings};
use crate::remote::sources::CategorySource;
use crate::remote::test_server::{ServerOptions, TestServer, testdata};

struct App {
    dir: TempDir,
    installations: InstallationsState,
    remote: RemoteState,
}

impl App {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        let config = dir.path().join("config ✓");
        let layout = AppLayout::new(&config, &dir.path().join("data"));
        let mut remote = RemoteState::new(Some(&config));
        // Never read the developer's own known_hosts.
        remote.known_hosts = None;
        Self {
            installations: InstallationsState::new(Some(layout)),
            remote,
            dir,
        }
    }

    /// Registers a fake executable, with an optional `mame.ini` next to it.
    fn add(&self, folder: &str, release: &str, ini: Option<&str>) -> String {
        let mame_dir = self.dir.path().join("MAME ✓").join(folder);
        fs::create_dir_all(&mame_dir).expect("dir");
        let path = mame_dir.join("mame.exe");
        fs::write(&path, format!("binary {folder}")).expect("write");
        if let Some(ini) = ini {
            fs::write(mame_dir.join("mame.ini"), ini).expect("ini");
        }
        self.installations
            .register(&ValidationReport {
                executable_path: path,
                status: ValidationStatus::Valid,
                version: Some(MameVersion {
                    release: release.to_owned(),
                    revision: format!("mame{}", release.replace('.', "")),
                }),
                output: None,
                checked_at: 1,
            })
            .expect("register")
    }

    fn mame_dir(&self, folder: &str) -> PathBuf {
        self.dir.path().join("MAME ✓").join(folder)
    }

    fn connection(&self, name: &str, port: u16, key: &str) -> String {
        let before: Vec<_> = self
            .remote
            .load()
            .expect("load")
            .connections
            .into_iter()
            .map(|connection| connection.id)
            .collect();
        let view = save_connection(
            &self.remote,
            ConnectionInput {
                id: None,
                name: name.to_owned(),
                provider: ProviderInput::Sftp(SftpSettings {
                    host: "127.0.0.1".to_owned(),
                    port,
                    username: "mame".to_owned(),
                    private_key_path: testdata(key),
                    host_key_verification: HostKeyVerification::Ymdb,
                }),
            },
        )
        .expect("save connection");
        view.connections
            .into_iter()
            .map(|summary| summary.connection.id)
            .find(|id| !before.contains(id))
            .expect("new id")
    }
}

fn enabled(connection: &str, path: &str, destination: LocalDestination) -> CategorySource {
    CategorySource {
        enabled: true,
        connection_id: Some(connection.to_owned()),
        remote_base_path: Some(path.to_owned()),
        organization: Some(CollectionOrganization::Merged),
        destination: Some(destination),
    }
}

fn mame_directory(entry: &str) -> LocalDestination {
    LocalDestination::MameDirectory {
        directory: entry.to_owned(),
    }
}

#[test]
fn shares_connections_between_installations_and_refuses_orphans() {
    let app = App::new();
    let old = app.add("0.279", "0.279", None);
    let new = app.add("0.280", "0.280", None);
    let nas = app.connection("NAS", 22, "client_ed25519");
    let backup = app.connection("Backup", 2222, "client_ed25519");

    // One connection for several categories of two installations, with
    // different remote paths and destinations.
    let mut config = RemoteSourcesConfig::default();
    config.categories.insert(
        SourceCategory::MachineRoms,
        enabled(&nas, "/volume1/mame/0279/roms", mame_directory("roms")),
    );
    save_sources(&app.installations, &app.remote, &old, config).expect("old");
    let mut config = RemoteSourcesConfig::default();
    config.categories.insert(
        SourceCategory::MachineRoms,
        enabled(&nas, "/volume1/mame/0280/roms", mame_directory("roms")),
    );
    config.categories.insert(
        SourceCategory::MachineChds,
        enabled(&nas, "/volume1/mame/0280/chds", mame_directory("roms")),
    );
    save_sources(&app.installations, &app.remote, &new, config).expect("new");

    // Saving one installation never changes the other.
    let old_view = load_sources(&app.installations, &app.remote, &old).expect("load");
    assert_eq!(old_view.config.categories.len(), 1);
    assert_eq!(
        old_view.config.categories[&SourceCategory::MachineRoms]
            .remote_base_path
            .as_deref(),
        Some("/volume1/mame/0279/roms")
    );
    assert_eq!(old_view.connections.len(), 2);
    assert_eq!(old_view.connections[0].provider_type, ProviderType::Sftp);

    // The connection is in use: removal is refused and lists the references.
    let Err(CommandError::ConnectionInUse { references }) =
        remove_connection(&app.remote, &app.installations, &nas)
    else {
        panic!("removal should be refused");
    };
    let mut found: Vec<_> = references
        .iter()
        .map(|reference| (reference.installation_id.as_str(), reference.category))
        .collect();
    found.sort();
    let mut expected = vec![
        (old.as_str(), SourceCategory::MachineRoms),
        (new.as_str(), SourceCategory::MachineRoms),
        (new.as_str(), SourceCategory::MachineChds),
    ];
    expected.sort();
    assert_eq!(found, expected);
    assert!(
        references
            .iter()
            .any(|reference| reference.installation_label == "MAME 0.280")
    );
    assert!(app.remote.load().expect("load").get(&nas).is_some());

    // Unused connections can be removed; sources cannot point to them anymore.
    remove_connection(&app.remote, &app.installations, &backup).expect("remove unused");
    let mut config = RemoteSourcesConfig::default();
    config.categories.insert(
        SourceCategory::Samples,
        CategorySource {
            organization: Some(CollectionOrganization::ZipArchives),
            ..enabled(&backup, "/samples", mame_directory("samples"))
        },
    );
    assert!(matches!(
        save_sources(&app.installations, &app.remote, &old, config),
        Err(CommandError::InvalidRequest { .. })
    ));
    assert!(matches!(
        remove_connection(&app.remote, &app.installations, &backup),
        Err(CommandError::ConnectionNotFound { .. })
    ));

    // Once no source uses it, the connection can be removed.
    save_sources(
        &app.installations,
        &app.remote,
        &old,
        RemoteSourcesConfig::default(),
    )
    .expect("clear old");
    save_sources(
        &app.installations,
        &app.remote,
        &new,
        RemoteSourcesConfig::default(),
    )
    .expect("clear new");
    let view = remove_connection(&app.remote, &app.installations, &nas).expect("remove");
    assert!(view.connections.is_empty());
}

#[test]
fn offers_mame_directories_and_warns_about_shared_destinations() {
    let app = App::new();
    let first = app.add(
        "0.279",
        "0.279",
        Some("rompath roms;bios\nsamplepath sets\n"),
    );
    let second = app.add("0.280", "0.280", None);
    fs::create_dir(app.mame_dir("0.279").join("roms")).expect("dir");
    let nas = app.connection("NAS", 22, "client_ed25519");

    let view = load_sources(&app.installations, &app.remote, &first).expect("view");
    assert_eq!(view.categories.len(), 5);
    assert_eq!(view.ini_file, Some(app.mame_dir("0.279").join("mame.ini")));
    let rom = &view.mame_directories[0];
    assert_eq!(rom.origin, DirectoriesOrigin::MameIni);
    let entries: Vec<_> = rom
        .directories
        .iter()
        .map(|d| (d.entry.as_str(), d.status))
        .collect();
    assert_eq!(
        entries,
        [
            ("roms", SourceStatus::Available),
            ("bios", SourceStatus::Missing)
        ]
    );
    assert_eq!(view.mame_directories[1].directories[0].entry, "sets");

    // Without mame.ini, MAME's defaults; global launch options win when set.
    let view = load_sources(&app.installations, &app.remote, &second).expect("view");
    assert_eq!(
        view.mame_directories[0].origin,
        DirectoriesOrigin::MameDefault
    );
    assert_eq!(view.mame_directories[0].directories[0].entry, "roms");
    let mut launch = LaunchConfig::default();
    let custom_roms = app
        .dir
        .path()
        .join("Shared ROMs")
        .to_string_lossy()
        .into_owned();
    launch.global_options.insert(
        "rompath".to_owned(),
        OptionValue::List(vec![custom_roms.clone()]),
    );
    app.installations
        .resolve(&second)
        .expect("resolve")
        .launch_config
        .save(launch)
        .expect("launch");
    let view = load_sources(&app.installations, &app.remote, &second).expect("view");
    assert_eq!(
        view.mame_directories[0].origin,
        DirectoriesOrigin::LaunchOptions
    );
    assert_eq!(view.mame_directories[0].directories[0].entry, custom_roms);

    // Both installations download into the same directory: allowed, with a warning.
    let shared = app.dir.path().join("Shared ROMs");
    let mut config = RemoteSourcesConfig::default();
    config.categories.insert(
        SourceCategory::MachineRoms,
        enabled(
            &nas,
            "/0279",
            LocalDestination::Custom {
                directory: shared.clone(),
            },
        ),
    );
    save_sources(&app.installations, &app.remote, &first, config).expect("first");
    let mut config = RemoteSourcesConfig::default();
    config.categories.insert(
        SourceCategory::MachineRoms,
        enabled(&nas, "/0280", mame_directory(&custom_roms)),
    );
    let view = save_sources(&app.installations, &app.remote, &second, config).expect("second");
    let messages: Vec<_> = view.warnings.iter().map(|w| w.message.as_str()).collect();
    assert!(
        messages
            .iter()
            .any(|m| m.starts_with("MAME 0.279 also downloads Machine ROMs")),
        "{messages:?}"
    );
    // The custom directory does not exist yet.
    let view = load_sources(&app.installations, &app.remote, &first).expect("view");
    assert!(
        view.warnings
            .iter()
            .any(|w| w.message.contains("is not available"))
    );
}

#[test]
fn removing_an_installation_with_its_settings_deletes_its_sources_only() {
    let app = App::new();
    let first = app.add("a", "0.279", None);
    let second = app.add("b", "0.280", None);
    let nas = app.connection("NAS", 22, "client_ed25519");
    for id in [&first, &second] {
        let mut config = RemoteSourcesConfig::default();
        config.categories.insert(
            SourceCategory::MachineRoms,
            enabled(&nas, "/roms", mame_directory("roms")),
        );
        save_sources(&app.installations, &app.remote, id, config).expect("save");
    }
    let resolved = app.installations.resolve(&first).expect("resolve");
    let result = app
        .installations
        .remove(&resolved, false, true)
        .expect("remove");
    assert!(result.kept_paths.is_empty());
    assert!(!resolved.remote_sources.exists());
    assert!(
        app.installations
            .resolve(&second)
            .expect("second")
            .remote_sources
            .exists()
    );
    // Only the remaining installation references the connection now.
    assert_eq!(
        references_to(&app.installations, &nas)
            .expect("references")
            .len(),
        1
    );
}

#[tokio::test]
async fn approves_only_the_presented_server_identity() {
    let app = App::new();
    let root = tempfile::tempdir().expect("root");
    fs::create_dir(root.path().join("roms")).expect("dir");
    let server = TestServer::start(ServerOptions {
        host_key: "host_a",
        authorized: vec!["client_ed25519"],
        root: root.path().to_path_buf(),
        sftp: true,
    })
    .await;
    let id = app.connection("Test server", server.port, "client_ed25519");

    // Nothing was presented yet.
    assert!(matches!(
        approve_host_key(&app.remote, &id, "SHA256:anything"),
        Err(CommandError::InvalidRequest { .. })
    ));
    let ConnectionTestResult::Failure {
        error: RemoteError::HostKeyUnknown { fingerprint, .. },
    } = test_connection(&app.remote, &id, None).await.expect("test")
    else {
        panic!("the identity should be unknown");
    };
    assert!(matches!(
        approve_host_key(&app.remote, &id, "SHA256:another"),
        Err(CommandError::InvalidRequest { .. })
    ));
    let view = approve_host_key(&app.remote, &id, &fingerprint).expect("approve");
    let ProviderConfig::Sftp(sftp) = &view.connections[0].connection.provider;
    assert_eq!(
        sftp.approved_host_key
            .as_ref()
            .map(|key| key.fingerprint.as_str()),
        Some(fingerprint.as_str())
    );

    let ConnectionTestResult::Success { info } =
        test_connection(&app.remote, &id, None).await.expect("test")
    else {
        panic!("the connection should succeed");
    };
    assert_eq!(info.home_directory.as_deref(), Some("/"));
    let listing = browse(&app.remote, &id, "", None).await.expect("browse");
    assert_eq!(listing.path, "/");
    assert_eq!(listing.entries[0].name, "roms");
    assert!(matches!(
        browse(&app.remote, &id, "/missing", None).await,
        Err(CommandError::Remote {
            error: RemoteError::NotFound { .. }
        })
    ));
    assert!(matches!(
        browse(&app.remote, &id, "relative", None).await,
        Err(CommandError::InvalidRequest { .. })
    ));

    // Editing the port forgets the identity: it belongs to another server.
    save_connection(
        &app.remote,
        ConnectionInput {
            id: Some(id.clone()),
            name: "Test server".to_owned(),
            provider: ProviderInput::Sftp(SftpSettings {
                host: "127.0.0.1".to_owned(),
                port: 1,
                username: "mame".to_owned(),
                private_key_path: testdata("client_ed25519"),
                host_key_verification: HostKeyVerification::Ymdb,
            }),
        },
    )
    .expect("edit");
    let config = app.remote.load().expect("load");
    let ProviderConfig::Sftp(sftp) = &config.get(&id).expect("connection").provider;
    assert!(sftp.approved_host_key.is_none());
}

#[test]
fn reports_an_invalid_connections_file_without_replacing_it() {
    let app = App::new();
    let file = app
        .dir
        .path()
        .join("config ✓")
        .join("remote-connections.json");
    fs::create_dir_all(file.parent().expect("parent")).expect("dir");
    fs::write(&file, "{ broken").expect("write");
    let view = connections_view(&app.remote);
    assert!(view.load_error.is_some());
    assert!(matches!(
        save_connection(
            &app.remote,
            ConnectionInput {
                id: None,
                name: "NAS".to_owned(),
                provider: ProviderInput::Sftp(SftpSettings {
                    host: "nas".to_owned(),
                    port: 22,
                    username: "mame".to_owned(),
                    private_key_path: testdata("client_ed25519"),
                    host_key_verification: HostKeyVerification::Ymdb,
                }),
            }
        ),
        Err(CommandError::Settings { .. })
    ));
    assert_eq!(fs::read_to_string(&file).expect("read"), "{ broken");
}
