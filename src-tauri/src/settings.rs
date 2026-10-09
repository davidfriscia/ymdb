//! Persistent application settings stored as a JSON file.
//!
//! The file lives in the platform-specific application configuration
//! directory. Writes are atomic: the new content is written to a temporary file
//! in the same directory, flushed to disk, then renamed over the settings file.

use std::fmt;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const SETTINGS_FILE_NAME: &str = "settings.json";
pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// Version of the settings format, for future migrations.
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub mame: Option<MameSettings>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            mame: None,
        }
    }
}

fn default_schema_version() -> u32 {
    SCHEMA_VERSION
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MameSettings {
    /// Absolute path of the MAME executable selected by the user. The detected
    /// version is not stored: it is read again on every verification.
    pub executable_path: PathBuf,
}

#[derive(Debug)]
pub enum SettingsError {
    Read {
        path: PathBuf,
        source: io::Error,
    },
    /// The file exists but does not contain valid settings.
    Invalid {
        path: PathBuf,
        source: serde_json::Error,
    },
    Write {
        path: PathBuf,
        source: io::Error,
    },
    Serialize(serde_json::Error),
}

impl fmt::Display for SettingsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, source } => {
                write!(
                    f,
                    "failed to read settings file '{}': {source}",
                    path.display()
                )
            }
            Self::Invalid { path, source } => {
                write!(f, "settings file '{}' is invalid: {source}", path.display())
            }
            Self::Write { path, source } => {
                write!(
                    f,
                    "failed to write settings file '{}': {source}",
                    path.display()
                )
            }
            Self::Serialize(source) => write!(f, "failed to serialize settings: {source}"),
        }
    }
}

impl std::error::Error for SettingsError {}

/// Reads and writes the settings file in a given directory.
#[derive(Debug, Clone)]
pub struct SettingsStore {
    file: PathBuf,
}

impl SettingsStore {
    pub fn in_dir(dir: &Path) -> Self {
        Self {
            file: dir.join(SETTINGS_FILE_NAME),
        }
    }

    /// Loads the settings. A missing file yields the default settings.
    pub fn load(&self) -> Result<Settings, SettingsError> {
        let content = match fs::read(&self.file) {
            Ok(content) => content,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(Settings::default());
            }
            Err(source) => {
                return Err(SettingsError::Read {
                    path: self.file.clone(),
                    source,
                });
            }
        };
        serde_json::from_slice(&content).map_err(|source| SettingsError::Invalid {
            path: self.file.clone(),
            source,
        })
    }

    /// Writes the settings atomically, creating the directory if needed.
    pub fn save(&self, settings: &Settings) -> Result<(), SettingsError> {
        let mut content = serde_json::to_vec_pretty(settings).map_err(SettingsError::Serialize)?;
        content.push(b'\n');

        let write_error = |source| SettingsError::Write {
            path: self.file.clone(),
            source,
        };
        if let Some(dir) = self.file.parent() {
            fs::create_dir_all(dir).map_err(write_error)?;
        }
        let temp_file = self.file.with_extension("json.tmp");
        let result =
            write_synced(&temp_file, &content).and_then(|()| fs::rename(&temp_file, &self.file));
        if result.is_err() {
            // Best effort: never leave a partial temporary file behind.
            let _ = fs::remove_file(&temp_file);
        }
        result.map_err(write_error)
    }

    /// Loads the settings, applies `change` and saves the result.
    ///
    /// An invalid settings file is replaced with defaults plus the change,
    /// so the user can always recover by saving a new configuration.
    pub fn update(&self, change: impl FnOnce(&mut Settings)) -> Result<Settings, SettingsError> {
        let mut settings = match self.load() {
            Ok(settings) => settings,
            Err(SettingsError::Invalid { .. }) => Settings::default(),
            Err(error) => return Err(error),
        };
        change(&mut settings);
        self.save(&settings)?;
        Ok(settings)
    }
}

