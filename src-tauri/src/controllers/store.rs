//! `controllers.json` and the files YMDB owns for controller profiles.
//!
//! `controllers.json` lives in the configuration directory, is written
//! atomically and validated when loaded. A missing file is the initial state
//! (the `mame` profile, nothing else) and is not created until something
//! changes; an invalid or unreadable file is reported and never replaced
//! implicitly, since it holds data entered by the user.
//!
//! The `base.cfg` of a profile is a verbatim copy of an imported controller
//! configuration file, under `controller-profiles/<profileId>/`. MAME only
//! reads it; YMDB writes it only when the user imports a file.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::ControllerPaths;
use super::model::ControllersConfig;
use crate::atomic_file;

pub const CONTROLLERS_FILE_NAME: &str = "controllers.json";

#[derive(Debug)]
pub enum ControllersStoreError {
    Read { path: PathBuf, source: io::Error },
    Invalid { path: PathBuf, message: String },
    Write { path: PathBuf, source: io::Error },
}

impl fmt::Display for ControllersStoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, source } => {
                write!(f, "failed to read '{}': {source}", path.display())
            }
            Self::Invalid { path, message } => write!(
                f,
                "the controllers file '{}' is invalid: {message}",
                path.display()
            ),
            Self::Write { path, source } => {
                write!(f, "failed to write '{}': {source}", path.display())
            }
        }
    }
}

impl std::error::Error for ControllersStoreError {}

#[derive(Debug, Clone)]
pub struct ControllersStore {
    file: PathBuf,
    paths: ControllerPaths,
}

impl ControllersStore {
    /// The store of the configuration directory `dir`.
    pub fn in_dir(dir: &Path) -> Self {
        Self {
            file: dir.join(CONTROLLERS_FILE_NAME),
            paths: ControllerPaths::new(dir),
        }
    }

    pub fn paths(&self) -> &ControllerPaths {
        &self.paths
    }

    /// Loads the configuration; the system profiles are added when missing
    /// (nothing is written until the configuration changes).
    pub fn load(&self) -> Result<ControllersConfig, ControllersStoreError> {
        let content = match fs::read(&self.file) {
            Ok(content) => content,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(ControllersConfig::new());
            }
            Err(source) => {
                return Err(ControllersStoreError::Read {
                    path: self.file.clone(),
                    source,
                });
            }
        };
        let invalid = |message: String| ControllersStoreError::Invalid {
            path: self.file.clone(),
            message,
        };
        let mut config: ControllersConfig =
            serde_json::from_slice(&content).map_err(|error| invalid(error.to_string()))?;
        config.ensure_system_profiles();
        config.validate().map_err(invalid)?;
        Ok(config)
    }

    /// Validates and saves the configuration atomically.
    pub fn save(&self, config: &ControllersConfig) -> Result<(), ControllersStoreError> {
        let invalid = |message: String| ControllersStoreError::Invalid {
            path: self.file.clone(),
            message,
        };
        config.validate().map_err(invalid)?;
        let mut content =
            serde_json::to_vec_pretty(config).map_err(|error| invalid(error.to_string()))?;
        content.push(b'\n');
        atomic_file::write(&self.file, &content).map_err(|source| ControllersStoreError::Write {
            path: self.file.clone(),
            source,
        })
    }

    /// Writes the `base.cfg` of a profile atomically (replacing a previous
    /// one). `content` must have been validated.
    pub fn write_base(&self, profile_id: &str, content: &[u8]) -> io::Result<()> {
        atomic_file::write(&self.paths.base_file(profile_id), content)
    }

    /// Removes a `base.cfg` written by [`Self::write_base`] whose profile
    /// could not be saved. Best effort: an orphan file is harmless.
    pub fn discard_base(&self, profile_id: &str) {
        let _ = fs::remove_file(self.paths.base_file(profile_id));
        let _ = fs::remove_dir(self.paths.profile_dir(profile_id));
    }

    /// First step of the removal of a profile's `base.cfg`: moves it aside,
    /// next to it, so that it can be restored if the removal cannot be saved.
    /// Returns the moved file, or `None` when the profile has no `base.cfg`.
    pub fn set_base_aside(&self, profile_id: &str) -> io::Result<Option<PathBuf>> {
        let base = self.paths.base_file(profile_id);
        let aside = base.with_extension("cfg.removed");
        match fs::rename(&base, &aside) {
            Ok(()) => Ok(Some(aside)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }

    /// Puts back a `base.cfg` moved by [`Self::set_base_aside`].
    pub fn restore_base(&self, profile_id: &str, aside: &Path) -> io::Result<()> {
        fs::rename(aside, self.paths.base_file(profile_id))
    }

    /// Deletes a `base.cfg` moved by [`Self::set_base_aside`] once its
    /// removal is saved, and the profile's directory if nothing else is in
    /// it. Best effort: a leftover file is never read.
    pub fn delete_base_aside(&self, profile_id: &str, aside: &Path) {
        let _ = fs::remove_file(aside);
        let _ = fs::remove_dir(self.paths.profile_dir(profile_id));
    }
}
