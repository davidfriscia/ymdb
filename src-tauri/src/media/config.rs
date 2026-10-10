//! Media categories and their source directories (`media.json`).

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::atomic_file;

pub const SCHEMA_VERSION: u32 = 1;
pub const MEDIA_FILE_NAME: &str = "media.json";
const MAX_SOURCES_PER_CATEGORY: usize = 16;

/// A kind of image. Categories follow the folder names of the common MAME
/// artwork collections.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaCategory {
    pub id: &'static str,
    pub label: &'static str,
}

/// Supported categories. Add a category here to make it configurable.
pub const CATEGORIES: &[MediaCategory] = &[
    MediaCategory {
        id: "snap",
        label: "Snapshots",
    },
    MediaCategory {
        id: "titles",
        label: "Title screens",
    },
    MediaCategory {
        id: "flyers",
        label: "Flyers",
    },
    MediaCategory {
        id: "cabinets",
        label: "Cabinets",
    },
    MediaCategory {
        id: "marquees",
        label: "Marquees",
    },
];

pub fn is_category(id: &str) -> bool {
    CATEGORIES.iter().any(|category| category.id == id)
}

/// Source directories per category, in order of precedence.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MediaConfig {
    #[serde(default = "schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub sources: BTreeMap<String, Vec<PathBuf>>,
}

fn schema_version() -> u32 {
    SCHEMA_VERSION
}

impl MediaConfig {
    pub fn new() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            sources: BTreeMap::new(),
        }
    }

    /// Checks the structure; whether directories exist is a status, not an error.
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(format!(
                "unsupported media configuration version {}",
                self.schema_version
            ));
        }
        for (category, paths) in &self.sources {
            if !is_category(category) {
                return Err(format!("unknown media category '{category}'"));
            }
            if paths.len() > MAX_SOURCES_PER_CATEGORY {
                return Err(format!(
                    "at most {MAX_SOURCES_PER_CATEGORY} directories per category"
                ));
            }
            for (index, path) in paths.iter().enumerate() {
                if !path.is_absolute() {
                    return Err(format!(
                        "'{}' is not an absolute directory path",
                        path.display()
                    ));
                }
                if path.to_string_lossy().chars().any(char::is_control) {
                    return Err("directory paths must not contain control characters".to_owned());
                }
                if paths[..index].contains(path) {
                    return Err(format!(
                        "'{}' is listed twice in {category}",
                        path.display()
                    ));
                }
            }
        }
        Ok(())
    }

    /// Drops categories without directories.
    pub fn normalized(mut self) -> Self {
        self.sources.retain(|_, paths| !paths.is_empty());
        self
    }

    pub fn sources(&self, category: &str) -> &[PathBuf] {
        self.sources.get(category).map_or(&[], Vec::as_slice)
    }
}

/// Whether a configured directory can be used right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SourceStatus {
    Available,
    Missing,
    NotADirectory,
    Unreadable,
}

pub fn source_status(path: &Path) -> SourceStatus {
    match fs::metadata(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => SourceStatus::Missing,
        Err(_) => SourceStatus::Unreadable,
        Ok(metadata) if !metadata.is_dir() => SourceStatus::NotADirectory,
        Ok(_) => match fs::read_dir(path) {
            Ok(_) => SourceStatus::Available,
            Err(_) => SourceStatus::Unreadable,
        },
    }
}

#[derive(Debug)]
pub enum MediaStoreError {
    Read { path: PathBuf, source: io::Error },
    Invalid { path: PathBuf, message: String },
    Rejected(String),
    Write { path: PathBuf, source: io::Error },
}

impl fmt::Display for MediaStoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, source } => {
                write!(f, "failed to read '{}': {source}", path.display())
            }
            Self::Invalid { path, message } => {
                write!(
                    f,
                    "media configuration '{}' is invalid: {message}",
                    path.display()
                )
            }
            Self::Rejected(message) => write!(f, "invalid media configuration: {message}"),
            Self::Write { path, source } => {
                write!(f, "failed to write '{}': {source}", path.display())
            }
        }
    }
}

impl std::error::Error for MediaStoreError {}

/// Reads and writes `media.json` in the application configuration directory.
#[derive(Debug, Clone)]
pub struct MediaConfigStore {
    file: PathBuf,
}

impl MediaConfigStore {
    pub fn in_dir(dir: &Path) -> Self {
        Self {
            file: dir.join(MEDIA_FILE_NAME),
        }
    }

    /// A missing file means no sources.
    pub fn load(&self) -> Result<MediaConfig, MediaStoreError> {
        let content = match fs::read(&self.file) {
            Ok(content) => content,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(MediaConfig::new()),
            Err(source) => {
                return Err(MediaStoreError::Read {
                    path: self.file.clone(),
                    source,
                });
            }
        };
        let invalid = |message: String| MediaStoreError::Invalid {
            path: self.file.clone(),
            message,
        };
        let config: MediaConfig =
            serde_json::from_slice(&content).map_err(|error| invalid(error.to_string()))?;
        config.validate().map_err(invalid)?;
        Ok(config)
    }

    pub fn save(&self, config: MediaConfig) -> Result<MediaConfig, MediaStoreError> {
        let config = config.normalized();
        config.validate().map_err(MediaStoreError::Rejected)?;
        let mut content = serde_json::to_vec_pretty(&config)
            .map_err(|error| MediaStoreError::Rejected(error.to_string()))?;
        content.push(b'\n');
        atomic_file::write(&self.file, &content).map_err(|source| MediaStoreError::Write {
            path: self.file.clone(),
            source,
        })?;
        Ok(config)
    }
}
