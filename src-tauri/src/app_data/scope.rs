//! What a backup contains and what a restore replaces.
//!
//! A backup lists files by logical path: `config/...` or `data/...` below
//! one of the two roots, plus `interface-preferences.json` (the webview's
//! interface preferences, sent by the frontend). Only the paths accepted by
//! [`classify`] may appear in an archive, so an archive can never write
//! anywhere else; their category is derived from the path, never trusted.
//!
//! A restore replaces the whole [`replaced_names`] of each root (moved to a
//! rollback copy first), including what a backup does not contain, such as
//! classification projections, which are rebuilt from the restored catalogs
//! and support files.

use serde::{Deserialize, Serialize};

use super::Root;
use crate::catalog::store::CATALOG_FILE_NAME;
use crate::controllers::model::is_id as is_profile_id;
use crate::controllers::store::CONTROLLERS_FILE_NAME;
use crate::installations::is_installation_id;
use crate::installations::store::REGISTRY_FILE_NAME;
use crate::launcher::store::LAUNCH_FILE_NAME;
use crate::lists::store::LISTS_FILE_NAME;
use crate::media::config::MEDIA_FILE_NAME;
use crate::remote::connections::CONNECTIONS_FILE_NAME;
use crate::remote::sources::SOURCES_FILE_NAME;
use crate::saved_filters::SAVED_FILTERS_FILE_NAME;
use crate::settings::SETTINGS_FILE_NAME;
use crate::support_files::repository::REPOSITORY_FILE_NAME;

/// Per-installation directories, in both roots.
pub const INSTALLATIONS_DIR: &str = "installations";
/// Controller profile files shared by every installation (`base.cfg`).
pub const CONTROLLER_PROFILES_DIR: &str = "controller-profiles";
/// Logical path of the interface preferences.
pub const INTERFACE_PREFERENCES: &str = "interface-preferences.json";
/// Work directory of a restore, in each root. Never part of a backup.
pub const RESTORE_WORK_DIR: &str = ".ymdb-restore";
/// Work directory of an export, in the data root. Never part of a backup.
pub const EXPORT_WORK_DIR: &str = ".ymdb-export";

/// Files of the configuration root included in a backup. `settings.json`
/// and the top-level `launch.json` belong to the single-installation layout:
/// the migration keeps them as a copy of the old configuration.
pub const CONFIG_FILES: [&str; 8] = [
    REGISTRY_FILE_NAME,
    CONTROLLERS_FILE_NAME,
    LISTS_FILE_NAME,
    SAVED_FILTERS_FILE_NAME,
    MEDIA_FILE_NAME,
    CONNECTIONS_FILE_NAME,
    SETTINGS_FILE_NAME,
    LAUNCH_FILE_NAME,
];

/// Names, at the top of each root, that a restore replaces as a whole.
pub fn replaced_names(root: Root) -> Vec<String> {
    match root {
        Root::Config => CONFIG_FILES
            .iter()
            .copied()
            .chain([INSTALLATIONS_DIR, CONTROLLER_PROFILES_DIR])
            .map(str::to_owned)
            .collect(),
        // The journal files of the repository must move with it: a stale
        // journal next to another database would corrupt it.
        Root::Data => [
            INSTALLATIONS_DIR.to_owned(),
            REPOSITORY_FILE_NAME.to_owned(),
            format!("{REPOSITORY_FILE_NAME}-journal"),
            format!("{REPOSITORY_FILE_NAME}-wal"),
            format!("{REPOSITORY_FILE_NAME}-shm"),
        ]
        .into(),
    }
}

/// Kind of data, for the manifest and the import preview.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Category {
    Installations,
    LaunchConfigurations,
    RomManagerSources,
    ControllerProfileConfigurations,
    InstallationFiles,
    Controllers,
    ControllerBaseFiles,
    Lists,
    SavedFilters,
    MediaSources,
    RemoteConnections,
    LegacyConfiguration,
    Catalogs,
    SupportFiles,
    InterfacePreferences,
}

