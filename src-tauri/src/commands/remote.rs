//! Remote connection and remote source commands.
//!
//! Connections are global (`remote-connections.json`); sources belong to one
//! installation (`installations/<id>/remote-sources.json`) and reference
//! connections by id. Changes to both are serialized by one lock, so a
//! source can never be saved with a connection that is being removed.
//!
//! Network operations (test, browse) are explicit user actions, run on the
//! async runtime with time limits and never hold the lock. Passphrases are
//! used for one connection and dropped; they are never stored or logged.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use super::installations::{InstallationsState, ResolvedInstallation};
use super::{CommandError, run_blocking};
use crate::installations::new_id;
use crate::launcher::options::OptionValue;
use crate::mame::ini::{NativeSearchPaths, read_search_paths, resolve_entry};
use crate::mame::platform;
use crate::media::config::{SourceStatus, source_status};
use crate::remote::connections::{
    ApprovedHostKey, ConnectionError, ConnectionInput, ConnectionsConfig, ConnectionsStore,
    ProviderConfig, ProviderType, RemoteConnection,
};
use crate::remote::path::RemotePath;
use crate::remote::provider::{
    Capabilities, ConnectOptions, ConnectionInfo, RemoteEntry, RemoteError, RemoteRepository,
    connect,
};
use crate::remote::sources::{
    CATEGORIES, CollectionOrganization, ConnectionReference, LocalDestination, RemoteSourcesConfig,
    SearchPath, SourceCategory, SourcesStoreError,
};

/// Listings longer than this are cut (the interface only browses).
const MAX_LISTED_ENTRIES: usize = 2000;

/// Managed state of remote repositories.
pub struct RemoteState {
    /// `None` when the configuration directory is unavailable.
    store: Option<ConnectionsStore>,
    /// Serializes changes to the connections and to every installation's sources.
    lock: Mutex<()>,
    /// Server identities shown to the user and not approved yet, by connection id.
    pending: Mutex<HashMap<String, PendingHostKey>>,
    /// The user's OpenSSH `known_hosts`, read only.
    known_hosts: Option<PathBuf>,
}

#[derive(Debug, Clone)]
struct PendingHostKey {
    host: String,
    port: u16,
    algorithm: String,
    fingerprint: String,
    public_key: String,
}

impl RemoteState {
    pub fn new(config_dir: Option<&Path>) -> Self {
        Self {
            store: config_dir.map(ConnectionsStore::in_dir),
            lock: Mutex::new(()),
            pending: Mutex::new(HashMap::new()),
            known_hosts: std::env::home_dir().map(|home| home.join(".ssh").join("known_hosts")),
        }
    }

    fn store(&self) -> Result<&ConnectionsStore, CommandError> {
        self.store.as_ref().ok_or_else(|| CommandError::Settings {
            message: "the application configuration directory is unavailable".to_owned(),
        })
    }

    fn load(&self) -> Result<ConnectionsConfig, CommandError> {
        self.store()?
            .load()
            .map_err(|error| CommandError::Settings {
                message: error.to_string(),
            })
    }

    /// Applies `change` to the connections and saves them, under the lock.
    fn update<T>(
        &self,
        change: impl FnOnce(&mut ConnectionsConfig) -> Result<T, CommandError>,
    ) -> Result<T, CommandError> {
        let _guard = self.lock.lock().unwrap_or_else(PoisonError::into_inner);
        let mut config = self.load()?;
        let value = change(&mut config)?;
        config
            .validate()
            .map_err(|message| CommandError::InvalidRequest { message })?;
        self.store()?
            .save(&config)
            .map_err(|error| CommandError::Settings {
                message: error.to_string(),
            })?;
        Ok(value)
    }

    fn connection(&self, id: &str) -> Result<RemoteConnection, CommandError> {
        self.load()?
            .get(id)
            .cloned()
            .ok_or_else(|| connection_not_found(id))
    }

    fn connect_options(&self, passphrase: Option<String>) -> ConnectOptions {
        ConnectOptions {
            passphrase: crate::remote::provider::Passphrase::new(passphrase),
            known_hosts: self.known_hosts.clone(),
            ..ConnectOptions::default()
        }
    }

    fn forget_pending(&self, connection_id: &str) {
        self.pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(connection_id);
    }

