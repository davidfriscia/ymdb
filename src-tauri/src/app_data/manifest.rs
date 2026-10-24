//! `manifest.json`: the description of a backup archive.
//!
//! The manifest names every file of the archive with its logical path,
//! category, size and SHA-256 checksum. It contains no credentials: YMDB
//! stores none (remote connections keep the path of a private key and the
//! approved public host keys, never a key or a passphrase).
//!
//! Compatibility rules:
//! - `formatVersion` newer than [`FORMAT_VERSION`]: refused.
//! - Backups made by a newer YMDB version: refused, since this version may
//!   not read their data correctly and would silently downgrade it.
//! - Backups made by the same or an older version: accepted. Their files go
//!   through the usual upgrades when YMDB reads them (JSON fields with
//!   defaults, additive catalog and repository migrations); each file is
//!   also checked by the store that reads it before the restore is offered.

use serde::{Deserialize, Serialize};

use super::BackupError;
use super::scope::{Category, ItemKind};

pub const FORMAT: &str = "ymdb-backup";
pub const FORMAT_VERSION: u32 = 1;
/// The single top-level directory of an archive.
pub const ARCHIVE_ROOT: &str = "ymdb-backup";
pub const MANIFEST_NAME: &str = "manifest.json";
/// Suggested file name suffix.
pub const FILE_SUFFIX: &str = ".ymdb-backup.zip";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub format: String,
    pub format_version: u32,
    /// Version of the YMDB that created the backup.
    pub app_version: String,
    /// Milliseconds since the Unix epoch.
    pub created_at: u64,
    pub platform: Platform,
    /// Schema versions of the data formats when the backup was created.
    pub schemas: Schemas,
    pub items: Vec<ManifestItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Platform {
    /// `windows`, `macos`, `linux`...
    pub os: String,
    pub arch: String,
}

impl Platform {
    pub fn current() -> Self {
        Self {
            os: std::env::consts::OS.to_owned(),
            arch: std::env::consts::ARCH.to_owned(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Schemas {
    pub installations: u32,
    pub catalog: i64,
    pub support_files: i64,
}

impl Schemas {
    pub fn current() -> Self {
        Self {
            installations: crate::installations::registry::SCHEMA_VERSION,
            catalog: crate::catalog::schema::SCHEMA_VERSION,
            support_files: crate::support_files::repository::SCHEMA_VERSION,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestItem {
    /// Logical path, relative to [`ARCHIVE_ROOT`] (see `scope::classify`).
    pub path: String,
    pub category: Category,
    pub kind: ItemKind,
    pub size: u64,
    /// SHA-256 of the content, lowercase hexadecimal.
    pub sha256: String,
}

impl Manifest {
    pub fn total_size(&self) -> u64 {
        self.items.iter().map(|item| item.size).sum()
    }

    /// Checks the format and the versions against this application.
    pub fn check_compatibility(&self, app_version: &str) -> Result<(), BackupError> {
        if self.format != FORMAT {
            return Err(BackupError::new("the file is not a YMDB backup"));
        }
        if self.format_version > FORMAT_VERSION {
            return Err(BackupError::new(format!(
                "the backup uses format version {}, created by a newer YMDB; this version \
                 reads format version {FORMAT_VERSION}",
                self.format_version
            )));
        }
        if self.format_version == 0 {
            return Err(BackupError::new("the backup format version is invalid"));
        }
        let created_by = parse_version(&self.app_version).ok_or_else(|| {
            BackupError::new(format!(
                "the backup was created by an unknown YMDB version '{}'",
                self.app_version
            ))
        })?;
        let current = parse_version(app_version).unwrap_or((0, 0, 0));
        if created_by > current {
            return Err(BackupError::new(format!(
                "the backup was created by YMDB {}, newer than this version ({app_version}). \
                 Restore it with YMDB {} or later",
                self.app_version, self.app_version
            )));
        }
        let schemas = Schemas::current();
        if self.schemas.installations > schemas.installations
            || self.schemas.catalog > schemas.catalog
            || self.schemas.support_files > schemas.support_files
        {
            return Err(BackupError::new(
                "the backup contains data formats newer than this version of YMDB supports",
            ));
        }
        Ok(())
    }
}

/// `major.minor.patch`, ignoring a pre-release or build suffix.
pub fn parse_version(version: &str) -> Option<(u64, u64, u64)> {
    let core = version.split(['-', '+']).next()?;
    let mut parts = core.split('.').map(|part| part.parse::<u64>().ok());
    let version = (parts.next()??, parts.next()??, parts.next()??);
    parts.next().is_none().then_some(version)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(app_version: &str, format_version: u32) -> Manifest {
        Manifest {
            format: FORMAT.to_owned(),
            format_version,
            app_version: app_version.to_owned(),
            created_at: 1,
            platform: Platform::current(),
            schemas: Schemas::current(),
            items: Vec::new(),
        }
    }

    #[test]
    fn accepts_the_same_and_older_versions() {
        for version in ["0.1.0", "0.0.9", "0.1.0-dev"] {
            assert_eq!(
                manifest(version, FORMAT_VERSION).check_compatibility("0.1.0"),
                Ok(()),
                "{version}"
            );
        }
    }

    #[test]
    fn refuses_newer_or_unknown_versions_and_formats() {
        for (version, format) in [
            ("0.2.0", FORMAT_VERSION),
            ("1.0.0", FORMAT_VERSION),
            ("0.1.0", FORMAT_VERSION + 1),
            ("0.1.0", 0),
            ("nonsense", FORMAT_VERSION),
            ("0.1", FORMAT_VERSION),
        ] {
            assert!(
                manifest(version, format)
                    .check_compatibility("0.1.0")
                    .is_err(),
                "{version} {format}"
            );
        }
        let mut other = manifest("0.1.0", FORMAT_VERSION);
        other.format = "something-else".to_owned();
        assert!(other.check_compatibility("0.1.0").is_err());
        let mut newer_schema = manifest("0.1.0", FORMAT_VERSION);
        newer_schema.schemas.catalog += 1;
        assert!(newer_schema.check_compatibility("0.1.0").is_err());
    }

    #[test]
    fn parses_versions() {
        assert_eq!(parse_version("1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_version("1.2.3-beta.1"), Some((1, 2, 3)));
        assert_eq!(parse_version("1.2"), None);
        assert_eq!(parse_version("1.2.3.4"), None);
    }
}
