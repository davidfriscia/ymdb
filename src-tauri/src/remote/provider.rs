//! The contract between the ROM manager and remote repositories.
//!
//! A [`RemoteRepository`] is an open connection to one repository. The ROM
//! manager only uses this contract, never a protocol directly, so a future
//! provider (HTTPS, S3) is added by implementing it and adding a variant to
//! [`Repository`] and to `connections::ProviderConfig`.
//!
//! Providers differ in what they can do: [`Capabilities`] says which optional
//! operations are available, and an unavailable operation fails with
//! [`RemoteError::Unsupported`] instead of being emulated.
//!
//! Downloads are streams ([`Download`]): nothing requires a whole file in
//! memory, and the caller decides where the bytes go (see `transfer`).

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use serde::Serialize;
use tokio::io::AsyncRead;

use super::connections::{ProviderConfig, RemoteConnection};
use super::path::RemotePath;
use super::sftp::{SftpRepository, SftpTarget};

/// Optional operations of a provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    /// Directories can be listed. A provider without listings (for example a
    /// plain web server) can only reach files by their known path.
    pub list_directory: bool,
    /// A download can start at an offset (resume, partial reads).
    pub ranged_download: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum EntryKind {
    File,
    Directory,
    Symlink,
    Other,
}

/// Metadata of a remote object, as far as the provider knows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteMetadata {
    pub kind: EntryKind,
    pub size: Option<u64>,
    /// Milliseconds since the Unix epoch.
    pub modified_at: Option<u64>,
}

/// One entry of a directory listing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteEntry {
    pub name: String,
    #[serde(flatten)]
    pub metadata: RemoteMetadata,
}

/// An open download: a stream of the object's bytes from `offset`.
pub struct Download {
    /// Size of the whole object, when known.
    pub size: Option<u64>,
    pub offset: u64,
    pub reader: Pin<Box<dyn AsyncRead + Send>>,
}

/// Why a remote operation failed. Messages never contain credentials.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum RemoteError {
    /// The host name cannot be resolved or the host refuses the connection.
    HostUnreachable { message: String },
    /// The server did not answer in time.
    TimedOut,
    /// The server's identity has never been approved: show the fingerprint
    /// and ask the user.
    HostKeyUnknown {
        host: String,
        port: u16,
        algorithm: String,
        fingerprint: String,
        /// The presented key in OpenSSH format, recorded when approved.
        public_key: String,
    },
    /// The server's identity differs from the approved one: possibly an
    /// attack. The connection is refused and nothing is approved.
    HostKeyChanged {
        host: String,
        port: u16,
        algorithm: String,
        fingerprint: String,
        expected_fingerprint: String,
        /// Where the approved key comes from: `ymdb` or `knownHosts`.
        source: HostKeySource,
    },
    /// The private key file cannot be read.
    PrivateKeyUnavailable { message: String },
    /// The file is not a supported private key.
    PrivateKeyInvalid { message: String },
    /// The private key is protected by a passphrase: ask for it.
    PassphraseRequired,
    /// The passphrase does not decrypt the private key.
    PassphraseIncorrect,
    /// The server refused the key for this user.
    AuthenticationFailed,
    /// The server refused access to the object.
    PermissionDenied { path: String },
    /// The file or directory does not exist.
    NotFound { path: String },
    /// A directory was expected.
    NotADirectory { path: String },
    /// The provider cannot perform this operation.
    Unsupported { operation: String },
    /// The connection was closed or broken during the operation.
    ConnectionLost,
    /// The operation was cancelled by the caller.
    Cancelled,
    /// A local file could not be written during a transfer.
    LocalIo { message: String },
    /// Any other protocol failure.
    Protocol { message: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum HostKeySource {
    Ymdb,
    KnownHosts,
}

impl std::fmt::Display for RemoteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::HostUnreachable { message } => write!(f, "the host is unreachable: {message}"),
            Self::TimedOut => f.write_str("the server did not answer in time"),
            Self::HostKeyUnknown { fingerprint, .. } => {
                write!(f, "the server identity {fingerprint} is not approved")
            }
            Self::HostKeyChanged { fingerprint, .. } => write!(
                f,
                "the server identity changed ({fingerprint}): the connection was refused"
            ),
            Self::PrivateKeyUnavailable { message } => {
                write!(f, "the private key cannot be read: {message}")
            }
            Self::PrivateKeyInvalid { message } => {
                write!(f, "the private key is not valid: {message}")
            }
            Self::PassphraseRequired => f.write_str("the private key needs a passphrase"),
            Self::PassphraseIncorrect => f.write_str("the passphrase is not correct"),
            Self::AuthenticationFailed => f.write_str("the server refused the private key"),
            Self::PermissionDenied { path } => write!(f, "access to '{path}' was denied"),
            Self::NotFound { path } => write!(f, "'{path}' does not exist"),
            Self::NotADirectory { path } => write!(f, "'{path}' is not a directory"),
            Self::Unsupported { operation } => {
                write!(f, "this repository does not support {operation}")
            }
            Self::ConnectionLost => f.write_str("the connection was interrupted"),
            Self::Cancelled => f.write_str("the operation was cancelled"),
            Self::LocalIo { message } => write!(f, "local file error: {message}"),
            Self::Protocol { message } => write!(f, "remote error: {message}"),
        }
    }
}

