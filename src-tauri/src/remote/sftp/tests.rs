//! SFTP provider tests against the in-process SSH server of `test_server`
//! (real SSH handshake, authentication and SFTP over TCP on 127.0.0.1).

use std::fs;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use tempfile::TempDir;
use tokio::io::AsyncReadExt;

use super::*;
use crate::remote::connections::{ProviderConfig, RemoteConnection, SftpSettings};
use crate::remote::provider::{Passphrase, Repository, connect};
use crate::remote::test_server::{ServerOptions, TestServer, private_key, silent_server, testdata};
use crate::remote::transfer::copy_download;

const AUTHORIZED: [&str; 3] = ["client_ed25519", "client_encrypted", "client_rsa"];
const BIG_FILE_SIZE: usize = 3 * 1024 * 1024 + 123;

fn content(size: usize) -> Vec<u8> {
    (0..size).map(|index| (index % 251) as u8).collect()
}

/// A served directory: `roms/` with files, a sub-directory and a forbidden entry.
fn served_root() -> TempDir {
    let dir = tempfile::tempdir().expect("temp dir");
    let roms = dir.path().join("roms");
    fs::create_dir_all(roms.join("sub dir ✓")).expect("dirs");
    fs::create_dir_all(roms.join("forbidden")).expect("dirs");
    fs::write(roms.join("readme.txt"), b"not a ROM").expect("write");
    fs::write(roms.join("big.bin"), content(BIG_FILE_SIZE)).expect("write");
    dir
}

async fn server(root: &TempDir, host_key: &'static str) -> TestServer {
    TestServer::start(ServerOptions {
        host_key,
        authorized: AUTHORIZED.to_vec(),
        root: root.path().to_path_buf(),
        sftp: true,
    })
    .await
}

fn approved(host_key: &str) -> ApprovedHostKey {
    let key = private_key(host_key).public_key().clone();
    ApprovedHostKey {
        algorithm: key.algorithm().as_str().to_owned(),
        fingerprint: fingerprint(&key),
        public_key: key.to_openssh().expect("openssh"),
        approved_at: 1,
    }
}

fn target(port: u16, key: &str, approved_host: Option<&str>) -> SftpTarget {
    SftpTarget {
        host: "127.0.0.1".to_owned(),
        port,
        username: "mame".to_owned(),
        private_key_path: testdata(key),
        verification: HostKeyVerification::Ymdb,
        approved_host_key: approved_host.map(approved),
    }
}

fn options(passphrase: Option<&str>) -> ConnectOptions {
    ConnectOptions {
        passphrase: Passphrase::new(passphrase.map(str::to_owned)),
        timeouts: Timeouts {
            connect: Duration::from_secs(10),
            request: Duration::from_secs(5),
        },
        known_hosts: None,
    }
}

async fn connect_error(target: &SftpTarget, options: &ConnectOptions) -> RemoteError {
    match SftpRepository::connect(target, options).await {
        Ok(_) => panic!("the connection should fail"),
        Err(error) => error,
    }
}

fn path(text: &str) -> RemotePath {
    RemotePath::parse(text).expect(text)
}