    fn remember_pending(&self, connection_id: &str, error: &RemoteError) {
        let mut pending = self.pending.lock().unwrap_or_else(PoisonError::into_inner);
        match error {
            RemoteError::HostKeyUnknown {
                host,
                port,
                algorithm,
                fingerprint,
                public_key,
            } => {
                pending.insert(
                    connection_id.to_owned(),
                    PendingHostKey {
                        host: host.clone(),
                        port: *port,
                        algorithm: algorithm.clone(),
                        fingerprint: fingerprint.clone(),
                        public_key: public_key.clone(),
                    },
                );
            }
            _ => {
                pending.remove(connection_id);
            }
        }
    }
}

fn connection_not_found(id: &str) -> CommandError {
    CommandError::ConnectionNotFound {
        connection_id: id.to_owned(),
    }
}

fn connection_error(error: ConnectionError) -> CommandError {
    match error {
        ConnectionError::NotFound(connection_id) => {
            CommandError::ConnectionNotFound { connection_id }
        }
        ConnectionError::Invalid(message) => CommandError::InvalidRequest { message },
    }
}

/// A connection as shown to the user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionSummary {
    #[serde(flatten)]
    pub connection: RemoteConnection,
    /// Whether the private key file exists (its content is only read when connecting).
    pub private_key_found: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionsView {
    pub connections: Vec<ConnectionSummary>,
    /// Set when `remote-connections.json` cannot be read. It is never
    /// replaced implicitly: changes are refused until it is fixed.
    pub load_error: Option<String>,
}

fn connections_view(state: &RemoteState) -> ConnectionsView {
    match state.load() {
        Ok(config) => ConnectionsView {
            connections: config
                .connections
                .into_iter()
                .map(|connection| {
                    let ProviderConfig::Sftp(sftp) = &connection.provider;
                    let private_key_found = sftp.settings.private_key_path.is_file();
                    ConnectionSummary {
                        connection,
                        private_key_found,
                    }
                })
                .collect(),
            load_error: None,
        },
        Err(error) => ConnectionsView {
            connections: Vec::new(),
            load_error: Some(match error {
                CommandError::Settings { message } => message,
                other => format!("{other:?}"),
            }),
        },
    }
}

/// Outcome of a connection test. A failure is an expected result, not a
/// command error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "status",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ConnectionTestResult {
    Success { info: ConnectionInfo },
    Failure { error: RemoteError },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteListing {
    /// The directory listed (normalized).
    pub path: String,
    pub parent: Option<String>,
    pub entries: Vec<RemoteEntry>,
    /// Whether entries beyond the limit were left out.
    pub truncated: bool,
    pub capabilities: Capabilities,
}

#[tauri::command]
pub async fn get_remote_connections(app: AppHandle) -> Result<ConnectionsView, CommandError> {
    use tauri::Manager;
    // Checks key files, possibly on a network share: off the main thread.
    run_blocking(move || connections_view(&app.state::<RemoteState>())).await
}

/// Creates (without id) or updates a connection. Editing the host or port
/// forgets the approved server identity.
#[tauri::command]
pub fn save_remote_connection(
    connection: ConnectionInput,
    remote: State<'_, RemoteState>,
) -> Result<ConnectionsView, CommandError> {
    save_connection(&remote, connection)
}

/// Removes a connection that no remote source references. References are
/// never removed implicitly: the error lists them.
#[tauri::command]
pub fn remove_remote_connection(
    connection_id: String,
    remote: State<'_, RemoteState>,
    installations: State<'_, InstallationsState>,
) -> Result<ConnectionsView, CommandError> {
    remove_connection(&remote, &installations, &connection_id)
}

/// Connects for real (resolution, handshake, server identity,
/// authentication, SFTP) and disconnects.
#[tauri::command]
pub async fn test_remote_connection(
    connection_id: String,
    passphrase: Option<String>,
    remote: State<'_, RemoteState>,
) -> Result<ConnectionTestResult, CommandError> {
    test_connection(&remote, &connection_id, passphrase).await
}

