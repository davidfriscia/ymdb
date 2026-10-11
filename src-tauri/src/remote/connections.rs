//! Remote connections (`remote-connections.json`).
//!
//! A connection says how to reach a repository; it is global and shared by
//! every MAME installation, which reference it by id from their remote
//! sources. Each connection names its provider (`providerType`) and keeps
//! the provider's own settings (`providerConfig`), so a new provider is a new
//! variant of [`ProviderConfig`] and existing connections never change.
//!
//! Only non-sensitive settings are stored: the path of the private key (never
//! the key), and the server identity approved by the user. Passphrases are
//! never stored.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::atomic_file;

pub const SCHEMA_VERSION: u32 = 1;
pub const CONNECTIONS_FILE_NAME: &str = "remote-connections.json";
pub const DEFAULT_SSH_PORT: u16 = 22;
pub const MAX_CONNECTIONS: usize = 100;
pub const MAX_NAME_LENGTH: usize = 60;
const MAX_HOST_LENGTH: usize = 253;
const MAX_USERNAME_LENGTH: usize = 100;

/// A configured remote connection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteConnection {
    /// Opaque and stable: sources reference connections by id.
    pub id: String,
    pub name: String,
    #[serde(flatten)]
    pub provider: ProviderConfig,
}

/// The provider of a connection and its settings, serialized as
/// `"providerType": "sftp", "providerConfig": { ... }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "providerType",
    content = "providerConfig",
    rename_all = "camelCase"
)]
pub enum ProviderConfig {
    Sftp(SftpConfig),
}

/// Provider types, as shown to the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ProviderType {
    Sftp,
}

impl ProviderConfig {
    pub fn provider_type(&self) -> ProviderType {
        match self {
            Self::Sftp(_) => ProviderType::Sftp,
        }
    }
}

/// SFTP over SSH, authenticated with a private key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SftpConfig {
    #[serde(flatten)]
    pub settings: SftpSettings,
    /// The server identity approved by the user, if any. Set only by an
    /// explicit approval, never from the frontend's saved settings.
    #[serde(default)]
    pub approved_host_key: Option<ApprovedHostKey>,
}

/// The editable SFTP settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SftpSettings {
    /// Host name or IP address.
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    pub username: String,
    /// Absolute path of the private key file. The key itself is never copied.
    pub private_key_path: PathBuf,
    #[serde(default)]
    pub host_key_verification: HostKeyVerification,
}

fn default_port() -> u16 {
    DEFAULT_SSH_PORT
}

/// Where trusted server identities come from. Server identities are always
/// verified; there is no mode that skips the check.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HostKeyVerification {
    /// The identity approved in YMDB (shown on first connection).
    #[default]
    Ymdb,
    /// The user's OpenSSH `known_hosts` file first (read only), then the
    /// identity approved in YMDB for hosts it does not list.
    KnownHosts,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovedHostKey {
    /// For example `ssh-ed25519`.
    pub algorithm: String,
    /// `SHA256:<base64>`, as printed by OpenSSH.
    pub fingerprint: String,
    /// The public key in OpenSSH format (`<algorithm> <base64>`).
    pub public_key: String,
    /// Milliseconds since the Unix epoch.
    pub approved_at: u64,
}

/// A connection as created or edited by the user (no id for a new one).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionInput {
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    #[serde(flatten)]
    pub provider: ProviderInput,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(
    tag = "providerType",
    content = "providerConfig",
    rename_all = "camelCase"
)]
pub enum ProviderInput {
    Sftp(SftpSettings),
}

/// Content of `remote-connections.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionsConfig {
    #[serde(default = "schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub connections: Vec<RemoteConnection>,
}

fn schema_version() -> u32 {
    SCHEMA_VERSION
}