#[tokio::test]
async fn connects_lists_stats_and_streams_through_the_provider_contract() {
    let root = served_root();
    let server = server(&root, "host_a").await;
    let connection = RemoteConnection {
        id: "0123456789abcdef0123456789abcdef".to_owned(),
        name: "Test".to_owned(),
        provider: ProviderConfig::Sftp(SftpConfig {
            settings: SftpSettings {
                host: "127.0.0.1".to_owned(),
                port: server.port,
                username: "mame".to_owned(),
                private_key_path: testdata("client_ed25519"),
                host_key_verification: HostKeyVerification::Ymdb,
            },
            approved_host_key: Some(approved("host_a")),
        }),
    };
    // Provider resolution: the connection's provider type picks SFTP.
    let (repository, info) = connect(&connection, &options(None)).await.expect("connect");
    assert!(matches!(repository, Repository::Sftp(_)));
    assert_eq!(info.home_directory.as_deref(), Some("/"));
    assert!(info.capabilities.list_directory && info.capabilities.ranged_download);

    let stat = repository.stat(&path("/roms/big.bin")).await.expect("stat");
    assert_eq!(stat.kind, EntryKind::File);
    assert_eq!(stat.size, Some(BIG_FILE_SIZE as u64));
    assert!(stat.modified_at.is_some());
    assert_eq!(
        repository.stat(&path("/roms")).await.expect("stat").kind,
        EntryKind::Directory
    );
    assert!(
        repository
            .exists(&path("/roms/readme.txt"))
            .await
            .expect("exists")
    );
    assert!(
        !repository
            .exists(&path("/roms/missing.zip"))
            .await
            .expect("exists")
    );
    assert_eq!(
        repository.stat(&path("/roms/missing.zip")).await,
        Err(RemoteError::NotFound {
            path: "/roms/missing.zip".to_owned()
        })
    );

    let entries = repository
        .list_directory(&path("/roms"))
        .await
        .expect("list");
    let names: Vec<_> = entries.iter().map(|entry| entry.name.as_str()).collect();
    assert_eq!(names, ["big.bin", "forbidden", "readme.txt", "sub dir ✓"]);
    assert_eq!(entries[3].metadata.kind, EntryKind::Directory);
    assert_eq!(
        repository.list_directory(&path("/roms/readme.txt")).await,
        Err(RemoteError::NotADirectory {
            path: "/roms/readme.txt".to_owned()
        })
    );
    assert_eq!(
        repository.list_directory(&path("/nothing")).await,
        Err(RemoteError::NotFound {
            path: "/nothing".to_owned()
        })
    );
    assert_eq!(
        repository.list_directory(&path("/roms/forbidden")).await,
        Err(RemoteError::PermissionDenied {
            path: "/roms/forbidden".to_owned()
        })
    );

    // Streaming: chunk by chunk into a writer, with progress.
    let download = repository
        .open_download(&path("/roms/big.bin"), 0)
        .await
        .expect("open");
    assert_eq!(download.size, Some(BIG_FILE_SIZE as u64));
    let mut output = Vec::new();
    let mut reports = 0;
    let copied = copy_download(download, &mut output, &AtomicBool::new(false), |_| {
        reports += 1;
    })
    .await
    .expect("copy");
    assert_eq!(copied, BIG_FILE_SIZE as u64);
    assert!(reports > 1);
    assert!(output == content(BIG_FILE_SIZE));

    // Ranged download.
    let mut tail = Vec::new();
    let download = repository
        .open_download(&path("/roms/big.bin"), 1_000_000)
        .await
        .expect("open at offset");
    copy_download(download, &mut tail, &AtomicBool::new(false), |_| {})
        .await
        .expect("copy");
    assert!(tail == content(BIG_FILE_SIZE)[1_000_000..]);

    assert!(matches!(
        repository.open_download(&path("/roms"), 0).await,
        Err(RemoteError::Protocol { .. })
    ));
    assert!(matches!(
        repository.open_download(&path("/roms/none.zip"), 0).await,
        Err(RemoteError::NotFound { .. })
    ));
    repository.close().await;
}

#[tokio::test]
async fn supports_rsa_and_passphrase_protected_keys() {
    let root = served_root();
    let server = server(&root, "host_a").await;
    let (repository, _) = SftpRepository::connect(
        &target(server.port, "client_rsa", Some("host_a")),
        &options(None),
    )
    .await
    .expect("rsa");
    repository.close().await;

    let encrypted = target(server.port, "client_encrypted", Some("host_a"));
    assert_eq!(
        connect_error(&encrypted, &options(None)).await,
        RemoteError::PassphraseRequired
    );
    assert_eq!(
        connect_error(&encrypted, &options(Some("wrong"))).await,
        RemoteError::PassphraseIncorrect
    );
    let (repository, _) = SftpRepository::connect(&encrypted, &options(Some("correct horse")))
        .await
        .expect("with passphrase");
    repository.close().await;
}

