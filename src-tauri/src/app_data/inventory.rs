//! The inventory of the files and directories YMDB uses, for the
//! Application Data settings section.
//!
//! Every location comes from the same code that the application uses
//! (stores, `AppLayout`, the backup scope), never from fixed paths.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::scope::{self, CONTROLLER_PROFILES_DIR, INSTALLATIONS_DIR};
use super::{AppDirs, Root};
use crate::catalog::store::CATALOG_FILE_NAME;
use crate::controllers::store::CONTROLLERS_FILE_NAME;
use crate::installations::store::{REGISTRY_FILE_NAME, RegistryStore};
use crate::launcher::store::LAUNCH_FILE_NAME;
use crate::lists::store::LISTS_FILE_NAME;
use crate::media::config::{MEDIA_FILE_NAME, MediaConfigStore};
use crate::remote::connections::{CONNECTIONS_FILE_NAME, ConnectionsStore, ProviderConfig};
use crate::remote::sources::{LocalDestination, RemoteSourcesStore};
use crate::saved_filters::SAVED_FILTERS_FILE_NAME;
use crate::settings::SETTINGS_FILE_NAME;
use crate::support_files::repository::REPOSITORY_FILE_NAME;
use crate::window_state::WINDOW_STATE_FILE_NAME;

/// Who is responsible for a piece of data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DataClass {
    /// Data YMDB is responsible for; included in a backup.
    Owned,
    /// Rebuilt by YMDB when needed.
    Regenerable,
    /// Logs, work and partial files.
    Transient,
    /// Left by an older version; no longer used.
    Legacy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum EntryKind {
    File,
    Directory,
}

/// One location of the inventory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DataEntry {
    /// Stable id, used to open the location.
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub path: PathBuf,
    pub kind: EntryKind,
    pub class: DataClass,
    pub exists: bool,
    /// Bytes, for files and directory trees.
    pub size: Option<u64>,
    /// Files in a directory tree.
    pub files: Option<u64>,
    pub in_backup: bool,
    /// Why the entry is not part of a backup.
    pub exclusion_reason: Option<&'static str>,
}

/// A file or directory outside YMDB's data that its configuration refers to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalReference {
    pub kind: ExternalKind,
    /// What refers to it (an installation, a media category, a connection).
    pub owner: String,
    pub path: PathBuf,
    pub exists: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ExternalKind {
    MameExecutable,
    MediaDirectory,
    RomDestination,
    PrivateKey,
}

struct Definition {
    id: &'static str,
    name: &'static str,
    description: &'static str,
    root: Root,
    relative: &'static str,
    kind: EntryKind,
    class: DataClass,
    exclusion_reason: Option<&'static str>,
}

