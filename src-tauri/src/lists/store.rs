//! `lists.json`: the persisted lists.
//!
//! It lives in the configuration directory, is written atomically and
//! validated when loaded. A missing file means empty system lists; an invalid
//! or unreadable file is reported and never replaced implicitly, since it
//! holds data entered by the user.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::model::Lists;
use crate::atomic_file;

pub const LISTS_FILE_NAME: &str = "lists.json";

#[derive(Debug)]
pub enum ListsStoreError {
    Read { path: PathBuf, source: io::Error },
    Invalid { path: PathBuf, message: String },
    Write { path: PathBuf, source: io::Error },
}

impl fmt::Display for ListsStoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, source } => {
                write!(f, "failed to read '{}': {source}", path.display())
            }
            Self::Invalid { path, message } => write!(
                f,
                "the lists file '{}' is invalid: {message}",
                path.display()
            ),
            Self::Write { path, source } => {
                write!(f, "failed to write '{}': {source}", path.display())
            }
        }
    }
}

impl std::error::Error for ListsStoreError {}

#[derive(Debug, Clone)]
pub struct ListsStore {
    file: PathBuf,
}

impl ListsStore {
    pub fn in_dir(dir: &Path) -> Self {
        Self {
            file: dir.join(LISTS_FILE_NAME),
        }
    }

    /// Loads the lists; the system lists are added when missing (nothing is
    /// written until the lists change).
    pub fn load(&self) -> Result<Lists, ListsStoreError> {
        let content = match fs::read(&self.file) {
            Ok(content) => content,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Lists::new()),
            Err(source) => {
                return Err(ListsStoreError::Read {
                    path: self.file.clone(),
                    source,
                });
            }
        };
        let invalid = |message: String| ListsStoreError::Invalid {
            path: self.file.clone(),
            message,
        };
        let mut lists: Lists =
            serde_json::from_slice(&content).map_err(|error| invalid(error.to_string()))?;
        lists.validate().map_err(invalid)?;
        lists.ensure_system_lists();
        Ok(lists)
    }

    /// Validates and saves the lists atomically.
    pub fn save(&self, lists: &Lists) -> Result<(), ListsStoreError> {
        let invalid = |message: String| ListsStoreError::Invalid {
            path: self.file.clone(),
            message,
        };
        lists.validate().map_err(invalid)?;
        let mut content =
            serde_json::to_vec_pretty(lists).map_err(|error| invalid(error.to_string()))?;
        content.push(b'\n');
        atomic_file::write(&self.file, &content).map_err(|source| ListsStoreError::Write {
            path: self.file.clone(),
            source,
        })
    }
}
