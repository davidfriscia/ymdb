//! Whether a catalog still matches the configured MAME executable.
//!
//! The check never runs MAME: it compares the path, size and modification
//! time recorded at import time with the configured executable. A catalog is
//! never assumed to be valid for a different executable or an updated one.

use std::fs;
use std::path::Path;
use std::time::UNIX_EPOCH;

use serde::Serialize;

use super::store::CatalogInfo;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CatalogFreshness {
    /// Imported from the configured executable, which has not changed since.
    Current,
    /// The configured executable was modified (for example updated in place).
    ExecutableChanged,
    /// The catalog was imported from another executable.
    DifferentExecutable,
    /// The configured executable no longer exists.
    ExecutableMissing,
    /// No MAME executable is configured.
    MameNotConfigured,
}

pub fn check_freshness(info: &CatalogInfo, configured: Option<&Path>) -> CatalogFreshness {
    let Some(configured) = configured else {
        return CatalogFreshness::MameNotConfigured;
    };
    if configured != info.executable_path {
        return CatalogFreshness::DifferentExecutable;
    }
    let Ok(metadata) = fs::metadata(configured) else {
        return CatalogFreshness::ExecutableMissing;
    };
    let modified_at = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .and_then(|elapsed| u64::try_from(elapsed.as_millis()).ok());
    let size_changed = info
        .executable_size
        .is_some_and(|size| size != metadata.len());
    let time_changed = info
        .executable_modified_at
        .is_some_and(|recorded| Some(recorded) != modified_at);
    if size_changed || time_changed {
        CatalogFreshness::ExecutableChanged
    } else {
        CatalogFreshness::Current
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::catalog::store::CatalogStatistics;

    fn info_for(path: &Path) -> CatalogInfo {
        let metadata = fs::metadata(path).expect("metadata");
        CatalogInfo {
            schema_version: 1,
            mame_release: "0.280".to_owned(),
            mame_revision: "mame0280".to_owned(),
            mame_build: None,
            executable_path: path.to_path_buf(),
            executable_size: Some(metadata.len()),
            executable_modified_at: metadata
                .modified()
                .ok()
                .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                .and_then(|elapsed| u64::try_from(elapsed.as_millis()).ok()),
            imported_at: 1,
            import_duration_ms: 1,
            database_size: 1,
            software: None,
            statistics: CatalogStatistics {
                machines: 0,
                devices: 0,
                bios: 0,
                mechanical: 0,
                clones: 0,
                runnable_systems: 0,
                roms: 0,
                disks: 0,
                software_lists: 0,
            },
        }
    }

    #[test]
    fn reports_each_freshness_state() {
        let dir = tempfile::tempdir().expect("temp dir");
        let executable = dir.path().join("mame");
        fs::write(&executable, "build 1").expect("write");
        let info = info_for(&executable);

        assert_eq!(
            check_freshness(&info, Some(&executable)),
            CatalogFreshness::Current
        );
        assert_eq!(
            check_freshness(&info, None),
            CatalogFreshness::MameNotConfigured
        );
        assert_eq!(
            check_freshness(&info, Some(&PathBuf::from("/other/mame"))),
            CatalogFreshness::DifferentExecutable
        );

        fs::write(&executable, "build 2 with a different size").expect("rewrite");
        assert_eq!(
            check_freshness(&info, Some(&executable)),
            CatalogFreshness::ExecutableChanged
        );

        fs::remove_file(&executable).expect("remove");
        assert_eq!(
            check_freshness(&info, Some(&executable)),
            CatalogFreshness::ExecutableMissing
        );
    }
}