impl Category {
    pub fn label(self) -> &'static str {
        match self {
            Self::Installations => "MAME installations",
            Self::LaunchConfigurations => "Launch configurations",
            Self::RomManagerSources => "ROM Manager sources",
            Self::ControllerProfileConfigurations => "Controller profile MAME configurations",
            Self::InstallationFiles => "Other installation files",
            Self::Controllers => "Controllers, profiles and assignments",
            Self::ControllerBaseFiles => "Controller profile base files",
            Self::Lists => "Favorites and lists",
            Self::SavedFilters => "Saved filters",
            Self::MediaSources => "Media sources",
            Self::RemoteConnections => "Remote connections",
            Self::LegacyConfiguration => "Configuration of the single-installation layout",
            Self::Catalogs => "Catalogs",
            Self::SupportFiles => "Support files",
            Self::InterfacePreferences => "Interface preferences",
        }
    }
}

/// How an entry is copied into a backup.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ItemKind {
    /// Copied byte for byte.
    File,
    /// A consistent copy made with the SQLite backup API.
    Sqlite,
}

/// A logical path checked by [`classify`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Classified {
    /// `None` for the interface preferences, which are not a file of a root.
    pub root: Option<Root>,
    /// Components below the root.
    pub components: Vec<String>,
    pub category: Category,
    pub kind: ItemKind,
}

/// Checks a logical path and returns its location and category, or `None`
/// when a backup may not contain it.
pub fn classify(path: &str) -> Option<Classified> {
    if path == INTERFACE_PREFERENCES {
        return Some(Classified {
            root: None,
            components: vec![INTERFACE_PREFERENCES.to_owned()],
            category: Category::InterfacePreferences,
            kind: ItemKind::File,
        });
    }
    let mut parts = path.split('/');
    let root = match parts.next()? {
        "config" => Root::Config,
        "data" => Root::Data,
        _ => return None,
    };
    let components: Vec<String> = parts.map(str::to_owned).collect();
    if components.is_empty()
        || components.len() > MAX_DEPTH
        || !components
            .iter()
            .all(|component| is_safe_component(component))
    {
        return None;
    }
    let names: Vec<&str> = components.iter().map(String::as_str).collect();
    let (category, kind) = match (root, names.as_slice()) {
        (Root::Config, [file]) => (config_file_category(file)?, ItemKind::File),
        (Root::Config, [INSTALLATIONS_DIR, id, rest @ ..])
            if is_installation_id(id) && !rest.is_empty() =>
        {
            let category = match rest {
                [LAUNCH_FILE_NAME] => Category::LaunchConfigurations,
                [SOURCES_FILE_NAME] => Category::RomManagerSources,
                [CONTROLLER_PROFILES_DIR, profile, _, ..] if is_profile_id(profile) => {
                    Category::ControllerProfileConfigurations
                }
                [CONTROLLER_PROFILES_DIR, ..] => return None,
                _ => Category::InstallationFiles,
            };
            (category, ItemKind::File)
        }
        (Root::Config, [CONTROLLER_PROFILES_DIR, profile, _, ..]) if is_profile_id(profile) => {
            (Category::ControllerBaseFiles, ItemKind::File)
        }
        (Root::Data, [INSTALLATIONS_DIR, id, CATALOG_FILE_NAME]) if is_installation_id(id) => {
            (Category::Catalogs, ItemKind::Sqlite)
        }
        (Root::Data, [REPOSITORY_FILE_NAME]) => (Category::SupportFiles, ItemKind::Sqlite),
        _ => return None,
    };
    Some(Classified {
        root: Some(root),
        components,
        category,
        kind,
    })
}

fn config_file_category(name: &str) -> Option<Category> {
    Some(match name {
        REGISTRY_FILE_NAME => Category::Installations,
        CONTROLLERS_FILE_NAME => Category::Controllers,
        LISTS_FILE_NAME => Category::Lists,
        SAVED_FILTERS_FILE_NAME => Category::SavedFilters,
        MEDIA_FILE_NAME => Category::MediaSources,
        CONNECTIONS_FILE_NAME => Category::RemoteConnections,
        SETTINGS_FILE_NAME | LAUNCH_FILE_NAME => Category::LegacyConfiguration,
        _ => return None,
    })
}

/// Deepest accepted path below a root (MAME may create subdirectories in
/// the configuration directories of controller profiles).
const MAX_DEPTH: usize = 12;
const MAX_COMPONENT_LENGTH: usize = 200;

