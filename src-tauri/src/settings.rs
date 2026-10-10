//! `settings.json` of the single-installation layout (format version 1).
//!
//! Older YMDB releases stored the one configured MAME executable here. The
//! file is now only read by the migration to the installation registry
//! (`installations::migration`) and is never written, so it remains a backup
//! of the previous configuration.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Deserialize;

pub const SETTINGS_FILE_NAME: &str = "settings.json";

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    #[serde(default)]
    pub mame: Option<MameSettings>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MameSettings {
    /// Absolute path of the MAME executable selected by the user.
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
        }
    }
}

impl std::error::Error for SettingsError {}

/// Reads the settings file in a given directory.
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_documented_format() {
        let dir = tempfile::tempdir().expect("temp dir");
        fs::write(
            dir.path().join(SETTINGS_FILE_NAME),
            r#"{ "schemaVersion": 1, "mame": { "executablePath": "/home/user/Émulateurs MAME ✓/mame" } }"#,
        )
        .expect("write");

        let settings = SettingsStore::in_dir(dir.path()).load().expect("load");

        assert_eq!(
            settings.mame.map(|mame| mame.executable_path),
            Some(PathBuf::from("/home/user/Émulateurs MAME ✓/mame"))
        );
    }

    #[test]
    fn missing_file_or_fields_yield_no_executable() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = SettingsStore::in_dir(dir.path());
        assert_eq!(store.load().expect("load"), Settings::default());

        fs::write(dir.path().join(SETTINGS_FILE_NAME), "{}").expect("write");
        assert_eq!(store.load().expect("load"), Settings::default());
    }

    #[test]
    fn invalid_content_is_reported_as_invalid() {
        let dir = tempfile::tempdir().expect("temp dir");
        for content in ["{ not json", r#"{ "mame": { "executablePath": 42 } }"#] {
            fs::write(dir.path().join(SETTINGS_FILE_NAME), content).expect("write");

            let result = SettingsStore::in_dir(dir.path()).load();

            assert!(
                matches!(result, Err(SettingsError::Invalid { .. })),
                "{result:?}"
            );
        }
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
}