const DEFINITIONS: &[Definition] = &[
    Definition {
        id: "installations",
        name: "MAME installations",
        description: "The registered MAME executables and the active installation.",
        root: Root::Config,
        relative: REGISTRY_FILE_NAME,
        kind: EntryKind::File,
        class: DataClass::Owned,
        exclusion_reason: None,
    },
    Definition {
        id: "installationSettings",
        name: "Installation settings",
        description: "Per installation: launch configuration (launch.json), ROM Manager sources \
                      (remote-sources.json) and the MAME configuration directories of controller \
                      profiles.",
        root: Root::Config,
        relative: INSTALLATIONS_DIR,
        kind: EntryKind::Directory,
        class: DataClass::Owned,
        exclusion_reason: None,
    },
    Definition {
        id: "controllers",
        name: "Controllers",
        description: "Controllers, controller profiles and their assignments.",
        root: Root::Config,
        relative: CONTROLLERS_FILE_NAME,
        kind: EntryKind::File,
        class: DataClass::Owned,
        exclusion_reason: None,
    },
    Definition {
        id: "controllerProfiles",
        name: "Controller profile files",
        description: "Imported baseline controller files (base.cfg) of controller profiles.",
        root: Root::Config,
        relative: CONTROLLER_PROFILES_DIR,
        kind: EntryKind::Directory,
        class: DataClass::Owned,
        exclusion_reason: None,
    },
    Definition {
        id: "lists",
        name: "Favorites and lists",
        description: "Favorites and user lists, by MAME machine and software names.",
        root: Root::Config,
        relative: LISTS_FILE_NAME,
        kind: EntryKind::File,
        class: DataClass::Owned,
        exclusion_reason: None,
    },
    Definition {
        id: "savedFilters",
        name: "Saved filters",
        description: "Catalog browser filter presets.",
        root: Root::Config,
        relative: SAVED_FILTERS_FILE_NAME,
        kind: EntryKind::File,
        class: DataClass::Owned,
        exclusion_reason: None,
    },
    Definition {
        id: "mediaSources",
        name: "Media sources",
        description: "Directories searched for artwork, by media category.",
        root: Root::Config,
        relative: MEDIA_FILE_NAME,
        kind: EntryKind::File,
        class: DataClass::Owned,
        exclusion_reason: None,
    },
    Definition {
        id: "remoteConnections",
        name: "Remote connections",
        description: "Remote repository connections: host, user, private key path and approved \
                      server identity. No password, passphrase or key is stored.",
        root: Root::Config,
        relative: CONNECTIONS_FILE_NAME,
        kind: EntryKind::File,
        class: DataClass::Owned,
        exclusion_reason: None,
    },
    Definition {
        id: "legacySettings",
        name: "Legacy settings",
        description: "Configuration of the single-installation layout, kept by the migration as \
                      a copy of the old configuration.",
        root: Root::Config,
        relative: SETTINGS_FILE_NAME,
        kind: EntryKind::File,
        class: DataClass::Owned,
        exclusion_reason: None,
    },
    Definition {
        id: "legacyLaunchConfig",
        name: "Legacy launch configuration",
        description: "Launch configuration of the single-installation layout, kept by the \
                      migration as a copy of the old configuration.",
        root: Root::Config,
        relative: LAUNCH_FILE_NAME,
        kind: EntryKind::File,
        class: DataClass::Owned,
        exclusion_reason: None,
    },
    Definition {
        id: "windowState",
        name: "Window state",
        description: "Position, size and maximized state of the main window.",
        root: Root::Config,
        relative: WINDOW_STATE_FILE_NAME,
        kind: EntryKind::File,
        class: DataClass::Regenerable,
        exclusion_reason: Some("Depends on the monitors of this computer."),
    },
    Definition {
        id: "catalogs",
        name: "Catalogs",
        description: "Per installation: the catalog imported from MAME (catalog.sqlite) and its \
                      classification projection (classifications.sqlite).",
        root: Root::Data,
        relative: INSTALLATIONS_DIR,
        kind: EntryKind::Directory,
        class: DataClass::Owned,
        exclusion_reason: None,
    },
    Definition {
        id: "supportFiles",
        name: "Support files",
        description: "Imported MAME support files (catver, genre, catlist, version, series) and \
                      the active source of each classification.",
        root: Root::Data,
        relative: REPOSITORY_FILE_NAME,
        kind: EntryKind::File,
        class: DataClass::Owned,
        exclusion_reason: None,
    },
    Definition {
        id: "controllerFiles",
        name: "Generated controller files",
        description: "Controller files generated for device mappings, named by their content.",
        root: Root::Data,
        relative: "controller-files",
        kind: EntryKind::Directory,
        class: DataClass::Regenerable,
        exclusion_reason: Some("Generated again before each launch that needs them."),
    },
    Definition {
        id: "launchLogs",
        name: "Launch logs",
        description: "Output of MAME sessions started by YMDB.",
        root: Root::Data,
        relative: "launch-logs",
        kind: EntryKind::Directory,
        class: DataClass::Transient,
        exclusion_reason: Some("Diagnostic logs of past sessions."),
    },
    Definition {
        id: "legacyCatalog",
        name: "Legacy catalog",
        description: "Catalog of the single-installation layout. Catalogs now live in the \
                      directory of each installation.",
        root: Root::Data,
        relative: CATALOG_FILE_NAME,
        kind: EntryKind::File,
        class: DataClass::Legacy,
        exclusion_reason: Some("Not used once the installations are migrated."),
    },
    Definition {
        id: "exportWork",
        name: "Export work directory",
        description: "Temporary copies made while a backup is being exported.",
        root: Root::Data,
        relative: scope::EXPORT_WORK_DIR,
        kind: EntryKind::Directory,
        class: DataClass::Transient,
        exclusion_reason: Some("Removed when the export ends."),
    },
    Definition {
        id: "restoreWorkConfig",
        name: "Restore work directory (configuration)",
        description: "Staged and rollback copies of a restore in progress.",
        root: Root::Config,
        relative: scope::RESTORE_WORK_DIR,
        kind: EntryKind::Directory,
        class: DataClass::Transient,
        exclusion_reason: Some("Removed when the restore ends."),
    },
    Definition {
        id: "restoreWorkData",
        name: "Restore work directory (data)",
        description: "Staged and rollback copies of a restore in progress.",
        root: Root::Data,
        relative: scope::RESTORE_WORK_DIR,
        kind: EntryKind::Directory,
        class: DataClass::Transient,
        exclusion_reason: Some("Removed when the restore ends."),
    },
];