impl Default for ConnectionsConfig {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            connections: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionError {
    NotFound(String),
    Invalid(String),
}

impl fmt::Display for ConnectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound(id) => write!(f, "remote connection '{id}' does not exist"),
            Self::Invalid(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for ConnectionError {}

fn invalid(message: impl Into<String>) -> ConnectionError {
    ConnectionError::Invalid(message.into())
}

/// Whether `id` has the format of connection ids (a random UUID, 32
/// lowercase hexadecimal digits).
pub fn is_connection_id(id: &str) -> bool {
    crate::installations::is_installation_id(id)
}

impl ConnectionsConfig {
    pub fn get(&self, id: &str) -> Option<&RemoteConnection> {
        self.connections
            .iter()
            .find(|connection| connection.id == id)
    }

    fn get_mut(&mut self, id: &str) -> Result<&mut RemoteConnection, ConnectionError> {
        self.connections
            .iter_mut()
            .find(|connection| connection.id == id)
            .ok_or_else(|| ConnectionError::NotFound(id.to_owned()))
    }

    /// Adds a new connection or updates an existing one and returns its id.
    /// Changing the host or port of an SFTP connection forgets the approved
    /// server identity, which belongs to the previous server.
    pub fn upsert(
        &mut self,
        input: ConnectionInput,
        new_id: String,
    ) -> Result<String, ConnectionError> {
        let name = input.name.trim().to_owned();
        let ProviderInput::Sftp(mut settings) = input.provider;
        settings.host = settings.host.trim().to_owned();
        settings.username = settings.username.trim().to_owned();
        let id = match input.id {
            None => {
                if self.connections.len() >= MAX_CONNECTIONS {
                    return Err(invalid(format!(
                        "at most {MAX_CONNECTIONS} remote connections can be configured"
                    )));
                }
                self.connections.push(RemoteConnection {
                    id: new_id.clone(),
                    name,
                    provider: ProviderConfig::Sftp(SftpConfig {
                        settings,
                        approved_host_key: None,
                    }),
                });
                new_id
            }
            Some(id) => {
                let connection = self.get_mut(&id)?;
                connection.name = name;
                let ProviderConfig::Sftp(config) = &mut connection.provider;
                let same_server = config.settings.host.eq_ignore_ascii_case(&settings.host)
                    && config.settings.port == settings.port;
                if !same_server {
                    config.approved_host_key = None;
                }
                config.settings = settings;
                id
            }
        };
        Ok(id)
    }

    pub fn remove(&mut self, id: &str) -> Result<RemoteConnection, ConnectionError> {
        let index = self
            .connections
            .iter()
            .position(|connection| connection.id == id)
            .ok_or_else(|| ConnectionError::NotFound(id.to_owned()))?;
        Ok(self.connections.remove(index))
    }

    /// Records (or with `None` forgets) the approved identity of an SFTP server.
    pub fn set_approved_host_key(
        &mut self,
        id: &str,
        key: Option<ApprovedHostKey>,
    ) -> Result<(), ConnectionError> {
        let ProviderConfig::Sftp(config) = &mut self.get_mut(id)?.provider;
        config.approved_host_key = key;
        Ok(())
    }

    /// Checks the whole configuration; nothing invalid is used or saved.
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(format!(
                "unsupported remote connections file version {}",
                self.schema_version
            ));
        }
        if self.connections.len() > MAX_CONNECTIONS {
            return Err(format!(
                "at most {MAX_CONNECTIONS} remote connections can be configured"
            ));
        }
        for (index, connection) in self.connections.iter().enumerate() {
            if !is_connection_id(&connection.id) {
                return Err(format!("invalid connection id '{}'", connection.id));
            }
            let previous = &self.connections[..index];
            if previous.iter().any(|other| other.id == connection.id) {
                return Err(format!("repeated connection id '{}'", connection.id));
            }
            check_name(&connection.name)?;
            if previous
                .iter()
                .any(|other| other.name.to_lowercase() == connection.name.to_lowercase())
            {
                return Err(format!(
                    "another connection is already named '{}'",
                    connection.name
                ));
            }
            match &connection.provider {
                ProviderConfig::Sftp(config) => check_sftp(config)?,
            }
        }
        Ok(())
    }
}

fn check_name(name: &str) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err("the connection name is required".to_owned());
    }
    if name.chars().count() > MAX_NAME_LENGTH {
        return Err(format!(
            "a connection name has at most {MAX_NAME_LENGTH} characters"
        ));
    }
    if name.trim() != name || name.chars().any(char::is_control) {
        return Err(format!("invalid connection name '{name}'"));
    }
    Ok(())
}

