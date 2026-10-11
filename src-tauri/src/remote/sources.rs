//! Remote sources of one MAME installation (`installations/<id>/remote-sources.json`).
//!
//! For each category of files (machine ROMs and CHDs, software list ROMs and
//! CHDs, samples) an installation may name a remote connection (by id, never
//! by copying its settings), the base path of the collection in that
//! repository, how the collection is organized and the local directory that
//! will receive downloaded files. Categories are independent and optional.
//!
//! The remote side and the local side are separate: several categories may
//! use different repositories and share one local directory, or the
//! opposite. How remote file names map to local ones is decided by the ROM
//! manager, not here.

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::connections::ConnectionsConfig;
use super::path::RemotePath;
use crate::atomic_file;
use crate::launcher::options::{self, OptionValue};

pub const SCHEMA_VERSION: u32 = 1;
pub const SOURCES_FILE_NAME: &str = "remote-sources.json";

/// A category of files that MAME looks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SourceCategory {
    MachineRoms,
    MachineChds,
    SoftwareListRoms,
    SoftwareListChds,
    Samples,
}

pub const CATEGORIES: [SourceCategory; 5] = [
    SourceCategory::MachineRoms,
    SourceCategory::MachineChds,
    SourceCategory::SoftwareListRoms,
    SourceCategory::SoftwareListChds,
    SourceCategory::Samples,
];

/// The MAME search path option that finds the files of a category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SearchPath {
    /// `rompath`: ROMs and CHDs of machines and software lists.
    Rom,
    /// `samplepath`.
    Sample,
}

impl SearchPath {
    pub fn option_name(self) -> &'static str {
        match self {
            Self::Rom => "rompath",
            Self::Sample => "samplepath",
        }
    }
}

impl SourceCategory {
    pub fn label(self) -> &'static str {
        match self {
            Self::MachineRoms => "Machine ROMs",
            Self::MachineChds => "Machine CHDs",
            Self::SoftwareListRoms => "Software List ROMs",
            Self::SoftwareListChds => "Software List CHDs",
            Self::Samples => "Samples",
        }
    }

    pub fn search_path(self) -> SearchPath {
        match self {
            Self::Samples => SearchPath::Sample,
            _ => SearchPath::Rom,
        }
    }

    /// The organizations a collection of this category can have.
    pub fn organizations(self) -> &'static [CollectionOrganization] {
        use CollectionOrganization::{Directories, Merged, NonMerged, Split, ZipArchives};
        match self {
            Self::Samples => &[ZipArchives, Directories],
            _ => &[Merged, Split, NonMerged],
        }
    }
}

/// How a remote collection is organized.
///
/// ROM collections use the usual ZIP set types. CHD collections use the same
/// names for where the disks of clones live (with the parent, in each clone's
/// directory, or both); the exact path rules for CHDs are defined by the ROM
/// manager and are not those of ZIP archives. Samples are either one ZIP per
/// sample set or one directory per set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CollectionOrganization {
    Merged,
    Split,
    NonMerged,
    ZipArchives,
    Directories,
}

/// Where downloaded files of a category go.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "mode",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum LocalDestination {
    /// One of the directories MAME already searches (`rompath` or
    /// `samplepath`), as written in its configuration (it may be relative to
    /// the MAME directory).
    MameDirectory { directory: String },
    /// A directory chosen by the user; YMDB adds it to the search path when
    /// launching MAME.
    Custom { directory: PathBuf },
}

/// The source of one category. A disabled category may be incomplete.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CategorySource {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub connection_id: Option<String>,
    #[serde(default)]
    pub remote_base_path: Option<String>,
    #[serde(default)]
    pub organization: Option<CollectionOrganization>,
    #[serde(default)]
    pub destination: Option<LocalDestination>,
}

/// Content of `remote-sources.json`. Categories that are not configured are absent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RemoteSourcesConfig {
    #[serde(default = "schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub categories: BTreeMap<SourceCategory, CategorySource>,
}

fn schema_version() -> u32 {
    SCHEMA_VERSION
}

impl Default for RemoteSourcesConfig {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            categories: BTreeMap::new(),
        }
    }
}

/// A source referencing a connection: used to refuse removing it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionReference {
    pub installation_id: String,
    pub installation_label: String,
    pub category: SourceCategory,
}

impl RemoteSourcesConfig {
    /// Checks the structure. Enabled categories must be complete; values that
    /// are present are checked even in disabled categories.
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(format!(
                "unsupported remote sources file version {}",
                self.schema_version
            ));
        }
        for (category, source) in &self.categories {
            check_source(*category, source)
                .map_err(|message| format!("{}: {message}", category.label()))?;
        }
        Ok(())
    }

    /// Checks that every referenced connection exists.
    pub fn check_references(&self, connections: &ConnectionsConfig) -> Result<(), String> {
        for (category, source) in &self.categories {
            if let Some(id) = &source.connection_id
                && connections.get(id).is_none()
            {
                return Err(format!(
                    "{}: the remote connection no longer exists; choose another one",
                    category.label()
                ));
            }
        }
        Ok(())
    }

    /// Categories referencing a connection.
    pub fn categories_using(&self, connection_id: &str) -> Vec<SourceCategory> {
        self.categories
            .iter()
            .filter(|(_, source)| source.connection_id.as_deref() == Some(connection_id))
            .map(|(category, _)| *category)
            .collect()
    }

    /// Removes categories without any value, so the file only keeps what is set.
    pub fn normalized(mut self) -> Self {
        self.categories
            .retain(|_, source| *source != CategorySource::default());
        for source in self.categories.values_mut() {
            if let Some(path) = &source.remote_base_path {
                // Validated before saving; keep the normalized form.
                source.remote_base_path = Some(
                    RemotePath::parse(path).map_or_else(|_| path.clone(), |path| path.to_string()),
                );
            }
            if let Some(LocalDestination::MameDirectory { directory }) = &mut source.destination {
                *directory = directory.trim().to_owned();
            }
        }
        self
    }

    /// Local destinations of enabled categories, per search path, without
    /// repetitions, in category order.
    pub fn destinations(&self, search_path: SearchPath) -> Vec<String> {
        let mut directories: Vec<String> = Vec::new();
        for (category, source) in &self.categories {
            if !source.enabled || category.search_path() != search_path {
                continue;
            }
            let directory = match &source.destination {
                Some(LocalDestination::MameDirectory { directory }) => directory.clone(),
                Some(LocalDestination::Custom { directory }) => {
                    directory.to_string_lossy().into_owned()
                }
                None => continue,
            };
            if !directories.contains(&directory) {
                directories.push(directory);
            }
        }
        directories
    }
}

