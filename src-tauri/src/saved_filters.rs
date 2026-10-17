//! Saved filter presets (`saved-filters.json`).
//!
//! A preset is a named configuration of the catalog browser's search and
//! filters, global to YMDB: it names no installation and no catalog row, so
//! it applies to whichever catalog is active when it is loaded.
//!
//! The criteria are defined by the browser, which validates them when it
//! loads a preset; this module stores them verbatim with their format version
//! (`criteriaVersion`), so criteria written by a newer YMDB are preserved,
//! never rewritten. The file itself has a schema version: a file of a newer
//! version, or an invalid file, is reported and never replaced.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::atomic_file;

pub const SAVED_FILTERS_FILE_NAME: &str = "saved-filters.json";
pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_PRESETS: usize = 500;
pub const MAX_NAME_LENGTH: usize = 100;
/// Largest serialized criteria of one preset.
pub const MAX_CRITERIA_SIZE: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SavedFilters {
    pub schema_version: u32,
    pub presets: Vec<SavedFilter>,
}

impl Default for SavedFilters {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            presets: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SavedFilter {
    pub id: String,
    pub name: String,
    /// Version of the criteria format, defined by the browser.
    pub criteria_version: u32,
    /// The criteria, a JSON object kept verbatim.
    pub criteria: serde_json::Value,
    /// Milliseconds since the Unix epoch.
    pub created_at: u64,
    pub updated_at: u64,
}

impl SavedFilters {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(format!(
                "schema version {} is not supported (expected {SCHEMA_VERSION})",
                self.schema_version
            ));
        }
        if self.presets.len() > MAX_PRESETS {
            return Err(format!("at most {MAX_PRESETS} saved filters are supported"));
        }
        let mut ids = std::collections::HashSet::new();
        let mut names = std::collections::HashSet::new();
        for preset in &self.presets {
            if preset.id.is_empty()
                || preset.id.len() > 64
                || !preset
                    .id
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            {
                return Err(format!("invalid saved filter id '{}'", preset.id));
            }
            if !ids.insert(preset.id.as_str()) {
                return Err(format!("duplicate saved filter id '{}'", preset.id));
            }
            validate_name(&preset.name)?;
            if !names.insert(preset.name.to_lowercase()) {
                return Err(format!("duplicate saved filter name '{}'", preset.name));
            }
            validate_criteria(&preset.criteria)?;
        }
        Ok(())
    }

    fn get_mut(&mut self, id: &str) -> Result<&mut SavedFilter, String> {
        self.presets
            .iter_mut()
            .find(|preset| preset.id == id)
            .ok_or_else(|| "this saved filter no longer exists".to_owned())
    }

    fn check_unique_name(&self, name: &str, except: Option<&str>) -> Result<(), String> {
        let taken = self.presets.iter().any(|preset| {
            Some(preset.id.as_str()) != except && preset.name.to_lowercase() == name.to_lowercase()
        });
        if taken {
            Err(format!("a saved filter named '{name}' already exists"))
        } else {
            Ok(())
        }
    }

    /// Adds a preset; returns its id.
    pub fn create(
        &mut self,
        name: &str,
        criteria_version: u32,
        criteria: serde_json::Value,
        now: u64,
    ) -> Result<String, String> {
        let name = name.trim();
        validate_name(name)?;
        validate_criteria(&criteria)?;
        self.check_unique_name(name, None)?;
        if self.presets.len() >= MAX_PRESETS {
            return Err(format!("at most {MAX_PRESETS} saved filters are supported"));
        }
        let id = uuid::Uuid::new_v4().simple().to_string();
        self.presets.push(SavedFilter {
            id: id.clone(),
            name: name.to_owned(),
            criteria_version,
            criteria,
            created_at: now,
            updated_at: now,
        });
        Ok(id)
    }

    /// Replaces the criteria of a preset, keeping its name.
    pub fn update(
        &mut self,
        id: &str,
        criteria_version: u32,
        criteria: serde_json::Value,
        now: u64,
    ) -> Result<(), String> {
        validate_criteria(&criteria)?;
        let preset = self.get_mut(id)?;
        preset.criteria_version = criteria_version;
        preset.criteria = criteria;
        preset.updated_at = now;
        Ok(())
    }

    pub fn rename(&mut self, id: &str, name: &str, now: u64) -> Result<(), String> {
        let name = name.trim();
        validate_name(name)?;
        self.check_unique_name(name, Some(id))?;
        let preset = self.get_mut(id)?;
        preset.name = name.to_owned();
        preset.updated_at = now;
        Ok(())
    }

    pub fn delete(&mut self, id: &str) -> Result<(), String> {
        let before = self.presets.len();
        self.presets.retain(|preset| preset.id != id);
        if self.presets.len() == before {
            return Err("this saved filter no longer exists".to_owned());
        }
        Ok(())
    }
}

fn validate_name(name: &str) -> Result<(), String> {
    let length = name.chars().count();
    if name.trim() != name || length == 0 || length > MAX_NAME_LENGTH {
        return Err(format!(
            "a saved filter name must have 1 to {MAX_NAME_LENGTH} characters"
        ));
    }
    if name.chars().any(char::is_control) {
        return Err("a saved filter name cannot contain control characters".to_owned());
    }
    Ok(())
}

fn validate_criteria(criteria: &serde_json::Value) -> Result<(), String> {
    if !criteria.is_object() {
        return Err("saved filter criteria must be an object".to_owned());
    }
    if criteria.to_string().len() > MAX_CRITERIA_SIZE {
        return Err(format!(
            "saved filter criteria must be at most {MAX_CRITERIA_SIZE} bytes"
        ));
    }
    Ok(())
}

