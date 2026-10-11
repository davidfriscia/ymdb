//! The SFTP provider: SFTP over SSH with private key authentication.
//!
//! Built on `russh` and `russh-sftp` (pure Rust, no OpenSSL or system SSH
//! needed). A connection checks, in order: name resolution and TCP
//! connection, SSH handshake, the server identity, authentication with the
//! configured private key (decrypted with the passphrase when needed) and the
//! SFTP subsystem. Only SFTP requests are sent: no remote command is ever run.
//!
//! The server identity is always verified (see [`verify_host_key`]); an
//! unknown or changed key stops the connection before authentication, so the
//! private key is never used with an unverified server.

use std::io;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex, PoisonError};
use std::task::{Context, Poll};
use std::time::Duration;

use russh::client::{self, Handle};
use russh::keys::known_hosts::known_host_keys_path;
use russh::keys::{HashAlg, PrivateKey, PrivateKeyWithHashAlg, PublicKey, PublicKeyOrCertificate};
use russh_sftp::client::SftpSession;
use russh_sftp::client::error::Error as SftpError;
use russh_sftp::client::fs::{File, Metadata};
use russh_sftp::protocol::FileType;
use russh_sftp::protocol::StatusCode;
use tokio::io::{AsyncRead, AsyncSeekExt, ReadBuf};
use tokio::net::TcpStream;
use tokio::time::timeout;

use super::connections::{ApprovedHostKey, HostKeyVerification, SftpConfig};
use super::path::RemotePath;
use super::provider::{
    Capabilities, ConnectOptions, ConnectionInfo, Download, EntryKind, HostKeySource, RemoteEntry,
    RemoteError, RemoteMetadata, RemoteRepository, Timeouts,
};

/// Private key files larger than this are not keys.
const MAX_PRIVATE_KEY_SIZE: u64 = 64 * 1024;

const CAPABILITIES: Capabilities = Capabilities {
    list_directory: true,
    ranged_download: true,
};

/// Everything needed to open one SFTP connection.
#[derive(Debug, Clone)]
pub struct SftpTarget {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub private_key_path: PathBuf,
    pub verification: HostKeyVerification,
    pub approved_host_key: Option<ApprovedHostKey>,
}

impl SftpTarget {
    pub fn from_config(config: &SftpConfig) -> Self {
        Self {
            host: config.settings.host.clone(),
            port: config.settings.port,
            username: config.settings.username.clone(),
            private_key_path: config.settings.private_key_path.clone(),
            verification: config.settings.host_key_verification,
            approved_host_key: config.approved_host_key.clone(),
        }
    }

    /// The host as a socket address expects it (IPv6 without brackets).
    fn socket_host(&self) -> &str {
        self.host
            .strip_prefix('[')
            .and_then(|host| host.strip_suffix(']'))
            .unwrap_or(&self.host)
    }
}

/// The OpenSSH fingerprint of a key, `SHA256:<base64>`.
pub fn fingerprint(key: &PublicKey) -> String {
    key.fingerprint(HashAlg::Sha256).to_string()
}

/// Decides whether a server identity is trusted.
///
/// - With [`HostKeyVerification::KnownHosts`], the user's `known_hosts` file
///   is read (never written): a listed key that matches is trusted, a listed
///   key of the same type that differs is a changed identity.
/// - Otherwise, or for hosts `known_hosts` does not list, the key must match
///   the identity approved in YMDB; without one the identity is unknown.
///
/// A changed identity is never approved automatically.
pub fn verify_host_key(
    target: &SftpTarget,
    key: &PublicKey,
    known_hosts: Option<&Path>,
) -> Result<(), RemoteError> {
    let presented = fingerprint(key);
    let algorithm = key.algorithm().as_str().to_owned();
    if target.verification == HostKeyVerification::KnownHosts
        && let Some(path) = known_hosts
    {
        match russh::keys::check_known_hosts_path(&target.host, target.port, key, path) {
            Ok(true) => return Ok(()),
            Ok(false) => {}
            Err(russh::keys::Error::KeyChanged { line }) => {
                let expected = known_host_keys_path(&target.host, target.port, path)
                    .ok()
                    .and_then(|keys| keys.into_iter().find(|(number, _)| *number == line))
                    .map_or_else(
                        || format!("line {line} of known_hosts"),
                        |(_, key)| fingerprint(&key),
                    );
                return Err(RemoteError::HostKeyChanged {
                    host: target.host.clone(),
                    port: target.port,
                    algorithm,
                    fingerprint: presented,
                    expected_fingerprint: expected,
                    source: HostKeySource::KnownHosts,
                });
            }
            Err(error) => {
                return Err(RemoteError::Protocol {
                    message: format!("the known_hosts file could not be read: {error}"),
                });
            }
        }
    }
    match &target.approved_host_key {
        Some(approved) if approved.fingerprint == presented => Ok(()),
        Some(approved) => Err(RemoteError::HostKeyChanged {
            host: target.host.clone(),
            port: target.port,
            algorithm,
            fingerprint: presented,
            expected_fingerprint: approved.fingerprint.clone(),
            source: HostKeySource::Ymdb,
        }),
        None => Err(RemoteError::HostKeyUnknown {
            host: target.host.clone(),
            port: target.port,
            algorithm,
            fingerprint: presented,
            public_key: key.to_openssh().unwrap_or_default(),
        }),
    }
}