/// Host names, IPv4 and IPv6 addresses (optionally in brackets, with a zone).
fn check_host(host: &str) -> Result<(), String> {
    if host.is_empty() {
        return Err("the host is required".to_owned());
    }
    let valid = host.len() <= MAX_HOST_LENGTH
        && host.chars().all(|c| {
            c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ':' | '[' | ']' | '%')
        })
        && !host.starts_with('-');
    if !valid {
        return Err(format!(
            "'{host}' is not a valid host name or IP address (for example nas.local or 192.168.1.10)"
        ));
    }
    Ok(())
}

fn check_sftp(config: &SftpConfig) -> Result<(), String> {
    let settings = &config.settings;
    check_host(&settings.host)?;
    if settings.port == 0 {
        return Err("the port must be between 1 and 65535".to_owned());
    }
    let username = &settings.username;
    if username.is_empty() {
        return Err("the username is required".to_owned());
    }
    if username.chars().count() > MAX_USERNAME_LENGTH
        || username.trim() != username
        || username.chars().any(char::is_control)
    {
        return Err(format!("invalid username '{username}'"));
    }
    if settings.private_key_path.as_os_str().is_empty() {
        return Err("the private key file is required".to_owned());
    }
    if !settings.private_key_path.is_absolute() {
        return Err("the private key path must be absolute".to_owned());
    }
    if let Some(key) = &config.approved_host_key
        && (!key.fingerprint.starts_with("SHA256:")
            || key.public_key.is_empty()
            || key.algorithm.is_empty())
    {
        return Err("the approved server identity is malformed".to_owned());
    }
    Ok(())
}

#[derive(Debug)]
pub enum ConnectionsStoreError {
    Read { path: PathBuf, source: io::Error },
    Invalid { path: PathBuf, message: String },
    Write { path: PathBuf, source: io::Error },
}

impl fmt::Display for ConnectionsStoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, source } => {
                write!(f, "failed to read '{}': {source}", path.display())
            }
            Self::Invalid { path, message } => write!(
                f,
                "the remote connections file '{}' is invalid: {message}",
                path.display()
            ),
            Self::Write { path, source } => {
                write!(f, "failed to write '{}': {source}", path.display())
            }
        }
    }
}

impl std::error::Error for ConnectionsStoreError {}

/// `remote-connections.json` in the configuration directory. An invalid file
/// is reported and never replaced implicitly, since sources reference it.
#[derive(Debug, Clone)]
pub struct ConnectionsStore {
    file: PathBuf,
}

impl ConnectionsStore {
    pub fn in_dir(dir: &Path) -> Self {
        Self {
            file: dir.join(CONNECTIONS_FILE_NAME),
        }
    }

