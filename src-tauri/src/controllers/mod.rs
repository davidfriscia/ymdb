//! Controller management: controllers, controller profiles and their
//! assignments to machines and software, global to YMDB.
//!
//! MAME stays in charge of input configuration: its own menus assign inputs
//! and it saves them in its configuration directory (`default.cfg`,
//! `<machine>.cfg`). YMDB only decides which directory and which controller
//! configuration file a launch uses, and never writes MAME's `cfg` files.
//!
//! - A **controller** is the logical identity of a physical device (name,
//!   kind, MAME device class, and for a mapped joystick its MAME device id,
//!   joystick provider and binding: `device` when the id follows the device,
//!   `portOrder` when it follows the port or enumeration order).
//!   Controllers assigned to players are mapped to the MAME joystick index of
//!   their player with a generated controller file ([`ctrlr_file`]).
//! - A **controller profile** is a reusable input configuration. The system
//!   profile `mame` ("MAME configuration") passes nothing to MAME, which keeps
//!   the behavior of YMDB without profiles. A user profile owns one MAME
//!   configuration directory per installation
//!   (`<config>/installations/<id>/controller-profiles/<profileId>/cfg`) and an
//!   optional baseline controller file
//!   (`<config>/controller-profiles/<profileId>/base.cfg`).
//! - **Assignments** select a profile and controllers for every launch
//!   (default), a machine, a software or a machine running a software; a
//!   launch request may override them temporarily. Each field is inherited
//!   separately (see [`resolve`]).
//!
//! - [`model`]: entities, validation and changes.
//! - [`store`]: `controllers.json` and the `base.cfg` files.
//! - [`base_file`]: validation of imported controller configuration files.
//! - [`resolve`]: the effective profile and controllers of a launch.
//! - [`devices`]: joystick providers and what their device ids identify.
//! - [`discovery`]: on-demand detection of the connected joysticks with MAME.
//! - [`ctrlr_file`]: the generated controller file of player mappings.

pub mod base_file;
pub mod ctrlr_file;
pub mod devices;
pub mod discovery;
pub mod model;
pub mod resolve;
pub mod store;

use std::fs;
use std::path::{Path, PathBuf};

const PROFILES_DIR: &str = "controller-profiles";
const INSTALLATIONS_DIR: &str = "installations";
const GENERATED_DIR: &str = "controller-files";
/// Name of the baseline controller file, passed to MAME as `-ctrlr base`.
pub const BASE_NAME: &str = "base";

/// Locations of the files of controller profiles. Ids joined into paths must
/// be valid profile ids ([`model::is_id`]) and registered installation ids.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControllerPaths {
    config_dir: PathBuf,
    generated_dir: PathBuf,
}

impl ControllerPaths {
    /// Generated controller files go to `<config_dir>/controller-files`
    /// unless [`Self::with_data_dir`] chooses the local data directory.
    pub fn new(config_dir: &Path) -> Self {
        Self {
            config_dir: config_dir.to_path_buf(),
            generated_dir: config_dir.join(GENERATED_DIR),
        }
    }

    /// Generated controller files are derived data: they belong in the
    /// local (non-roaming) data directory.
    pub fn with_data_dir(mut self, data_dir: &Path) -> Self {
        self.generated_dir = data_dir.join(GENERATED_DIR);
        self
    }

    /// Directory of the generated controller files (named by their
    /// content); passed to MAME as `-ctrlrpath` when devices are mapped.
    pub fn generated_dir(&self) -> &Path {
        &self.generated_dir
    }

    /// Directory of the files of a profile shared by every installation;
    /// passed to MAME as `-ctrlrpath`.
    pub fn profile_dir(&self, profile_id: &str) -> PathBuf {
        self.config_dir.join(PROFILES_DIR).join(profile_id)
    }

    pub fn base_file(&self, profile_id: &str) -> PathBuf {
        self.profile_dir(profile_id)
            .join(format!("{BASE_NAME}.cfg"))
    }

    /// MAME configuration directory of a profile for one installation;
    /// passed to MAME as `-cfg_directory`. Only MAME writes inside it.
    pub fn context_dir(&self, installation_id: &str, profile_id: &str) -> PathBuf {
        self.config_dir
            .join(INSTALLATIONS_DIR)
            .join(installation_id)
            .join(PROFILES_DIR)
            .join(profile_id)
            .join("cfg")
    }

    /// Whether files of a profile with this id exist (for example kept after
    /// the profile was deleted), in any installation.
    pub fn is_used(&self, profile_id: &str) -> bool {
        if self.profile_dir(profile_id).exists() {
            return true;
        }
        fs::read_dir(self.config_dir.join(INSTALLATIONS_DIR))
            .map(|entries| {
                entries
                    .flatten()
                    .any(|entry| entry.path().join(PROFILES_DIR).join(profile_id).exists())
            })
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests;