#[tokio::test]
async fn reports_unknown_and_changed_server_identities() {
    let root = served_root();
    let server = server(&root, "host_a").await;

    let error = connect_error(&target(server.port, "client_ed25519", None), &options(None)).await;
    let RemoteError::HostKeyUnknown {
        fingerprint: shown,
        public_key: presented,
        algorithm,
        port,
        ..
    } = error
    else {
        panic!("unexpected {error:?}");
    };
    assert_eq!(shown, approved("host_a").fingerprint);
    assert!(shown.starts_with("SHA256:"));
    assert!(presented.starts_with("ssh-ed25519 "));
    assert_eq!(algorithm, "ssh-ed25519");
    assert_eq!(port, server.port);

    // The approved identity belongs to another server: refused, never replaced.
    let error = connect_error(
        &target(server.port, "client_ed25519", Some("host_b")),
        &options(None),
    )
    .await;
    assert_eq!(
        error,
        RemoteError::HostKeyChanged {
            host: "127.0.0.1".to_owned(),
            port: server.port,
            algorithm: "ssh-ed25519".to_owned(),
            fingerprint: approved("host_a").fingerprint,
            expected_fingerprint: approved("host_b").fingerprint,
            source: HostKeySource::Ymdb,
        }
    );
}

#[tokio::test]
async fn uses_openssh_known_hosts_when_configured() {
    let root = served_root();
    let server = server(&root, "host_a").await;
    let dir = tempfile::tempdir().expect("temp dir");
    let known_hosts = dir.path().join("known_hosts");
    let line = |host_key: &str| {
        format!(
            "# test\n[127.0.0.1]:{} {}\n",
            server.port,
            approved(host_key).public_key
        )
    };
    let mut known = target(server.port, "client_ed25519", None);
    known.verification = HostKeyVerification::KnownHosts;
    let options = ConnectOptions {
        known_hosts: Some(known_hosts.clone()),
        ..options(None)
    };

    fs::write(&known_hosts, line("host_a")).expect("write");
    let (repository, _) = SftpRepository::connect(&known, &options)
        .await
        .expect("listed in known_hosts");
    repository.close().await;

    fs::write(&known_hosts, line("host_b")).expect("write");
    let error = connect_error(&known, &options).await;
    assert!(
        matches!(
            &error,
            RemoteError::HostKeyChanged { source: HostKeySource::KnownHosts, expected_fingerprint, .. }
                if *expected_fingerprint == approved("host_b").fingerprint
        ),
        "{error:?}"
    );

    // Not listed: the identity approved in YMDB decides.
    fs::write(&known_hosts, "").expect("write");
    assert!(matches!(
        connect_error(&known, &options).await,
        RemoteError::HostKeyUnknown { .. }
    ));
    known.approved_host_key = Some(approved("host_a"));
    let (repository, _) = SftpRepository::connect(&known, &options)
        .await
        .expect("approved in YMDB");
    repository.close().await;
    // The YMDB mode ignores known_hosts.
    fs::write(&known_hosts, line("host_b")).expect("write");
    known.verification = HostKeyVerification::Ymdb;
    let (repository, _) = SftpRepository::connect(&known, &options)
        .await
        .expect("ymdb mode");
    repository.close().await;
}

