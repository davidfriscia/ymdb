//! The registered installations and the active one.
//!
//! Rules:
//!
//! - An installation is identified by its id, never by its version: several
//!   installations may have the same version with different executables.
//! - The same executable cannot be registered twice; paths are compared with
//!   the platform's rules (see `mame::platform::same_executable`).
//! - The executable path of an installation never changes. Another executable
//!   is another installation, so a catalog is never associated with a build it
//!   was not imported from.
//! - Adding an installation never changes the others; the first one added
//!   becomes active.
//! - When the active installation is removed, the first remaining one in
//!   registration order becomes active (none when the list is empty).

use std::collections::HashSet;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};

use super::is_installation_id;
use crate::mame::platform;
use crate::mame::version::MameVersion;

pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_DISPLAY_NAME_LENGTH: usize = 60;
pub const MAX_INSTALLATIONS: usize = 200;

/// A registered MAME installation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MameInstallation {
    pub id: String,
    pub executable_path: PathBuf,
    /// Name chosen by the user; `None` shows the version.
    #[serde(default)]
    pub display_name: Option<String>,
    /// Version found by the last successful verification.
    #[serde(default)]
    pub version: Option<MameVersion>,
    /// The executable file as it was when its version was read, to detect
    /// a replaced or updated file without running it.
    #[serde(default)]
    pub identity: Option<ExecutableIdentity>,
    /// Milliseconds since the Unix epoch.
    pub added_at: u64,
    /// Last successful verification, in milliseconds since the Unix epoch.
    #[serde(default)]
    pub verified_at: Option<u64>,
}

/// Size and modification time of an executable file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutableIdentity {
    pub size: u64,
    /// Milliseconds since the Unix epoch.
    pub modified_at: Option<u64>,
}

impl ExecutableIdentity {
    /// Reads the identity of a file, or `None` when it does not exist.
    pub fn read(path: &Path) -> Option<Self> {
        let metadata = fs::metadata(path).ok().filter(fs::Metadata::is_file)?;
        Some(Self {
            size: metadata.len(),
            modified_at: metadata
                .modified()
                .ok()
                .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                .and_then(|elapsed| u64::try_from(elapsed.as_millis()).ok()),
        })
    }
}

/// State of an executable, known without running it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ExecutableStatus {
    /// Unchanged since its version was read.
    Verified,
    /// Its version has never been read.
    NotVerified,
    /// The file was replaced or modified since its version was read.
    Changed,
    /// The file no longer exists.
    Missing,
}

impl MameInstallation {
    pub fn new(id: String, executable_path: PathBuf, added_at: u64) -> Self {
        Self {
            id,
            executable_path,
            display_name: None,
            version: None,
            identity: None,
            added_at,
            verified_at: None,
        }
    }

    /// The name shown to the user.
    pub fn label(&self) -> String {
        match (&self.display_name, &self.version) {
            (Some(name), _) => name.clone(),
            (None, Some(version)) => format!("MAME {}", version.release),
            (None, None) => "MAME (version unknown)".to_owned(),
        }
    }

    pub fn executable_status(&self) -> ExecutableStatus {
        match (
            ExecutableIdentity::read(&self.executable_path),
            self.identity,
        ) {
            (None, _) => ExecutableStatus::Missing,
            (Some(_), None) => ExecutableStatus::NotVerified,
            (Some(current), Some(recorded)) if current == recorded => ExecutableStatus::Verified,
            (Some(_), Some(_)) => ExecutableStatus::Changed,
        }
    }
}

/// Content of `installations.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallationRegistry {
    #[serde(default = "schema_version")]
    pub schema_version: u32,
    /// In registration order.
    #[serde(default)]
    pub installations: Vec<MameInstallation>,
    #[serde(default)]
    pub active_installation_id: Option<String>,
    /// Record of the upgrade from the single-installation layout, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legacy_migration: Option<LegacyMigration>,
}

fn schema_version() -> u32 {
    SCHEMA_VERSION
}

impl Default for InstallationRegistry {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            installations: Vec::new(),
            active_installation_id: None,
            legacy_migration: None,
        }
    }
}