/// The inventory. Directory sizes are computed here: call it off the UI thread.
pub fn entries(dirs: &AppDirs, webview_dir: Option<&Path>) -> Vec<DataEntry> {
    let mut entries: Vec<DataEntry> = DEFINITIONS
        .iter()
        .map(|definition| {
            let path = dirs.root(definition.root).join(definition.relative);
            let (exists, size, files) = measure(&path);
            DataEntry {
                id: definition.id,
                name: definition.name,
                description: definition.description,
                exists,
                size,
                files: (definition.kind == EntryKind::Directory && exists).then_some(files),
                path,
                kind: definition.kind,
                class: definition.class,
                in_backup: definition.exclusion_reason.is_none(),
                exclusion_reason: definition.exclusion_reason,
            }
        })
        .collect();
    if let Some(dir) = webview_dir {
        let (exists, size, files) = measure(dir);
        entries.push(DataEntry {
            id: "webview",
            name: "Web view data",
            description: "Storage of the system web view: interface preferences (theme, browser \
                          view options) and caches. The interface preferences are included in \
                          backups by the interface itself.",
            path: dir.to_path_buf(),
            kind: EntryKind::Directory,
            class: DataClass::Regenerable,
            exists,
            size,
            files: exists.then_some(files),
            in_backup: false,
            exclusion_reason: Some(
                "Browser caches; the interface preferences are exported separately.",
            ),
        });
    }
    entries
}

/// The path of an entry, for "Open location".
pub fn entry_path(dirs: &AppDirs, webview_dir: Option<&Path>, id: &str) -> Option<PathBuf> {
    if id == "webview" {
        return webview_dir.map(Path::to_path_buf);
    }
    DEFINITIONS
        .iter()
        .find(|definition| definition.id == id)
        .map(|definition| dirs.root(definition.root).join(definition.relative))
}

/// Existence, size and number of files (directory trees are walked without
/// following symbolic links).
fn measure(path: &Path) -> (bool, Option<u64>, u64) {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return (false, None, 0);
    };
    if !metadata.is_dir() {
        return (true, Some(metadata.len()), 1);
    }
    let mut size = 0;
    let mut files = 0;
    let mut pending = vec![path.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir).into_iter().flatten().flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                pending.push(entry.path());
            } else if kind.is_file() {
                files += 1;
                size += entry.metadata().map_or(0, |metadata| metadata.len());
            }
        }
    }
    (true, Some(size), files)
}

/// The external files and directories referenced by the configuration in
/// `config_dir` (the live one or a staged restore), checked on this computer.
/// Unreadable files contribute nothing: they are reported where they are used.
pub fn external_references(config_dir: &Path) -> Vec<ExternalReference> {
    let mut references = Vec::new();
    let mut add = |kind, owner: String, path: PathBuf| {
        let exists = path.exists();
        references.push(ExternalReference {
            kind,
            owner,
            path,
            exists,
        });
    };
    if let Ok(Some(registry)) = RegistryStore::in_dir(config_dir).load() {
        for installation in &registry.installations {
            add(
                ExternalKind::MameExecutable,
                installation.label(),
                installation.executable_path.clone(),
            );
            let sources = RemoteSourcesStore::in_dir(
                &config_dir.join(INSTALLATIONS_DIR).join(&installation.id),
            )
            .load();
            for (category, source) in sources.iter().flat_map(|sources| &sources.categories) {
                if let Some(LocalDestination::Custom { directory }) = &source.destination {
                    add(
                        ExternalKind::RomDestination,
                        format!("{} ({category:?})", installation.label()),
                        directory.clone(),
                    );
                }
            }
        }
    }
    if let Ok(media) = MediaConfigStore::in_dir(config_dir).load() {
        for (category, directories) in &media.sources {
            for directory in directories {
                add(
                    ExternalKind::MediaDirectory,
                    category.clone(),
                    directory.clone(),
                );
            }
        }
    }
    if let Ok(connections) = ConnectionsStore::in_dir(config_dir).load() {
        for connection in &connections.connections {
            let ProviderConfig::Sftp(sftp) = &connection.provider;
            add(
                ExternalKind::PrivateKey,
                connection.name.clone(),
                sftp.settings.private_key_path.clone(),
            );
        }
    }
    references
}