#[derive(Debug)]
pub enum SavedFiltersStoreError {
    Read { path: PathBuf, source: io::Error },
    Invalid { path: PathBuf, message: String },
    Write { path: PathBuf, source: io::Error },
}

impl fmt::Display for SavedFiltersStoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, source } => {
                write!(f, "failed to read '{}': {source}", path.display())
            }
            Self::Invalid { path, message } => write!(
                f,
                "the saved filters file '{}' is invalid: {message}",
                path.display()
            ),
            Self::Write { path, source } => {
                write!(f, "failed to write '{}': {source}", path.display())
            }
        }
    }
}

impl std::error::Error for SavedFiltersStoreError {}

#[derive(Debug, Clone)]
pub struct SavedFiltersStore {
    file: PathBuf,
}

impl SavedFiltersStore {
    pub fn in_dir(dir: &Path) -> Self {
        Self {
            file: dir.join(SAVED_FILTERS_FILE_NAME),
        }
    }

    /// Loads the presets; a missing file means none (nothing is written).
    pub fn load(&self) -> Result<SavedFilters, SavedFiltersStoreError> {
        let content = match fs::read(&self.file) {
            Ok(content) => content,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(SavedFilters::default());
            }
            Err(source) => {
                return Err(SavedFiltersStoreError::Read {
                    path: self.file.clone(),
                    source,
                });
            }
        };
        let invalid = |message: String| SavedFiltersStoreError::Invalid {
            path: self.file.clone(),
            message,
        };
        let filters: SavedFilters =
            serde_json::from_slice(&content).map_err(|error| invalid(error.to_string()))?;
        filters.validate().map_err(invalid)?;
        Ok(filters)
    }

    pub fn save(&self, filters: &SavedFilters) -> Result<(), SavedFiltersStoreError> {
        let invalid = |message: String| SavedFiltersStoreError::Invalid {
            path: self.file.clone(),
            message,
        };
        filters.validate().map_err(invalid)?;
        let mut content =
            serde_json::to_vec_pretty(filters).map_err(|error| invalid(error.to_string()))?;
        content.push(b'\n');
        atomic_file::write(&self.file, &content).map_err(|source| SavedFiltersStoreError::Write {
            path: self.file.clone(),
            source,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn criteria() -> serde_json::Value {
        json!({ "scope": "machines", "text": "pac", "values": { "series": ["Pac-Man"] } })
    }

    #[test]
    fn creates_renames_updates_and_deletes_presets() {
        let mut filters = SavedFilters::default();
        let id = filters
            .create(" Pac-Man games ", 1, criteria(), 10)
            .expect("create");
        assert_eq!(filters.presets[0].name, "Pac-Man games");
        assert_eq!(filters.presets[0].created_at, 10);

        filters.rename(&id, "Mazes", 20).expect("rename");
        filters
            .update(&id, 1, json!({ "scope": "software" }), 30)
            .expect("update");
        let preset = &filters.presets[0];
        assert_eq!(preset.name, "Mazes");
        assert_eq!(preset.criteria, json!({ "scope": "software" }));
        assert_eq!((preset.created_at, preset.updated_at), (10, 30));

        filters.delete(&id).expect("delete");
        assert!(filters.presets.is_empty());
        assert!(filters.delete(&id).is_err());
        assert!(filters.rename(&id, "Gone", 40).is_err());
    }

    #[test]
    fn rejects_invalid_names_and_criteria() {
        let mut filters = SavedFilters::default();
        filters
            .create("Shooters", 1, criteria(), 1)
            .expect("create");
        assert!(filters.create("shooters", 1, criteria(), 1).is_err());
        assert!(filters.create("   ", 1, criteria(), 1).is_err());
        assert!(filters.create(&"x".repeat(101), 1, criteria(), 1).is_err());
        assert!(filters.create("Tab\tname", 1, criteria(), 1).is_err());
        assert!(
            filters
                .create("List", 1, json!(["not", "an object"]), 1)
                .is_err()
        );
        let large = json!({ "text": "x".repeat(MAX_CRITERIA_SIZE) });
        assert!(filters.create("Large", 1, large, 1).is_err());
        let other = filters.create("Other", 1, criteria(), 1).expect("create");
        assert!(filters.rename(&other, "SHOOTERS", 2).is_err());
        // Renaming to its own name with another case is allowed.
        filters.rename(&other, "OTHER", 2).expect("rename");
    }

    #[test]
    fn persists_presets_and_keeps_unknown_criteria_versions() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = SavedFiltersStore::in_dir(dir.path());
        assert_eq!(store.load().expect("missing file"), SavedFilters::default());
        assert!(!dir.path().join(SAVED_FILTERS_FILE_NAME).exists());

        let mut filters = SavedFilters::default();
        // Criteria from a newer browser are stored verbatim.
        let future = json!({ "scope": "machines", "newFilter": { "deep": [1, 2] } });
        filters
            .create("Future", 7, future.clone(), 1)
            .expect("create");
        store.save(&filters).expect("save");
        let loaded = store.load().expect("load");
        assert_eq!(loaded, filters);
        assert_eq!(loaded.presets[0].criteria_version, 7);
        assert_eq!(loaded.presets[0].criteria, future);
    }

    #[test]
    fn reports_invalid_files_and_never_replaces_them() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = SavedFiltersStore::in_dir(dir.path());
        let path = dir.path().join(SAVED_FILTERS_FILE_NAME);
        for content in [
            "{ not json",
            r#"{ "schemaVersion": 2, "presets": [] }"#,
            r#"{ "schemaVersion": 1, "presets": [], "extra": true }"#,
        ] {
            fs::write(&path, content).expect("write");
            assert!(matches!(
                store.load(),
                Err(SavedFiltersStoreError::Invalid { .. })
            ));
            assert_eq!(fs::read_to_string(&path).expect("read"), content);
        }
    }
}