/// Reads and decodes a private key. OpenSSH, PEM (PKCS#1/PKCS#8) and PuTTY
/// keys are supported; encrypted keys need their passphrase.
pub fn load_private_key(path: &Path, passphrase: Option<&str>) -> Result<PrivateKey, RemoteError> {
    let unavailable = |message: String| RemoteError::PrivateKeyUnavailable { message };
    let metadata = std::fs::metadata(path).map_err(|error| unavailable(describe_io(&error)))?;
    if !metadata.is_file() {
        return Err(unavailable("the path is not a file".to_owned()));
    }
    if metadata.len() > MAX_PRIVATE_KEY_SIZE {
        return Err(RemoteError::PrivateKeyInvalid {
            message: "the file is too large to be a private key".to_owned(),
        });
    }
    let text = std::fs::read_to_string(path).map_err(|error| match error.kind() {
        io::ErrorKind::InvalidData => RemoteError::PrivateKeyInvalid {
            message: "the file is not a text key file".to_owned(),
        },
        _ => unavailable(describe_io(&error)),
    })?;
    decode_private_key(&text, passphrase)
}

fn decode_private_key(text: &str, passphrase: Option<&str>) -> Result<PrivateKey, RemoteError> {
    let invalid = || RemoteError::PrivateKeyInvalid {
        message:
            "the file is not a supported private key (select the private key, not the .pub file)"
                .to_owned(),
    };
    if text.contains("-----BEGIN OPENSSH PRIVATE KEY-----") {
        let key = PrivateKey::from_openssh(text).map_err(|_| invalid())?;
        if !key.is_encrypted() {
            return Ok(key);
        }
        let passphrase = passphrase.ok_or(RemoteError::PassphraseRequired)?;
        return key
            .decrypt(passphrase)
            .map_err(|_| RemoteError::PassphraseIncorrect);
    }
    let encrypted = text.contains("ENCRYPTED") || text.contains("Encryption: aes");
    match russh::keys::decode_secret_key(text, passphrase) {
        Ok(key) => Ok(key),
        Err(_) if encrypted && passphrase.is_none() => Err(RemoteError::PassphraseRequired),
        Err(russh::keys::Error::KeyIsEncrypted) if passphrase.is_none() => {
            Err(RemoteError::PassphraseRequired)
        }
        Err(_) if encrypted => Err(RemoteError::PassphraseIncorrect),
        Err(_) => Err(invalid()),
    }
}

fn describe_io(error: &io::Error) -> String {
    match error.kind() {
        io::ErrorKind::NotFound => "the file does not exist".to_owned(),
        io::ErrorKind::PermissionDenied => "permission denied".to_owned(),
        _ => error.to_string(),
    }
}

/// The SSH client side: only checks the server identity. The outcome of the
/// check is kept so that a refused key is reported with its fingerprint.
struct ClientHandler {
    target: SftpTarget,
    known_hosts: Option<PathBuf>,
    rejection: Arc<Mutex<Option<RemoteError>>>,
}

impl client::Handler for ClientHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let verdict = match server_public_key {
            PublicKeyOrCertificate::PublicKey { key, .. } => {
                verify_host_key(&self.target, key, self.known_hosts.as_deref())
            }
            PublicKeyOrCertificate::Certificate(_) => Err(RemoteError::Protocol {
                message: "SSH host certificates are not supported".to_owned(),
            }),
        };
        let trusted = verdict.is_ok();
        *self
            .rejection
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = verdict.err();
        Ok(trusted)
    }
}

/// An open SFTP connection.
pub struct SftpRepository {
    sftp: SftpSession,
    /// Kept alive by open downloads too.
    connection: Arc<Handle<ClientHandler>>,
}