/// Approves the server identity shown by the last test of a connection.
/// Only an identity actually presented by the server can be approved.
#[tauri::command]
pub fn approve_remote_host_key(
    connection_id: String,
    fingerprint: String,
    remote: State<'_, RemoteState>,
) -> Result<ConnectionsView, CommandError> {
    approve_host_key(&remote, &connection_id, &fingerprint)
}

/// Forgets the approved server identity; the next connection asks again.
#[tauri::command]
pub fn forget_remote_host_key(
    connection_id: String,
    remote: State<'_, RemoteState>,
) -> Result<ConnectionsView, CommandError> {
    remote.update(|config| {
        config
            .set_approved_host_key(&connection_id, None)
            .map_err(connection_error)
    })?;
    Ok(connections_view(&remote))
}

/// Lists a remote directory (read only). An empty path lists the directory
/// the server starts in.
#[tauri::command]
pub async fn browse_remote_directory(
    connection_id: String,
    path: String,
    passphrase: Option<String>,
    remote: State<'_, RemoteState>,
) -> Result<RemoteListing, CommandError> {
    browse(&remote, &connection_id, &path, passphrase).await
}

fn save_connection(
    remote: &RemoteState,
    connection: ConnectionInput,
) -> Result<ConnectionsView, CommandError> {
    let id = remote.update(|config| {
        config
            .upsert(connection, new_id())
            .map_err(connection_error)
    })?;
    remote.forget_pending(&id);
    Ok(connections_view(remote))
}

fn remove_connection(
    remote: &RemoteState,
    installations: &InstallationsState,
    connection_id: &str,
) -> Result<ConnectionsView, CommandError> {
    remote.update(|config| {
        if config.get(connection_id).is_none() {
            return Err(connection_not_found(connection_id));
        }
        let references = references_to(installations, connection_id)?;
        if !references.is_empty() {
            return Err(CommandError::ConnectionInUse { references });
        }
        config.remove(connection_id).map_err(connection_error)?;
        Ok(())
    })?;
    remote.forget_pending(connection_id);
    Ok(connections_view(remote))
}

/// Sources of registered installations referencing a connection. A sources
/// file that cannot be read makes the check fail, rather than guess.
fn references_to(
    installations: &InstallationsState,
    connection_id: &str,
) -> Result<Vec<ConnectionReference>, CommandError> {
    let ids = installations.with_registry(|registry| {
        registry
            .installations
            .iter()
            .map(|installation| installation.id.clone())
            .collect::<Vec<_>>()
    })?;
    let mut references = Vec::new();
    for id in ids {
        let resolved = installations.resolve(&id)?;
        let sources = resolved
            .remote_sources
            .load()
            .map_err(|error| CommandError::Settings {
                message: format!(
                    "the remote sources of {} cannot be read, so the connection is kept: {error}",
                    resolved.installation.label()
                ),
            })?;
        references.extend(
            sources
                .categories_using(connection_id)
                .into_iter()
                .map(|category| ConnectionReference {
                    installation_id: id.clone(),
                    installation_label: resolved.installation.label(),
                    category,
                }),
        );
    }
    Ok(references)
}

async fn test_connection(
    remote: &RemoteState,
    connection_id: &str,
    passphrase: Option<String>,
) -> Result<ConnectionTestResult, CommandError> {
    let connection = remote.connection(connection_id)?;
    let options = remote.connect_options(passphrase);
    let result = match connect(&connection, &options).await {
        Ok((repository, info)) => {
            repository.close().await;
            remote.forget_pending(connection_id);
            ConnectionTestResult::Success { info }
        }
        Err(error) => {
            remote.remember_pending(connection_id, &error);
            ConnectionTestResult::Failure { error }
        }
    };
    Ok(result)
}

