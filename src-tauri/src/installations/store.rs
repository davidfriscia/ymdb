//! `installations.json`: the persisted registry.
//!
//! It lives in the configuration directory, is written atomically and
//! validated when loaded. An invalid file is reported and never replaced
//! implicitly, since it references the catalogs and launch configurations of
//! every installation.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::registry::InstallationRegistry;
use crate::atomic_file;

pub const REGISTRY_FILE_NAME: &str = "installations.json";

#[derive(Debug)]
pub enum RegistryStoreError {
    Read { path: PathBuf, source: io::Error },
    Invalid { path: PathBuf, message: String },
    Write { path: PathBuf, source: io::Error },
}

impl fmt::Display for RegistryStoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, source } => {
                write!(f, "failed to read '{}': {source}", path.display())
            }
            Self::Invalid { path, message } => write!(
                f,
                "the installations file '{}' is invalid: {message}",
                path.display()
            ),
            Self::Write { path, source } => {
                write!(f, "failed to write '{}': {source}", path.display())
            }
        }
    }
}

impl std::error::Error for RegistryStoreError {}

#[derive(Debug, Clone)]
pub struct RegistryStore {
    file: PathBuf,
}

impl RegistryStore {
    pub fn in_dir(dir: &Path) -> Self {
        Self {
            file: dir.join(REGISTRY_FILE_NAME),
        }
    }

    /// Loads the registry, or `None` when the file does not exist yet.
    pub fn load(&self) -> Result<Option<InstallationRegistry>, RegistryStoreError> {
        let content = match fs::read(&self.file) {
            Ok(content) => content,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(source) => {
                return Err(RegistryStoreError::Read {
                    path: self.file.clone(),
                    source,
                });
            }
        };
        let invalid = |message: String| RegistryStoreError::Invalid {
            path: self.file.clone(),
            message,
        };
        let registry: InstallationRegistry =
            serde_json::from_slice(&content).map_err(|error| invalid(error.to_string()))?;
        registry.validate().map_err(invalid)?;
        Ok(Some(registry))
    }

    /// Validates and saves the registry atomically.
    pub fn save(&self, registry: &InstallationRegistry) -> Result<(), RegistryStoreError> {
        registry
            .validate()
            .map_err(|message| RegistryStoreError::Invalid {
                path: self.file.clone(),
                message,
            })?;
        let mut content =
            serde_json::to_vec_pretty(registry).map_err(|error| RegistryStoreError::Invalid {
                path: self.file.clone(),
                message: error.to_string(),
            })?;
        content.push(b'\n');
        atomic_file::write(&self.file, &content).map_err(|source| RegistryStoreError::Write {
            path: self.file.clone(),
            source,
        })
    }
}