/// Progress of the migration from the single-installation layout (see
/// `migration`). It is recorded before any file is moved, so an interrupted
/// migration resumes with the same installations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyMigration {
    pub state: MigrationState,
    /// Installation receiving the existing catalog, with the values used to
    /// verify it after the move.
    #[serde(default)]
    pub catalog: Option<MigratedCatalog>,
    /// Installation receiving the existing `launch.json`.
    #[serde(default)]
    pub launch_config_installation_id: Option<String>,
    pub started_at: u64,
    #[serde(default)]
    pub completed_at: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MigrationState {
    Pending,
    Completed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigratedCatalog {
    pub installation_id: String,
    /// `None` when the catalog could not be read before the move.
    #[serde(default)]
    pub mame_release: Option<String>,
    #[serde(default)]
    pub machines: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryError {
    NotFound(String),
    /// The executable is already registered by this installation.
    Duplicate(String),
    Invalid(String),
}

impl fmt::Display for RegistryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound(id) => write!(f, "MAME installation '{id}' is not registered"),
            Self::Duplicate(id) => {
                write!(
                    f,
                    "this executable is already registered (installation '{id}')"
                )
            }
            Self::Invalid(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for RegistryError {}

impl InstallationRegistry {
    pub fn get(&self, id: &str) -> Option<&MameInstallation> {
        self.installations
            .iter()
            .find(|installation| installation.id == id)
    }

    fn get_mut(&mut self, id: &str) -> Result<&mut MameInstallation, RegistryError> {
        self.installations
            .iter_mut()
            .find(|installation| installation.id == id)
            .ok_or_else(|| RegistryError::NotFound(id.to_owned()))
    }

    pub fn active(&self) -> Option<&MameInstallation> {
        self.active_installation_id
            .as_deref()
            .and_then(|id| self.get(id))
    }

    /// The installation registered for an executable, by the platform's path rules.
    pub fn find_by_executable(&self, path: &Path) -> Option<&MameInstallation> {
        self.installations
            .iter()
            .find(|installation| platform::same_executable(&installation.executable_path, path))
    }

    /// Registers an installation. The first one becomes active; otherwise the
    /// active installation does not change.
    pub fn add(&mut self, installation: MameInstallation) -> Result<(), RegistryError> {
        if let Some(existing) = self.find_by_executable(&installation.executable_path) {
            return Err(RegistryError::Duplicate(existing.id.clone()));
        }
        if self.installations.len() >= MAX_INSTALLATIONS {
            return Err(RegistryError::Invalid(format!(
                "at most {MAX_INSTALLATIONS} installations can be registered"
            )));
        }
        if !is_installation_id(&installation.id) || self.get(&installation.id).is_some() {
            return Err(RegistryError::Invalid(format!(
                "invalid installation id '{}'",
                installation.id
            )));
        }
        if self.active_installation_id.is_none() {
            self.active_installation_id = Some(installation.id.clone());
        }
        self.installations.push(installation);
        Ok(())
    }

    /// Unregisters an installation. When it was active, the first remaining
    /// installation in registration order becomes active.
    pub fn remove(&mut self, id: &str) -> Result<MameInstallation, RegistryError> {
        let index = self
            .installations
            .iter()
            .position(|installation| installation.id == id)
            .ok_or_else(|| RegistryError::NotFound(id.to_owned()))?;
        let removed = self.installations.remove(index);
        if self.active_installation_id.as_deref() == Some(id) {
            self.active_installation_id = self
                .installations
                .first()
                .map(|installation| installation.id.clone());
        }
        Ok(removed)
    }

    pub fn set_active(&mut self, id: &str) -> Result<(), RegistryError> {
        self.get_mut(id)?;
        self.active_installation_id = Some(id.to_owned());
        Ok(())
    }

    /// Sets or clears (`None` or blank) the display name.
    pub fn rename(&mut self, id: &str, name: Option<&str>) -> Result<(), RegistryError> {
        let name = name.map(str::trim).filter(|name| !name.is_empty());
        if let Some(name) = name {
            check_display_name(name).map_err(RegistryError::Invalid)?;
        }
        self.get_mut(id)?.display_name = name.map(str::to_owned);
        Ok(())
    }

    /// Records the result of a successful verification.
    pub fn record_verification(
        &mut self,
        id: &str,
        version: MameVersion,
        identity: Option<ExecutableIdentity>,
        verified_at: u64,
    ) -> Result<(), RegistryError> {
        let installation = self.get_mut(id)?;
        installation.version = Some(version);
        installation.identity = identity;
        installation.verified_at = Some(verified_at);
        Ok(())
    }

    /// Checks a loaded registry; nothing invalid is used or saved.
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(format!(
                "unsupported installations file version {}",
                self.schema_version
            ));
        }
        let mut ids = HashSet::new();
        for (index, installation) in self.installations.iter().enumerate() {
            if !is_installation_id(&installation.id) || !ids.insert(installation.id.as_str()) {
                return Err(format!("invalid or repeated id '{}'", installation.id));
            }
            if !installation.executable_path.is_absolute() {
                return Err(format!(
                    "the executable path of '{}' is not absolute",
                    installation.id
                ));
            }
            if self.installations[..index].iter().any(|other| {
                platform::same_executable(&other.executable_path, &installation.executable_path)
            }) {
                return Err(format!(
                    "'{}' is registered more than once",
                    installation.executable_path.display()
                ));
            }
            if let Some(name) = &installation.display_name {
                check_display_name(name)?;
            }
        }
        if let Some(active) = &self.active_installation_id
            && self.get(active).is_none()
        {
            return Err(format!(
                "the active installation '{active}' is not registered"
            ));
        }
        Ok(())
    }
}

fn check_display_name(name: &str) -> Result<(), String> {
    if name.chars().count() > MAX_DISPLAY_NAME_LENGTH {
        return Err(format!(
            "an installation name has at most {MAX_DISPLAY_NAME_LENGTH} characters"
        ));
    }
    if name.trim() != name || name.is_empty() || name.chars().any(char::is_control) {
        return Err(format!("invalid installation name '{name}'"));
    }
    Ok(())
}