impl SftpRepository {
    /// Connects, verifies the server, authenticates and opens SFTP.
    pub async fn connect(
        target: &SftpTarget,
        options: &ConnectOptions,
    ) -> Result<(Self, ConnectionInfo), RemoteError> {
        let timeouts = options.timeouts;
        let socket = open_socket(target, timeouts.connect).await?;

        let rejection = Arc::new(Mutex::new(None));
        let handler = ClientHandler {
            target: target.clone(),
            known_hosts: options.known_hosts.clone(),
            rejection: Arc::clone(&rejection),
        };
        let config = Arc::new(client::Config {
            keepalive_interval: Some(Duration::from_secs(20)),
            keepalive_max: 3,
            nodelay: true,
            ..client::Config::default()
        });
        let handshake = timeout(
            timeouts.connect,
            client::connect_stream(config, socket, handler),
        )
        .await
        .map_err(|_| RemoteError::TimedOut)?;
        let mut handle = handshake.map_err(|error| {
            rejection
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .take()
                .unwrap_or_else(|| ssh_error(error))
        })?;

        // The key is read only now, after the server was verified.
        let path = target.private_key_path.clone();
        let passphrase = options.passphrase.expose().map(str::to_owned);
        let key =
            tokio::task::spawn_blocking(move || load_private_key(&path, passphrase.as_deref()))
                .await
                .map_err(|error| RemoteError::Protocol {
                    message: error.to_string(),
                })??;
        let hash = timeout(timeouts.connect, handle.best_supported_rsa_hash())
            .await
            .map_err(|_| RemoteError::TimedOut)?
            .map_err(ssh_error)?
            .flatten();
        let authentication = timeout(
            timeouts.connect,
            handle.authenticate_publickey(
                target.username.clone(),
                PrivateKeyWithHashAlg::new(Arc::new(key), hash),
            ),
        )
        .await
        .map_err(|_| RemoteError::TimedOut)?
        .map_err(ssh_error)?;
        if !authentication.success() {
            return Err(RemoteError::AuthenticationFailed);
        }

        let sftp = timeout(timeouts.connect, open_sftp(&handle, timeouts))
            .await
            .map_err(|_| RemoteError::TimedOut)??;
        let home_directory = sftp.canonicalize(".").await.ok();
        Ok((
            Self {
                sftp,
                connection: Arc::new(handle),
            },
            ConnectionInfo {
                home_directory,
                capabilities: CAPABILITIES,
            },
        ))
    }
}

async fn open_socket(target: &SftpTarget, limit: Duration) -> Result<TcpStream, RemoteError> {
    let host = target.socket_host();
    let addresses: Vec<_> = timeout(limit, tokio::net::lookup_host((host, target.port)))
        .await
        .map_err(|_| RemoteError::TimedOut)?
        .map_err(|_| RemoteError::HostUnreachable {
            message: format!("the host name '{}' could not be resolved", target.host),
        })?
        .collect();
    let mut last_error = None;
    for address in addresses {
        match timeout(limit, TcpStream::connect(address)).await {
            Err(_) => return Err(RemoteError::TimedOut),
            Ok(Ok(socket)) => return Ok(socket),
            Ok(Err(error)) => last_error = Some(error),
        }
    }
    Err(RemoteError::HostUnreachable {
        message: match last_error {
            Some(error) if error.kind() == io::ErrorKind::ConnectionRefused => format!(
                "the connection to port {} was refused (is the SSH server running?)",
                target.port
            ),
            Some(error) => error.to_string(),
            None => format!("no address found for '{}'", target.host),
        },
    })
}

async fn open_sftp(
    handle: &Handle<ClientHandler>,
    timeouts: Timeouts,
) -> Result<SftpSession, RemoteError> {
    let channel = handle.channel_open_session().await.map_err(ssh_error)?;
    channel
        .request_subsystem(true, "sftp")
        .await
        .map_err(ssh_error)?;
    let config = russh_sftp::client::Config {
        request_timeout_secs: timeouts.request.as_secs().max(1),
        ..russh_sftp::client::Config::default()
    };
    SftpSession::new_with_config(channel.into_stream(), config)
        .await
        .map_err(|_| RemoteError::Protocol {
            message: "the server does not provide SFTP for this user".to_owned(),
        })
}