fn approve_host_key(
    remote: &RemoteState,
    connection_id: &str,
    fingerprint: &str,
) -> Result<ConnectionsView, CommandError> {
    let pending = remote
        .pending
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(connection_id)
        .cloned()
        .filter(|pending| pending.fingerprint == fingerprint)
        .ok_or_else(|| CommandError::InvalidRequest {
            message: "this server identity was not presented by the last connection test; test the connection again".to_owned(),
        })?;
    remote.update(|config| {
        let connection = config
            .get(connection_id)
            .ok_or_else(|| connection_not_found(connection_id))?;
        let ProviderConfig::Sftp(sftp) = &connection.provider;
        if !sftp.settings.host.eq_ignore_ascii_case(&pending.host)
            || sftp.settings.port != pending.port
        {
            return Err(CommandError::InvalidRequest {
                message: "the connection changed since the test; test it again".to_owned(),
            });
        }
        config
            .set_approved_host_key(
                connection_id,
                Some(ApprovedHostKey {
                    algorithm: pending.algorithm.clone(),
                    fingerprint: pending.fingerprint.clone(),
                    public_key: pending.public_key.clone(),
                    approved_at: now_millis(),
                }),
            )
            .map_err(connection_error)
    })?;
    remote.forget_pending(connection_id);
    Ok(connections_view(remote))
}

async fn browse(
    remote: &RemoteState,
    connection_id: &str,
    path: &str,
    passphrase: Option<String>,
) -> Result<RemoteListing, CommandError> {
    let requested = if path.trim().is_empty() {
        None
    } else {
        Some(
            RemotePath::parse(path).map_err(|error| CommandError::InvalidRequest {
                message: error.to_string(),
            })?,
        )
    };
    let connection = remote.connection(connection_id)?;
    let options = remote.connect_options(passphrase);
    let (repository, info) = connect(&connection, &options)
        .await
        .map_err(|error| CommandError::Remote { error })?;
    let directory = requested.unwrap_or_else(|| {
        info.home_directory
            .as_deref()
            .and_then(|home| RemotePath::parse(home).ok())
            .unwrap_or_else(RemotePath::root)
    });
    let capabilities = repository.capabilities();
    let result = if capabilities.list_directory {
        repository.list_directory(&directory).await
    } else {
        Err(RemoteError::Unsupported {
            operation: "directory listing".to_owned(),
        })
    };
    repository.close().await;
    let mut entries = result.map_err(|error| CommandError::Remote { error })?;
    let truncated = entries.len() > MAX_LISTED_ENTRIES;
    entries.truncate(MAX_LISTED_ENTRIES);
    Ok(RemoteListing {
        parent: directory.parent().map(|parent| parent.to_string()),
        path: directory.to_string(),
        entries,
        truncated,
        capabilities,
    })
}

/// Native file picker for a private key. `None` when cancelled.
#[tauri::command]
pub async fn pick_private_key_file(app: AppHandle) -> Result<Option<PathBuf>, CommandError> {
    let ssh_dir = std::env::home_dir()
        .map(|home| home.join(".ssh"))
        .filter(|dir| dir.is_dir());
    pick(app, "Select a private key file", ssh_dir, false).await
}

/// Native directory picker for a local destination. `None` when cancelled.
#[tauri::command]
pub async fn pick_destination_directory(app: AppHandle) -> Result<Option<PathBuf>, CommandError> {
    pick(
        app,
        "Select a local directory for downloaded files",
        None,
        true,
    )
    .await
}

async fn pick(
    app: AppHandle,
    title: &'static str,
    start: Option<PathBuf>,
    folder: bool,
) -> Result<Option<PathBuf>, CommandError> {
    let selection = run_blocking(move || {
        let mut dialog = app.dialog().file().set_title(title);
        if let Some(start) = start {
            dialog = dialog.set_directory(start);
        }
        if folder {
            dialog.blocking_pick_folder()
        } else {
            dialog.blocking_pick_file()
        }
    })
    .await?;
    selection
        .map(|path| path.into_path())
        .transpose()
        .map_err(|error| CommandError::Internal {
            message: format!("unsupported selection: {error}"),
        })
}