#[tokio::test]
async fn reports_authentication_and_key_file_failures() {
    let root = served_root();
    let server = server(&root, "host_a").await;
    assert_eq!(
        connect_error(
            &target(server.port, "stranger", Some("host_a")),
            &options(None)
        )
        .await,
        RemoteError::AuthenticationFailed
    );
    assert!(matches!(
        connect_error(
            &target(server.port, "missing_key", Some("host_a")),
            &options(None)
        )
        .await,
        RemoteError::PrivateKeyUnavailable { .. }
    ));
    assert!(matches!(
        connect_error(
            &target(server.port, "not_a_key", Some("host_a")),
            &options(None)
        )
        .await,
        RemoteError::PrivateKeyInvalid { .. }
    ));
    // A public key selected by mistake.
    assert!(matches!(
        connect_error(
            &target(server.port, "client_ed25519.pub", Some("host_a")),
            &options(None)
        )
        .await,
        RemoteError::PrivateKeyInvalid { .. }
    ));
}

#[tokio::test]
async fn reports_unreachable_hosts_timeouts_and_missing_sftp() {
    // Nothing listens on a port that was just released.
    let port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        listener.local_addr().expect("address").port()
    };
    assert!(matches!(
        connect_error(&target(port, "client_ed25519", None), &options(None)).await,
        RemoteError::HostUnreachable { .. }
    ));

    let (port, silent) = silent_server().await;
    let mut quick = options(None);
    quick.timeouts.connect = Duration::from_millis(300);
    assert_eq!(
        connect_error(&target(port, "client_ed25519", None), &quick).await,
        RemoteError::TimedOut
    );
    silent.abort();

    let root = served_root();
    let no_sftp = TestServer::start(ServerOptions {
        host_key: "host_a",
        authorized: AUTHORIZED.to_vec(),
        root: root.path().to_path_buf(),
        sftp: false,
    })
    .await;
    assert!(matches!(
        connect_error(
            &target(no_sftp.port, "client_ed25519", Some("host_a")),
            &options(None)
        )
        .await,
        RemoteError::Protocol { .. }
    ));
}

#[tokio::test]
async fn reports_an_interrupted_transfer() {
    let root = served_root();
    let server = server(&root, "host_a").await;
    let (repository, _) = SftpRepository::connect(
        &target(server.port, "client_ed25519", Some("host_a")),
        &options(None),
    )
    .await
    .expect("connect");
    let mut download = repository
        .open_download(&path("/roms/big.bin"), 0)
        .await
        .expect("open");
    let mut first = vec![0; 1024];
    download
        .reader
        .read_exact(&mut first)
        .await
        .expect("first bytes");
    assert!(first == content(1024));

    server.disconnect_all().await;
    let mut rest = Vec::new();
    let result = copy_download(download, &mut rest, &AtomicBool::new(false), |_| {}).await;
    assert!(
        matches!(result, Err(RemoteError::ConnectionLost)),
        "{result:?}"
    );
    assert!(rest.len() < BIG_FILE_SIZE);

    // Cancellation stops a transfer that is still healthy.
    let server = self::server(&root, "host_a").await;
    let (repository, _) = SftpRepository::connect(
        &target(server.port, "client_ed25519", Some("host_a")),
        &options(None),
    )
    .await
    .expect("connect");
    let download = repository
        .open_download(&path("/roms/big.bin"), 0)
        .await
        .expect("open");
    let cancel = AtomicBool::new(false);
    let mut output = Vec::new();
    let result = copy_download(download, &mut output, &cancel, |position| {
        if position > 0 {
            cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    })
    .await;
    assert_eq!(result, Err(RemoteError::Cancelled));
    repository.close().await;
}

#[test]
fn decodes_keys_without_a_server() {
    let read = |name: &str| fs::read_to_string(testdata(name)).expect("read");
    assert!(decode_private_key(&read("client_ed25519"), None).is_ok());
    assert!(decode_private_key(&read("client_rsa"), Some("ignored")).is_ok());
    assert_eq!(
        decode_private_key(&read("client_encrypted"), None).err(),
        Some(RemoteError::PassphraseRequired)
    );
    assert!(matches!(
        decode_private_key("garbage", None),
        Err(RemoteError::PrivateKeyInvalid { .. })
    ));
    assert!(matches!(
        load_private_key(&testdata("."), None),
        Err(RemoteError::PrivateKeyUnavailable { .. })
    ));
}