    /// Loads the connections; a missing file means none.
    pub fn load(&self) -> Result<ConnectionsConfig, ConnectionsStoreError> {
        let content = match fs::read(&self.file) {
            Ok(content) => content,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(ConnectionsConfig::default());
            }
            Err(source) => {
                return Err(ConnectionsStoreError::Read {
                    path: self.file.clone(),
                    source,
                });
            }
        };
        let invalid = |message: String| ConnectionsStoreError::Invalid {
            path: self.file.clone(),
            message,
        };
        let config: ConnectionsConfig =
            serde_json::from_slice(&content).map_err(|error| invalid(error.to_string()))?;
        config.validate().map_err(invalid)?;
        Ok(config)
    }

    /// Validates and saves the connections atomically.
    pub fn save(&self, config: &ConnectionsConfig) -> Result<(), ConnectionsStoreError> {
        let invalid = |message: String| ConnectionsStoreError::Invalid {
            path: self.file.clone(),
            message,
        };
        config.validate().map_err(invalid)?;
        let mut content =
            serde_json::to_vec_pretty(config).map_err(|error| invalid(error.to_string()))?;
        content.push(b'\n');
        atomic_file::write(&self.file, &content).map_err(|source| ConnectionsStoreError::Write {
            path: self.file.clone(),
            source,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID_A: &str = "0123456789abcdef0123456789abcdef";
    const ID_B: &str = "fedcba9876543210fedcba9876543210";

    fn key_path() -> PathBuf {
        if cfg!(windows) {
            PathBuf::from("C:\\Users\\me\\.ssh\\id_ed25519")
        } else {
            PathBuf::from("/home/me/.ssh/id_ed25519")
        }
    }

    fn input(id: Option<&str>, name: &str, host: &str) -> ConnectionInput {
        ConnectionInput {
            id: id.map(str::to_owned),
            name: name.to_owned(),
            provider: ProviderInput::Sftp(SftpSettings {
                host: host.to_owned(),
                port: 22,
                username: "mame".to_owned(),
                private_key_path: key_path(),
                host_key_verification: HostKeyVerification::Ymdb,
            }),
        }
    }

    fn approved() -> ApprovedHostKey {
        ApprovedHostKey {
            algorithm: "ssh-ed25519".to_owned(),
            fingerprint: "SHA256:abc".to_owned(),
            public_key: "ssh-ed25519 AAAA".to_owned(),
            approved_at: 1,
        }
    }

    fn sftp(connection: &RemoteConnection) -> &SftpConfig {
        let ProviderConfig::Sftp(config) = &connection.provider;
        config
    }

    #[test]
    fn creates_edits_and_removes_connections() {
        let mut config = ConnectionsConfig::default();
        let id = config
            .upsert(input(None, "  Home NAS ", " nas.local "), ID_A.to_owned())
            .expect("create");
        assert_eq!(id, ID_A);
        config.validate().expect("valid");
        let connection = config.get(ID_A).expect("created");
        assert_eq!(connection.name, "Home NAS");
        assert_eq!(sftp(connection).settings.host, "nas.local");

        // Editing the name keeps the approved identity of the same server.
        config
            .set_approved_host_key(ID_A, Some(approved()))
            .expect("approve");
        config
            .upsert(input(Some(ID_A), "NAS", "NAS.local"), ID_B.to_owned())
            .expect("rename");
        assert_eq!(config.connections.len(), 1);
        assert!(
            sftp(config.get(ID_A).expect("kept"))
                .approved_host_key
                .is_some()
        );

        // Another server: the identity is forgotten.
        config
            .upsert(input(Some(ID_A), "NAS", "192.168.1.10"), ID_B.to_owned())
            .expect("change host");
        assert!(
            sftp(config.get(ID_A).expect("kept"))
                .approved_host_key
                .is_none()
        );

        assert_eq!(
            config.upsert(input(Some(ID_B), "x", "h"), ID_B.to_owned()),
            Err(ConnectionError::NotFound(ID_B.to_owned()))
        );
        assert_eq!(config.remove(ID_A).expect("remove").id, ID_A);
        assert!(config.connections.is_empty());
        assert!(config.remove(ID_A).is_err());
    }

    #[test]
    fn validates_fields() {
        type Change = Box<dyn Fn(&mut ConnectionsConfig)>;
        let edit = |change: fn(&mut SftpSettings)| -> Change {
            Box::new(move |config: &mut ConnectionsConfig| {
                let ProviderConfig::Sftp(sftp) = &mut config.connections[0].provider;
                change(&mut sftp.settings);
            })
        };
        let cases: Vec<(&str, Change)> = vec![
            (
                "empty name",
                Box::new(|c| c.connections[0].name = " ".to_owned()),
            ),
            (
                "long name",
                Box::new(|c| c.connections[0].name = "n".repeat(61)),
            ),
            (
                "duplicate name",
                Box::new(|c| c.connections[1].name = "home nas".to_owned()),
            ),
            (
                "bad id",
                Box::new(|c| c.connections[0].id = "nas".to_owned()),
            ),
            (
                "repeated id",
                Box::new(|c| c.connections[1].id = ID_A.to_owned()),
            ),
            ("empty host", edit(|s| s.host = String::new())),
            ("host with space", edit(|s| s.host = "my nas".to_owned())),
            ("host with user", edit(|s| s.host = "me@nas".to_owned())),
            (
                "host as option",
                edit(|s| s.host = "-oProxyCommand".to_owned()),
            ),
            ("port zero", edit(|s| s.port = 0)),
            ("empty user", edit(|s| s.username = String::new())),
            (
                "user with newline",
                edit(|s| s.username = "a\nb".to_owned()),
            ),
            ("no key", edit(|s| s.private_key_path = PathBuf::new())),
            (
                "relative key",
                edit(|s| s.private_key_path = PathBuf::from("id_ed25519")),
            ),
            (
                "bad identity",
                Box::new(|c| {
                    let ProviderConfig::Sftp(sftp) = &mut c.connections[0].provider;
                    sftp.approved_host_key = Some(ApprovedHostKey {
                        fingerprint: "MD5:aa".to_owned(),
                        ..approved()
                    });
                }),
            ),
            ("future version", Box::new(|c| c.schema_version = 2)),
        ];
        for (label, change) in cases {
            let mut config = ConnectionsConfig::default();
            config
                .upsert(input(None, "Home NAS", "nas.local"), ID_A.to_owned())
                .expect("a");
            config
                .upsert(input(None, "Backup", "[fe80::1%eth0]"), ID_B.to_owned())
                .expect("b");
            config.validate().expect("valid before the change");
            change(&mut config);
            assert!(config.validate().is_err(), "{label}");
        }
    }

    #[test]
    fn persists_in_the_documented_format() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = ConnectionsStore::in_dir(&dir.path().join("config ✓"));
        assert_eq!(store.load().expect("missing"), ConnectionsConfig::default());

        let mut config = ConnectionsConfig::default();
        config
            .upsert(input(None, "Home NAS", "nas.local"), ID_A.to_owned())
            .expect("create");
        config
            .set_approved_host_key(ID_A, Some(approved()))
            .expect("approve");
        store.save(&config).expect("save");
        assert_eq!(store.load().expect("reload"), config);

        let json: serde_json::Value = serde_json::from_slice(
            &fs::read(dir.path().join("config ✓").join(CONNECTIONS_FILE_NAME)).expect("read"),
        )
        .expect("json");
        assert_eq!(json["schemaVersion"], 1);
        let connection = &json["connections"][0];
        assert_eq!(connection["id"], ID_A);
        assert_eq!(connection["name"], "Home NAS");
        assert_eq!(connection["providerType"], "sftp");
        assert_eq!(connection["providerConfig"]["host"], "nas.local");
        assert_eq!(connection["providerConfig"]["port"], 22);
        assert_eq!(connection["providerConfig"]["username"], "mame");
        assert_eq!(connection["providerConfig"]["hostKeyVerification"], "ymdb");
        assert_eq!(
            connection["providerConfig"]["approvedHostKey"]["fingerprint"],
            "SHA256:abc"
        );
        // Optional fields take their defaults.
        let minimal = format!(
            r#"{{ "connections": [{{ "id": "{ID_A}", "name": "n", "providerType": "sftp",
                "providerConfig": {{ "host": "h", "username": "u", "privateKeyPath": {} }} }}] }}"#,
            serde_json::to_string(&key_path()).expect("path")
        );
        let parsed: ConnectionsConfig = serde_json::from_str(&minimal).expect("minimal");
        parsed.validate().expect("valid");
        let config = sftp(&parsed.connections[0]);
        assert_eq!(config.settings.port, DEFAULT_SSH_PORT);
        assert_eq!(
            config.settings.host_key_verification,
            HostKeyVerification::Ymdb
        );
        assert!(config.approved_host_key.is_none());
    }

    #[test]
    fn reports_invalid_files_and_unknown_providers() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = ConnectionsStore::in_dir(dir.path());
        for content in [
            "{ not json",
            r#"{ "schemaVersion": 99 }"#,
            r#"{ "connections": [{ "id": "x", "name": "n", "providerType": "ftp", "providerConfig": {} }] }"#,
        ] {
            fs::write(dir.path().join(CONNECTIONS_FILE_NAME), content).expect("write");
            assert!(
                matches!(store.load(), Err(ConnectionsStoreError::Invalid { .. })),
                "{content}"
            );
        }
    }
}