/// Where the MAME directories offered as destinations come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DirectoriesOrigin {
    /// The global options of the installation's launch configuration.
    LaunchOptions,
    /// `mame.ini`.
    MameIni,
    /// MAME's built-in default (no `mame.ini` found).
    MameDefault,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MameDirectory {
    /// As written in the configuration (may be relative).
    pub entry: String,
    /// The directory it names, relative to the MAME directory.
    pub path: PathBuf,
    pub status: SourceStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchPathDirectories {
    pub search_path: SearchPath,
    pub origin: DirectoriesOrigin,
    pub directories: Vec<MameDirectory>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryInfo {
    pub id: SourceCategory,
    pub label: &'static str,
    pub search_path: SearchPath,
    pub organizations: &'static [CollectionOrganization],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionChoice {
    pub id: String,
    pub name: String,
    pub provider_type: ProviderType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceWarning {
    pub category: SourceCategory,
    pub message: String,
}

/// The remote sources of one installation, with what the editor needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteSourcesView {
    pub installation_id: String,
    pub config: RemoteSourcesConfig,
    /// Set when the file cannot be read: `config` is then empty and saving
    /// replaces the file.
    pub load_error: Option<String>,
    pub categories: Vec<CategoryInfo>,
    pub connections: Vec<ConnectionChoice>,
    pub mame_directories: Vec<SearchPathDirectories>,
    pub ini_file: Option<PathBuf>,
    pub warnings: Vec<SourceWarning>,
}

#[tauri::command]
pub async fn get_remote_sources(
    installation_id: String,
    app: AppHandle,
) -> Result<RemoteSourcesView, CommandError> {
    use tauri::Manager;
    // Checks local directories, possibly on network shares: off the main thread.
    run_blocking(move || {
        load_sources(
            &app.state::<InstallationsState>(),
            &app.state::<RemoteState>(),
            &installation_id,
        )
    })
    .await?
}

/// Validates and saves the remote sources of one installation. Other
/// installations are not affected.
#[tauri::command]
pub async fn save_remote_sources(
    installation_id: String,
    config: RemoteSourcesConfig,
    app: AppHandle,
) -> Result<RemoteSourcesView, CommandError> {
    use tauri::Manager;
    run_blocking(move || {
        save_sources(
            &app.state::<InstallationsState>(),
            &app.state::<RemoteState>(),
            &installation_id,
            config,
        )
    })
    .await?
}

fn load_sources(
    installations: &InstallationsState,
    remote: &RemoteState,
    installation_id: &str,
) -> Result<RemoteSourcesView, CommandError> {
    let resolved = installations.resolve(installation_id)?;
    let (config, load_error) = match resolved.remote_sources.load() {
        Ok(config) => (config, None),
        Err(error) => (RemoteSourcesConfig::default(), Some(error.to_string())),
    };
    sources_view(installations, remote, &resolved, config, load_error)
}

fn save_sources(
    installations: &InstallationsState,
    remote: &RemoteState,
    installation_id: &str,
    config: RemoteSourcesConfig,
) -> Result<RemoteSourcesView, CommandError> {
    let resolved = installations.resolve(installation_id)?;
    let saved = {
        let _guard = remote.lock.lock().unwrap_or_else(PoisonError::into_inner);
        config
            .validate()
            .map_err(|message| CommandError::InvalidRequest { message })?;
        config
            .check_references(&remote.load()?)
            .map_err(|message| CommandError::InvalidRequest { message })?;
        resolved
            .remote_sources
            .save(config)
            .map_err(|error| match error {
                SourcesStoreError::Rejected(message) => CommandError::InvalidRequest { message },
                other => CommandError::Settings {
                    message: other.to_string(),
                },
            })?
    };
    sources_view(installations, remote, &resolved, saved, None)
}

fn sources_view(
    installations: &InstallationsState,
    remote: &RemoteState,
    resolved: &ResolvedInstallation,
    config: RemoteSourcesConfig,
    load_error: Option<String>,
) -> Result<RemoteSourcesView, CommandError> {
    let connections = remote
        .load()
        .map(|config| config.connections)
        .unwrap_or_default();
    let mame_dir = mame_dir(resolved);
    let native = read_search_paths(&mame_dir);
    let global = resolved
        .launch_config
        .load()
        .map(|config| config.global_options)
        .unwrap_or_default();
    let mame_directories = [SearchPath::Rom, SearchPath::Sample]
        .into_iter()
        .map(|search_path| mame_directories(&mame_dir, &native, &global, search_path))
        .collect();
    let mut warnings = Vec::new();
    for (category, source) in &config.categories {
        if let Some(id) = &source.connection_id
            && !connections.iter().any(|connection| &connection.id == id)
        {
            warnings.push(SourceWarning {
                category: *category,
                message: "The remote connection no longer exists. Choose another one.".to_owned(),
            });
        }
        if let Some(LocalDestination::Custom { directory }) = &source.destination
            && source_status(directory) != SourceStatus::Available
        {
            warnings.push(SourceWarning {
                category: *category,
                message: format!(
                    "The local directory {} is not available.",
                    directory.display()
                ),
            });
        }
    }
    warnings.extend(shared_destination_warnings(
        installations,
        resolved,
        &config,
    )?);
    Ok(RemoteSourcesView {
        installation_id: resolved.installation.id.clone(),
        config,
        load_error,
        categories: CATEGORIES
            .iter()
            .map(|category| CategoryInfo {
                id: *category,
                label: category.label(),
                search_path: category.search_path(),
                organizations: category.organizations(),
            })
            .collect(),
        connections: connections
            .iter()
            .map(|connection| ConnectionChoice {
                id: connection.id.clone(),
                name: connection.name.clone(),
                provider_type: connection.provider.provider_type(),
            })
            .collect(),
        mame_directories,
        ini_file: native.ini_file,
        warnings,
    })
}

fn mame_dir(resolved: &ResolvedInstallation) -> PathBuf {
    resolved
        .installation
        .executable_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default()
}

/// The directories MAME searches for a category: the installation's global
/// launch option when set, otherwise `mame.ini` or MAME's default.
fn mame_directories(
    mame_dir: &Path,
    native: &NativeSearchPaths,
    global: &crate::launcher::options::OptionSet,
    search_path: SearchPath,
) -> SearchPathDirectories {
    let (origin, entries) = match global.get(search_path.option_name()) {
        Some(OptionValue::List(entries)) => (DirectoriesOrigin::LaunchOptions, entries.clone()),
        _ => (
            if native.ini_file.is_some() {
                DirectoriesOrigin::MameIni
            } else {
                DirectoriesOrigin::MameDefault
            },
            match search_path {
                SearchPath::Rom => native.rompath.clone(),
                SearchPath::Sample => native.samplepath.clone(),
            },
        ),
    };
    SearchPathDirectories {
        search_path,
        origin,
        directories: entries
            .into_iter()
            .map(|entry| {
                let path = resolve_entry(mame_dir, &entry);
                MameDirectory {
                    status: source_status(&path),
                    path,
                    entry,
                }
            })
            .collect(),
    }
}

/// The local directories of an installation's enabled sources, resolved.
fn destination_paths(
    mame_dir: &Path,
    config: &RemoteSourcesConfig,
) -> Vec<(SourceCategory, PathBuf)> {
    config
        .categories
        .iter()
        .filter(|(_, source)| source.enabled)
        .filter_map(|(category, source)| {
            let path = match source.destination.as_ref()? {
                LocalDestination::MameDirectory { directory } => resolve_entry(mame_dir, directory),
                LocalDestination::Custom { directory } => directory.clone(),
            };
            Some((*category, path))
        })
        .collect()
}

/// Destinations also used by other installations. Sharing is allowed, but
/// files with the same name may differ between MAME versions.
fn shared_destination_warnings(
    installations: &InstallationsState,
    resolved: &ResolvedInstallation,
    config: &RemoteSourcesConfig,
) -> Result<Vec<SourceWarning>, CommandError> {
    let own = destination_paths(&mame_dir(resolved), config);
    if own.is_empty() {
        return Ok(Vec::new());
    }
    let others = installations.with_registry(|registry| {
        registry
            .installations
            .iter()
            .filter(|installation| installation.id != resolved.installation.id)
            .map(|installation| installation.id.clone())
            .collect::<Vec<_>>()
    })?;
    let mut warnings = Vec::new();
    for id in others {
        let other = installations.resolve(&id)?;
        let Ok(other_config) = other.remote_sources.load() else {
            continue;
        };
        let theirs = destination_paths(&mame_dir(&other), &other_config);
        for (category, path) in &own {
            for (other_category, other_path) in &theirs {
                if platform::same_path(path, other_path) {
                    warnings.push(SourceWarning {
                        category: *category,
                        message: format!(
                            "{} also downloads {} into this directory. Files with the same name can differ between MAME versions; YMDB never overwrites existing files.",
                            other.installation.label(),
                            other_category.label()
                        ),
                    });
                }
            }
        }
    }
    Ok(warnings)
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests;