/// A file or directory name that is valid and means the same on every
/// supported platform: no separators, no `.`/`..`, no drive or stream
/// syntax, no control characters, no trailing dot or space, no reserved
/// Windows device names.
pub fn is_safe_component(name: &str) -> bool {
    const RESERVED: [&str; 22] = [
        "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
        "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
    ];
    if name.is_empty()
        || name.len() > MAX_COMPONENT_LENGTH
        || name == "."
        || name == ".."
        || name.ends_with('.')
        || name.ends_with(' ')
        || name.chars().any(|character| {
            character.is_control()
                || matches!(
                    character,
                    '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'
                )
        })
    {
        return false;
    }
    let stem = name.split('.').next().unwrap_or(name).to_ascii_lowercase();
    !RESERVED.contains(&stem.as_str())
}

/// Whether a file met while walking a directory is a leftover of an
/// interrupted atomic write, which is never part of a backup.
pub fn is_temporary_file(name: &str) -> bool {
    name.ends_with(".tmp")
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "0123456789abcdef0123456789abcdef";

    #[test]
    fn classifies_the_files_a_backup_may_contain() {
        let cases = [
            (
                "config/installations.json",
                Category::Installations,
                ItemKind::File,
            ),
            ("config/lists.json", Category::Lists, ItemKind::File),
            (
                "config/settings.json",
                Category::LegacyConfiguration,
                ItemKind::File,
            ),
            (
                &format!("config/installations/{ID}/launch.json"),
                Category::LaunchConfigurations,
                ItemKind::File,
            ),
            (
                &format!("config/installations/{ID}/remote-sources.json"),
                Category::RomManagerSources,
                ItemKind::File,
            ),
            (
                &format!("config/installations/{ID}/controller-profiles/pad-1/cfg/sf2.cfg"),
                Category::ControllerProfileConfigurations,
                ItemKind::File,
            ),
            (
                "config/controller-profiles/pad-1/base.cfg",
                Category::ControllerBaseFiles,
                ItemKind::File,
            ),
            (
                &format!("data/installations/{ID}/catalog.sqlite"),
                Category::Catalogs,
                ItemKind::Sqlite,
            ),
            (
                "data/support-files.sqlite",
                Category::SupportFiles,
                ItemKind::Sqlite,
            ),
            (
                "interface-preferences.json",
                Category::InterfacePreferences,
                ItemKind::File,
            ),
        ];
        for (path, category, kind) in cases {
            let classified = classify(path).unwrap_or_else(|| panic!("{path}"));
            assert_eq!(
                (classified.category, classified.kind),
                (category, kind),
                "{path}"
            );
        }
    }

    #[test]
    fn rejects_paths_outside_the_backup_scope() {
        for path in [
            "",
            "/config/lists.json",
            "config",
            "config/",
            "config//lists.json",
            "config/../lists.json",
            "config/./lists.json",
            "config/window-state.json",
            "config/unknown.json",
            "other/lists.json",
            "C:/config/lists.json",
            "config\\lists.json",
            "config/installations/not-an-id/launch.json",
            &format!("config/installations/{ID}"),
            &format!("config/installations/{ID}/controller-profiles/pad-1"),
            &format!("config/installations/{ID}/controller-profiles/../cfg/x.cfg"),
            "config/controller-profiles/pad-1",
            "config/controller-profiles/-bad/base.cfg",
            &format!("data/installations/{ID}/classifications.sqlite"),
            &format!("data/installations/{ID}/catalog.sqlite.partial"),
            "data/launch-logs/1.log",
            "data/controller-files/ymdb-1.cfg",
            "data/.ymdb-restore/staged/support-files.sqlite",
            "config/controller-profiles/pad-1/con.cfg",
            "config/controller-profiles/pad-1/a:b",
            "config/controller-profiles/pad-1/trailing.",
        ] {
            assert_eq!(classify(path), None, "{path}");
        }
    }

    #[test]
    fn safe_components() {
        for name in ["sf2.cfg", "default.cfg", "a b.cfg", "日本.cfg", ".hidden"] {
            assert!(is_safe_component(name), "{name}");
        }
        for name in [
            "", ".", "..", "a/b", "a\\b", "c:", "x\0", "NUL", "com1.txt", "x ",
        ] {
            assert!(!is_safe_component(name), "{name:?}");
        }
    }
}