impl std::error::Error for RemoteError {}

impl RemoteError {
    /// Maps an I/O error while reading a download stream.
    pub fn from_stream(error: &std::io::Error) -> Self {
        match error.kind() {
            std::io::ErrorKind::TimedOut => Self::TimedOut,
            _ => Self::ConnectionLost,
        }
    }
}

/// Secret material given for one connection only. It is never stored,
/// serialized or printed.
#[derive(Clone, Default)]
pub struct Passphrase(Option<String>);

impl Passphrase {
    pub fn new(value: Option<String>) -> Self {
        Self(value.filter(|value| !value.is_empty()))
    }

    pub fn expose(&self) -> Option<&str> {
        self.0.as_deref()
    }
}

impl std::fmt::Debug for Passphrase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(if self.0.is_some() {
            "Passphrase(<redacted>)"
        } else {
            "Passphrase(None)"
        })
    }
}

/// Network time limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timeouts {
    /// Name resolution, TCP connection, handshake and authentication.
    pub connect: Duration,
    /// One request (stat, listing, one read of a download).
    pub request: Duration,
}

impl Default for Timeouts {
    fn default() -> Self {
        Self {
            connect: Duration::from_secs(15),
            request: Duration::from_secs(30),
        }
    }
}

/// What a connection needs besides its saved configuration.
#[derive(Debug, Clone, Default)]
pub struct ConnectOptions {
    pub passphrase: Passphrase,
    pub timeouts: Timeouts,
    /// The user's OpenSSH `known_hosts` file, for connections that use it.
    pub known_hosts: Option<std::path::PathBuf>,
}

/// An open connection to a remote repository.
pub trait RemoteRepository: Send + Sync {
    fn capabilities(&self) -> Capabilities;

    /// Metadata of an object; [`RemoteError::NotFound`] when it does not exist.
    fn stat(
        &self,
        path: &RemotePath,
    ) -> impl Future<Output = Result<RemoteMetadata, RemoteError>> + Send;