fn write_synced(path: &Path, content: &[u8]) -> io::Result<()> {
    let mut file = File::create(path)?;
    file.write_all(content)?;
    file.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mame_settings(path: &str) -> Settings {
        Settings {
            schema_version: SCHEMA_VERSION,
            mame: Some(MameSettings {
                executable_path: PathBuf::from(path),
            }),
        }
    }

    #[test]
    fn serializes_to_documented_format() {
        let json = serde_json::to_value(mame_settings("/opt/mame/mame")).expect("serialize");

        assert_eq!(
            json,
            serde_json::json!({
                "schemaVersion": 1,
                "mame": { "executablePath": "/opt/mame/mame" }
            })
        );
    }

    #[test]
    fn deserializes_with_defaults_for_missing_fields() {
        let settings: Settings = serde_json::from_str("{}").expect("deserialize");

        assert_eq!(settings, Settings::default());
    }

    #[test]
    fn missing_file_yields_default_settings() {
        let dir = tempfile::tempdir().expect("temp dir");

        let settings = SettingsStore::in_dir(dir.path()).load().expect("load");

        assert_eq!(settings, Settings::default());
    }

    #[test]
    fn saves_and_loads_paths_with_spaces_and_unicode() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = SettingsStore::in_dir(&dir.path().join("nested config"));
        let settings = mame_settings("/home/user/Émulateurs MAME ✓/mame");

        store.save(&settings).expect("save");

        assert_eq!(store.load().expect("load"), settings);
    }

    #[test]
    fn save_replaces_existing_file_without_leaving_temporary_files() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = SettingsStore::in_dir(dir.path());

        store
            .save(&mame_settings("/first/mame"))
            .expect("first save");
        store
            .save(&mame_settings("/second/mame"))
            .expect("second save");

        assert_eq!(store.load().expect("load"), mame_settings("/second/mame"));
        let entries: Vec<_> = fs::read_dir(dir.path())
            .expect("read dir")
            .map(|entry| entry.expect("entry").file_name())
            .collect();
        assert_eq!(entries, vec![SETTINGS_FILE_NAME]);
    }

    #[test]
    fn invalid_json_is_reported_as_invalid() {
        let dir = tempfile::tempdir().expect("temp dir");
        fs::write(dir.path().join(SETTINGS_FILE_NAME), "{ not json").expect("write");

        let result = SettingsStore::in_dir(dir.path()).load();

        assert!(
            matches!(result, Err(SettingsError::Invalid { .. })),
            "{result:?}"
        );
    }

    #[test]
    fn wrong_field_type_is_reported_as_invalid() {
        let dir = tempfile::tempdir().expect("temp dir");
        fs::write(
            dir.path().join(SETTINGS_FILE_NAME),
            r#"{ "mame": { "executablePath": 42 } }"#,
        )
        .expect("write");

        let result = SettingsStore::in_dir(dir.path()).load();

        assert!(
            matches!(result, Err(SettingsError::Invalid { .. })),
            "{result:?}"
        );
    }

    #[test]
    fn unreadable_settings_path_is_reported_as_read_error() {
        let dir = tempfile::tempdir().expect("temp dir");
        // A directory where the settings file should be cannot be read as a file.
        fs::create_dir(dir.path().join(SETTINGS_FILE_NAME)).expect("create dir");

        let result = SettingsStore::in_dir(dir.path()).load();

        assert!(
            matches!(result, Err(SettingsError::Read { .. })),
            "{result:?}"
        );
    }

    #[test]
    fn update_replaces_invalid_file() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = SettingsStore::in_dir(dir.path());
        fs::write(dir.path().join(SETTINGS_FILE_NAME), "corrupted").expect("write");

        let updated = store
            .update(|settings| settings.mame = mame_settings("/opt/mame").mame)
            .expect("update");

        assert_eq!(updated, mame_settings("/opt/mame"));
        assert_eq!(store.load().expect("load"), updated);
    }

    #[test]
    fn update_can_remove_mame_configuration() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = SettingsStore::in_dir(dir.path());
        store.save(&mame_settings("/opt/mame")).expect("save");

        store
            .update(|settings| settings.mame = None)
            .expect("update");

        assert_eq!(store.load().expect("load"), Settings::default());
    }

    #[test]
    fn write_failure_is_reported_as_write_error() {
        let dir = tempfile::tempdir().expect("temp dir");
        // The settings directory path is occupied by a regular file.
        let blocked = dir.path().join("blocked");
        fs::write(&blocked, "").expect("write");

        let result = SettingsStore::in_dir(&blocked).save(&Settings::default());

        assert!(
            matches!(result, Err(SettingsError::Write { .. })),
            "{result:?}"
        );
    }
}