/// Maps SSH transport failures.
fn ssh_error(error: russh::Error) -> RemoteError {
    use russh::Error;
    match error {
        Error::ConnectionTimeout
        | Error::InactivityTimeout
        | Error::KeepaliveTimeout
        | Error::Elapsed(_) => RemoteError::TimedOut,
        Error::IO(error) if error.kind() == io::ErrorKind::TimedOut => RemoteError::TimedOut,
        Error::Disconnect
        | Error::HUP
        | Error::SendError
        | Error::RecvError
        | Error::IO(_)
        | Error::WrongChannel => RemoteError::ConnectionLost,
        other => RemoteError::Protocol {
            message: other.to_string(),
        },
    }
}

/// Maps SFTP failures for an operation on `path`.
fn sftp_error(error: SftpError, path: &RemotePath, operation: &str) -> RemoteError {
    let path = path.to_string();
    match error {
        SftpError::Status(status) => match status.status_code {
            StatusCode::NoSuchFile => RemoteError::NotFound { path },
            StatusCode::PermissionDenied => RemoteError::PermissionDenied { path },
            StatusCode::OpUnsupported => RemoteError::Unsupported {
                operation: operation.to_owned(),
            },
            StatusCode::NoConnection | StatusCode::ConnectionLost => RemoteError::ConnectionLost,
            _ => RemoteError::Protocol {
                message: if status.error_message.is_empty() {
                    format!("{operation} failed for '{path}'")
                } else {
                    format!("{operation} failed for '{path}': {}", status.error_message)
                },
            },
        },
        SftpError::Timeout => RemoteError::TimedOut,
        SftpError::IO(_) => RemoteError::ConnectionLost,
        SftpError::UnexpectedBehavior(message)
            if message.contains("SendError") || message.contains("RecvError") =>
        {
            RemoteError::ConnectionLost
        }
        other => RemoteError::Protocol {
            message: other.to_string(),
        },
    }
}

fn metadata(attributes: &Metadata) -> RemoteMetadata {
    let kind = match attributes.file_type() {
        FileType::Dir => EntryKind::Directory,
        FileType::File => EntryKind::File,
        FileType::Symlink => EntryKind::Symlink,
        FileType::Other => EntryKind::Other,
    };
    RemoteMetadata {
        kind,
        size: attributes.size,
        modified_at: attributes.mtime.map(|seconds| u64::from(seconds) * 1000),
    }
}

impl RemoteRepository for SftpRepository {
    fn capabilities(&self) -> Capabilities {
        CAPABILITIES
    }

    async fn stat(&self, path: &RemotePath) -> Result<RemoteMetadata, RemoteError> {
        self.sftp
            .metadata(path.as_str())
            .await
            .map(|attributes| metadata(&attributes))
            .map_err(|error| sftp_error(error, path, "reading the metadata"))
    }

    async fn list_directory(&self, path: &RemotePath) -> Result<Vec<RemoteEntry>, RemoteError> {
        if self.stat(path).await?.kind != EntryKind::Directory {
            return Err(RemoteError::NotADirectory {
                path: path.to_string(),
            });
        }
        let listing = self
            .sftp
            .read_dir(path.as_str())
            .await
            .map_err(|error| sftp_error(error, path, "listing the directory"))?;
        let mut entries: Vec<_> = listing
            .filter(|entry| {
                let name = entry.file_name();
                name != "." && name != ".."
            })
            .map(|entry| RemoteEntry {
                name: entry.file_name(),
                metadata: metadata(&entry.metadata()),
            })
            .collect();
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(entries)
    }

    async fn open_download(&self, path: &RemotePath, offset: u64) -> Result<Download, RemoteError> {
        let info = self.stat(path).await?;
        if info.kind != EntryKind::File {
            return Err(RemoteError::Protocol {
                message: format!("'{path}' is not a file"),
            });
        }
        let mut file = self
            .sftp
            .open(path.as_str())
            .await
            .map_err(|error| sftp_error(error, path, "opening the file"))?;
        if offset > 0 {
            file.seek(io::SeekFrom::Start(offset))
                .await
                .map_err(|error| RemoteError::from_stream(&error))?;
        }
        Ok(Download {
            size: info.size,
            offset,
            reader: Box::pin(SftpReader {
                file,
                _connection: Arc::clone(&self.connection),
            }),
        })
    }

    async fn close(self) {
        // Best effort: the server may already be gone.
        let _ = self.sftp.close().await;
        let _ = self
            .connection
            .disconnect(russh::Disconnect::ByApplication, "", "en")
            .await;
    }
}

/// A remote file being read; keeps its connection open.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "used by the ROM manager's downloads")
)]
struct SftpReader {
    file: File,
    _connection: Arc<Handle<ClientHandler>>,
}

impl AsyncRead for SftpReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.file).poll_read(cx, buf)
    }
}

#[cfg(test)]
mod tests;