fn check_source(category: SourceCategory, source: &CategorySource) -> Result<(), String> {
    if let Some(id) = &source.connection_id
        && !super::connections::is_connection_id(id)
    {
        return Err(format!("invalid connection id '{id}'"));
    }
    if let Some(path) = &source.remote_base_path {
        RemotePath::parse(path).map_err(|error| format!("remote base path: {error}"))?;
    }
    if let Some(organization) = source.organization
        && !category.organizations().contains(&organization)
    {
        return Err(format!(
            "the organization '{}' does not apply to this category",
            serde_json::to_value(organization)
                .ok()
                .and_then(|value| value.as_str().map(str::to_owned))
                .unwrap_or_default()
        ));
    }
    if let Some(destination) = &source.destination {
        check_destination(category, destination)?;
    }
    if source.enabled {
        let missing = [
            (source.connection_id.is_none(), "a remote connection"),
            (source.remote_base_path.is_none(), "a remote base path"),
            (source.organization.is_none(), "the collection organization"),
            (source.destination.is_none(), "a local destination"),
        ]
        .into_iter()
        .filter(|(missing, _)| *missing)
        .map(|(_, field)| field)
        .collect::<Vec<_>>();
        if !missing.is_empty() {
            return Err(format!("an enabled category needs {}", missing.join(", ")));
        }
    }
    Ok(())
}

/// A destination must be usable as a MAME search path entry.
fn check_destination(
    category: SourceCategory,
    destination: &LocalDestination,
) -> Result<(), String> {
    let directory = match destination {
        LocalDestination::MameDirectory { directory } => {
            if directory.trim().is_empty() {
                return Err("choose one of the MAME directories".to_owned());
            }
            directory.trim().to_owned()
        }
        LocalDestination::Custom { directory } => {
            if !directory.is_absolute() {
                return Err("the custom local directory must be an absolute path".to_owned());
            }
            directory.to_string_lossy().into_owned()
        }
    };
    let name = category.search_path().option_name();
    options::validate(name, &OptionValue::List(vec![directory]))
        .map_err(|error| format!("local destination: {error}"))
}

#[derive(Debug)]
pub enum SourcesStoreError {
    Read { path: PathBuf, source: io::Error },
    Invalid { path: PathBuf, message: String },
    Rejected(String),
    Write { path: PathBuf, source: io::Error },
}

impl fmt::Display for SourcesStoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, source } => {
                write!(f, "failed to read '{}': {source}", path.display())
            }
            Self::Invalid { path, message } => write!(
                f,
                "the remote sources file '{}' is invalid: {message}",
                path.display()
            ),
            Self::Rejected(message) => write!(f, "invalid remote sources: {message}"),
            Self::Write { path, source } => {
                write!(f, "failed to write '{}': {source}", path.display())
            }
        }
    }
}

impl std::error::Error for SourcesStoreError {}

/// `remote-sources.json` of one installation.
#[derive(Debug, Clone)]
pub struct RemoteSourcesStore {
    file: PathBuf,
}

impl RemoteSourcesStore {
    pub fn in_dir(dir: &Path) -> Self {
        Self {
            file: dir.join(SOURCES_FILE_NAME),
        }
    }

    pub fn exists(&self) -> bool {
        self.file.is_file()
    }

    /// Loads the sources; a missing file means none.
    pub fn load(&self) -> Result<RemoteSourcesConfig, SourcesStoreError> {
        let content = match fs::read(&self.file) {
            Ok(content) => content,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(RemoteSourcesConfig::default());
            }
            Err(source) => {
                return Err(SourcesStoreError::Read {
                    path: self.file.clone(),
                    source,
                });
            }
        };
        let invalid = |message: String| SourcesStoreError::Invalid {
            path: self.file.clone(),
            message,
        };
        let config: RemoteSourcesConfig =
            serde_json::from_slice(&content).map_err(|error| invalid(error.to_string()))?;
        config.validate().map_err(invalid)?;
        Ok(config)
    }

    /// Validates, normalizes and saves the sources atomically.
    pub fn save(
        &self,
        config: RemoteSourcesConfig,
    ) -> Result<RemoteSourcesConfig, SourcesStoreError> {
        config.validate().map_err(SourcesStoreError::Rejected)?;
        let config = config.normalized();
        let mut content = serde_json::to_vec_pretty(&config)
            .map_err(|error| SourcesStoreError::Rejected(error.to_string()))?;
        content.push(b'\n');
        atomic_file::write(&self.file, &content).map_err(|source| SourcesStoreError::Write {
            path: self.file.clone(),
            source,
        })?;
        Ok(config)
    }

    /// Deletes the file (the directory is left to its other files).
    pub fn delete(&self) -> io::Result<()> {
        match fs::remove_file(&self.file) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            other => other,
        }
    }
}

#[cfg(test)]
mod tests;
