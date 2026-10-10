//! `launch.json`: the persisted launch configuration.
//!
//! It lives next to `settings.json`, is written atomically and validated both
//! when loaded and before being saved. It never touches MAME's own files.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::config::LaunchConfig;
use crate::atomic_file;

pub const LAUNCH_FILE_NAME: &str = "launch.json";

#[derive(Debug)]
pub enum LaunchStoreError {
    Read {
        path: PathBuf,
        source: io::Error,
    },
    /// The file exists but is not a valid launch configuration.
    Invalid {
        path: PathBuf,
        message: String,
    },
    /// The configuration to save is not valid.
    Rejected(String),
    Write {
        path: PathBuf,
        source: io::Error,
    },
}

impl fmt::Display for LaunchStoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, source } => {
                write!(f, "failed to read '{}': {source}", path.display())
            }
            Self::Invalid { path, message } => {
                write!(
                    f,
                    "launch configuration '{}' is invalid: {message}",
                    path.display()
                )
            }
            Self::Rejected(message) => write!(f, "invalid launch configuration: {message}"),
            Self::Write { path, source } => {
                write!(f, "failed to write '{}': {source}", path.display())
            }
        }
    }
}

impl std::error::Error for LaunchStoreError {}

#[derive(Debug, Clone)]
pub struct LaunchConfigStore {
    file: PathBuf,
}

impl LaunchConfigStore {
    pub fn in_dir(dir: &Path) -> Self {
        Self {
            file: dir.join(LAUNCH_FILE_NAME),
        }
    }

    /// Loads the configuration; a missing file gives the default configuration.
    pub fn load(&self) -> Result<LaunchConfig, LaunchStoreError> {
        let content = match fs::read(&self.file) {
            Ok(content) => content,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(LaunchConfig::default());
            }
            Err(source) => {
                return Err(LaunchStoreError::Read {
                    path: self.file.clone(),
                    source,
                });
            }
        };
        let invalid = |message: String| LaunchStoreError::Invalid {
            path: self.file.clone(),
            message,
        };
        let config: LaunchConfig =
            serde_json::from_slice(&content).map_err(|error| invalid(error.to_string()))?;
        config.validate().map_err(|error| invalid(error.0))?;
        Ok(config)
    }

    pub fn exists(&self) -> bool {
        self.file.is_file()
    }

    pub fn dir(&self) -> &Path {
        self.file.parent().unwrap_or(&self.file)
    }

    /// Deletes the file and its directory (when empty).
    pub fn delete(&self) -> io::Result<()> {
        match fs::remove_file(&self.file) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        if let Some(dir) = self.file.parent() {
            // Best effort: the directory may contain files YMDB did not create.
            let _ = fs::remove_dir(dir);
        }
        Ok(())
    }

    /// Validates, normalizes and saves the configuration atomically.
    pub fn save(&self, config: LaunchConfig) -> Result<LaunchConfig, LaunchStoreError> {
        let config = config.normalized();
        config
            .validate()
            .map_err(|error| LaunchStoreError::Rejected(error.0))?;
        let mut content = serde_json::to_vec_pretty(&config)
            .map_err(|error| LaunchStoreError::Rejected(error.to_string()))?;
        content.push(b'\n');
        atomic_file::write(&self.file, &content).map_err(|source| LaunchStoreError::Write {
            path: self.file.clone(),
            source,
        })?;
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::launcher::config::Profile;
    use crate::launcher::options::{OptionSet, OptionValue};

    #[test]
    fn missing_file_gives_defaults() {
        let dir = tempfile::tempdir().expect("temp dir");
        let config = LaunchConfigStore::in_dir(dir.path()).load().expect("load");
        assert_eq!(config, LaunchConfig::default());
    }

    #[test]
    fn saves_and_loads_profiles_overrides_and_unicode_paths() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = LaunchConfigStore::in_dir(&dir.path().join("config dir ✓"));
        let mut config = LaunchConfig::default();
        config.global_options.insert(
            "rompath".to_owned(),
            OptionValue::List(vec![
                "roms".to_owned(),
                "D:/Émulateurs MAME ✓/roms".to_owned(),
            ]),
        );
        config.profiles.push(Profile {
            id: "fast".to_owned(),
            name: "Fast forward".to_owned(),
            options: OptionSet::from([("speed".to_owned(), OptionValue::Number(4.0))]),
        });
        config.default_profile_id = "fast".to_owned();
        config.machine_overrides.insert(
            "pacman".to_owned(),
            OptionSet::from([("window".to_owned(), OptionValue::Boolean(true))]),
        );
        config
            .machine_overrides
            .insert("galaga".to_owned(), OptionSet::new());

        let saved = store.save(config).expect("save");
        assert!(!saved.machine_overrides.contains_key("galaga"));
        assert_eq!(store.load().expect("load"), saved);
        let entries: Vec<_> = fs::read_dir(dir.path().join("config dir ✓"))
            .expect("read dir")
            .map(|entry| entry.expect("entry").file_name())
            .collect();
        assert_eq!(entries, vec![LAUNCH_FILE_NAME]);
    }

    #[test]
    fn rejects_invalid_configuration_without_writing() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = LaunchConfigStore::in_dir(dir.path());
        let mut config = LaunchConfig::default();
        config
            .global_options
            .insert("volume".to_owned(), OptionValue::Number(99.0));

        assert!(matches!(
            store.save(config),
            Err(LaunchStoreError::Rejected(_))
        ));
        assert!(!dir.path().join(LAUNCH_FILE_NAME).exists());
    }

    #[test]
    fn reports_corrupted_or_invalid_files() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = LaunchConfigStore::in_dir(dir.path());
        for content in [
            "{ not json",
            r#"{ "globalOptions": { "shell": "rm -rf /" } }"#,
            r#"{ "profiles": [] }"#,
            r#"{ "schemaVersion": 99 }"#,
        ] {
            fs::write(dir.path().join(LAUNCH_FILE_NAME), content).expect("write");
            assert!(
                matches!(store.load(), Err(LaunchStoreError::Invalid { .. })),
                "{content}"
            );
        }
        // Saving a valid configuration replaces the invalid file.
        store.save(LaunchConfig::default()).expect("save");
        assert_eq!(store.load().expect("load"), LaunchConfig::default());
    }
}