    /// Whether an object exists.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "part of the contract, not used by the ROM manager yet"
        )
    )]
    fn exists(&self, path: &RemotePath) -> impl Future<Output = Result<bool, RemoteError>> + Send {
        async move {
            match self.stat(path).await {
                Ok(_) => Ok(true),
                Err(RemoteError::NotFound { .. }) => Ok(false),
                Err(error) => Err(error),
            }
        }
    }

    /// The entries of a directory, sorted by name, without `.` and `..`.
    /// [`RemoteError::Unsupported`] when the provider cannot list.
    fn list_directory(
        &self,
        path: &RemotePath,
    ) -> impl Future<Output = Result<Vec<RemoteEntry>, RemoteError>> + Send;

    /// Opens a stream of a file's content from `offset`. An offset other
    /// than 0 needs [`Capabilities::ranged_download`].
    fn open_download(
        &self,
        path: &RemotePath,
        offset: u64,
    ) -> impl Future<Output = Result<Download, RemoteError>> + Send;

    /// Closes the connection.
    fn close(self) -> impl Future<Output = ()> + Send;
}

/// Server information gathered by a successful connection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionInfo {
    /// The working directory the server starts in, when it says so.
    pub home_directory: Option<String>,
    pub capabilities: Capabilities,
}

/// A connected repository of any provider.
pub enum Repository {
    Sftp(SftpRepository),
}

/// Opens the repository of a connection with the provider it names.
pub async fn connect(
    connection: &RemoteConnection,
    options: &ConnectOptions,
) -> Result<(Repository, ConnectionInfo), RemoteError> {
    match &connection.provider {
        ProviderConfig::Sftp(config) => {
            let target = SftpTarget::from_config(config);
            let (repository, info) = SftpRepository::connect(&target, options).await?;
            Ok((Repository::Sftp(repository), info))
        }
    }
}

impl RemoteRepository for Repository {
    fn capabilities(&self) -> Capabilities {
        match self {
            Self::Sftp(repository) => repository.capabilities(),
        }
    }

    async fn stat(&self, path: &RemotePath) -> Result<RemoteMetadata, RemoteError> {
        match self {
            Self::Sftp(repository) => repository.stat(path).await,
        }
    }

    async fn list_directory(&self, path: &RemotePath) -> Result<Vec<RemoteEntry>, RemoteError> {
        match self {
            Self::Sftp(repository) => repository.list_directory(path).await,
        }
    }

    async fn open_download(&self, path: &RemotePath, offset: u64) -> Result<Download, RemoteError> {
        match self {
            Self::Sftp(repository) => repository.open_download(path, offset).await,
        }
    }

    async fn close(self) {
        match self {
            Self::Sftp(repository) => repository.close().await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_errors_to_the_frontend_contract() {
        assert_eq!(
            serde_json::to_value(RemoteError::HostKeyChanged {
                host: "nas".to_owned(),
                port: 22,
                algorithm: "ssh-ed25519".to_owned(),
                fingerprint: "SHA256:new".to_owned(),
                expected_fingerprint: "SHA256:old".to_owned(),
                source: HostKeySource::KnownHosts,
            })
            .expect("serialize"),
            serde_json::json!({
                "kind": "hostKeyChanged",
                "host": "nas",
                "port": 22,
                "algorithm": "ssh-ed25519",
                "fingerprint": "SHA256:new",
                "expectedFingerprint": "SHA256:old",
                "source": "knownHosts"
            })
        );
        assert_eq!(
            serde_json::to_value(RemoteError::PassphraseRequired).expect("serialize"),
            serde_json::json!({ "kind": "passphraseRequired" })
        );
        assert_eq!(
            serde_json::to_value(RemoteError::Unsupported {
                operation: "directory listing".to_owned()
            })
            .expect("serialize"),
            serde_json::json!({ "kind": "unsupported", "operation": "directory listing" })
        );
    }

    #[test]
    fn passphrases_are_never_printed() {
        let passphrase = Passphrase::new(Some("hunter2".to_owned()));
        assert_eq!(format!("{passphrase:?}"), "Passphrase(<redacted>)");
        assert_eq!(passphrase.expose(), Some("hunter2"));
        assert_eq!(Passphrase::new(Some(String::new())).expose(), None);
        let options = ConnectOptions {
            passphrase,
            ..ConnectOptions::default()
        };
        assert!(!format!("{options:?}").contains("hunter2"));
    }
}
