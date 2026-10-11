//! Registered MAME installations.
//!
//! YMDB manages any number of MAME installations side by side. Each one is
//! identified by an opaque, stable id (never by its version, since two
//! installations may have the same version) and owns its own files:
//!
//! - its catalog, `<data>/installations/<id>/catalog.sqlite`;
//! - its launch configuration, `<config>/installations/<id>/launch.json`;
//! - its remote sources, `<config>/installations/<id>/remote-sources.json`.
//!
//! Media sources, remote connections and interface preferences are shared by
//! all installations.
//!
//! - [`registry`]: the installations, the active one and their rules.
//! - [`store`]: `installations.json` persistence.
//! - [`migration`]: upgrade from the single-installation layout.

pub mod migration;
pub mod registry;
pub mod store;

use std::path::{Path, PathBuf};

/// Locations of the application files.
#[derive(Debug, Clone)]
pub struct AppLayout {
    /// Configuration directory (`settings.json`, `media.json`, launch configurations).
    pub config_dir: PathBuf,
    /// Local data directory (catalogs, session logs).
    pub data_dir: PathBuf,
}

const INSTALLATIONS_DIR: &str = "installations";

impl AppLayout {
    pub fn new(config_dir: &Path, data_dir: &Path) -> Self {
        Self {
            config_dir: config_dir.to_path_buf(),
            data_dir: data_dir.to_path_buf(),
        }
    }

    /// Directory of the catalog of an installation. `id` must be a registered id.
    pub fn catalog_dir(&self, id: &str) -> PathBuf {
        self.data_dir.join(INSTALLATIONS_DIR).join(id)
    }

    /// Configuration directory of an installation (`launch.json`,
    /// `remote-sources.json`). `id` must be a registered id.
    pub fn launch_config_dir(&self, id: &str) -> PathBuf {
        self.config_dir.join(INSTALLATIONS_DIR).join(id)
    }
}

/// A new installation id: 32 lowercase hexadecimal digits (random UUID).
pub fn new_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

/// Whether `id` has the format of installation ids. Ids are only used as path
/// segments after being found in the registry; this check rejects anything
/// else early.
pub fn is_installation_id(id: &str) -> bool {
    id.len() == 32
        && id
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
}

#[cfg(test)]
mod tests;
