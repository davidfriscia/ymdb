//! Migration from the single-installation layout.
//!
//! Before multi-installation support, YMDB stored one executable in
//! `settings.json`, one catalog in `<data>/catalog.sqlite` and one launch
//! configuration in `<config>/launch.json`. The migration registers that
//! installation and gives it the existing catalog and launch configuration:
//!
//! 1. Plan: read the old files (MAME is never run; the version comes from the
//!    catalog metadata when the catalog was imported from that executable)
//!    and save the registry with a `pending` migration record naming the
//!    installations that receive the catalog and the launch configuration.
//! 2. Move the catalog into the installation's directory (a rename on the same
//!    volume: nothing is re-imported or copied) and copy `launch.json`.
//! 3. Verify the moved catalog (release and machine count recorded in step 1)
//!    and the copied launch configuration (same bytes).
//! 4. Mark the migration `completed`.
//!
//! Every step can run again: an interrupted migration resumes from the saved
//! record with the same installation ids, and a completed one is never run
//! again. A failure leaves the record `pending` and every file where it is.
//! `settings.json` and `launch.json` are never modified, so they remain as a
//! backup of the old configuration.
//!
//! A catalog imported from another executable than the configured one is
//! given to a separate installation for that executable, so a catalog is
//! never associated with a build it does not come from.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::registry::{
    ExecutableIdentity, InstallationRegistry, LegacyMigration, MameInstallation, MigratedCatalog,
    MigrationState, RegistryError,
};
use super::store::{RegistryStore, RegistryStoreError};
use super::{AppLayout, new_id};
use crate::atomic_file;
use crate::catalog::store::{CatalogInfo, CatalogStore};
use crate::launcher::store::LAUNCH_FILE_NAME;
use crate::mame::platform;
use crate::mame::version::MameVersion;
use crate::settings::{SettingsError, SettingsStore};

#[derive(Debug)]
pub enum MigrationError {
    Registry(RegistryStoreError),
    /// The old `settings.json` cannot be read.
    LegacySettings(SettingsError),
    Plan(RegistryError),
    Io {
        path: PathBuf,
        source: io::Error,
    },
    /// A migrated file does not match what was recorded before moving it.
    Verification(String),
}

impl fmt::Display for MigrationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Registry(error) => error.fmt(f),
            Self::LegacySettings(error) => write!(
                f,
                "{error}. Fix or remove the file so that YMDB can upgrade its configuration"
            ),
            Self::Plan(error) => error.fmt(f),
            Self::Io { path, source } => write!(
                f,
                "could not migrate '{}': {source}. Close any other program (such as another \
                 YMDB) that uses it and try again",
                path.display()
            ),
            Self::Verification(message) => write!(f, "migration check failed: {message}"),
        }
    }
}

impl std::error::Error for MigrationError {}

impl From<RegistryStoreError> for MigrationError {
    fn from(error: RegistryStoreError) -> Self {
        Self::Registry(error)
    }
}

/// Loads the registry, migrating the single-installation layout first when
/// needed. Without a registry and without anything to migrate, returns an
/// empty registry without writing any file.
pub fn load_or_migrate(layout: &AppLayout) -> Result<InstallationRegistry, MigrationError> {
    let store = RegistryStore::in_dir(&layout.config_dir);
    let registry = match store.load()? {
        Some(registry) => registry,
        None => match plan(layout)? {
            Some(registry) => {
                store.save(&registry)?;
                registry
            }
            None => return Ok(InstallationRegistry::default()),
        },
    };
    let pending = registry
        .legacy_migration
        .as_ref()
        .is_some_and(|migration| migration.state == MigrationState::Pending);
    if pending {
        complete(layout, &store, registry)
    } else {
        Ok(registry)
    }
}

/// Step 1: the installations created from the old files, or `None` when
/// there is no old configuration or catalog.
pub(super) fn plan(layout: &AppLayout) -> Result<Option<InstallationRegistry>, MigrationError> {
    let settings = SettingsStore::in_dir(&layout.config_dir)
        .load()
        .map_err(MigrationError::LegacySettings)?;
    let configured = settings.mame.map(|mame| mame.executable_path);
    let legacy_catalog = CatalogStore::in_dir(&layout.data_dir);
    let catalog_exists = legacy_catalog.catalog_path().is_file();
    if configured.is_none() && !catalog_exists {
        return Ok(None);
    }
    // An unreadable catalog still moves with the configured installation,
    // which then reports it; it is never deleted.
    let catalog_info = if catalog_exists {
        legacy_catalog.read_info().ok().flatten()
    } else {
        None
    };
    let launch_exists = layout.config_dir.join(LAUNCH_FILE_NAME).is_file();
    let now = now_millis();

    let mut registry = InstallationRegistry::default();
    let mut catalog_owner = None;
    if let Some(path) = configured {
        let mut installation = MameInstallation::new(new_id(), path, now);
        let catalog_matches = catalog_info.as_ref().is_some_and(|info| {
            platform::same_executable(&info.executable_path, &installation.executable_path)
        });
        if catalog_matches && let Some(info) = &catalog_info {
            adopt_catalog_version(&mut installation, info);
        }
        if catalog_exists && (catalog_matches || catalog_info.is_none()) {
            catalog_owner = Some(installation.id.clone());
        }
        registry.add(installation).map_err(MigrationError::Plan)?;
    }
    if catalog_owner.is_none()
        && let Some(info) = &catalog_info
    {
        let mut installation = MameInstallation::new(new_id(), info.executable_path.clone(), now);
        adopt_catalog_version(&mut installation, info);
        catalog_owner = Some(installation.id.clone());
        registry.add(installation).map_err(MigrationError::Plan)?;
    }
    // The launch options were used with the configured executable, which is
    // the first installation when there is one.
    let launch_owner = registry
        .installations
        .first()
        .filter(|_| launch_exists)
        .map(|installation| installation.id.clone());

    registry.legacy_migration = Some(LegacyMigration {
        state: MigrationState::Pending,
        catalog: catalog_owner.map(|installation_id| MigratedCatalog {
            installation_id,
            mame_release: catalog_info.as_ref().map(|info| info.mame_release.clone()),
            machines: catalog_info.as_ref().map(|info| info.statistics.machines),
        }),
        launch_config_installation_id: launch_owner,
        started_at: now,
        completed_at: None,
    });
    Ok(Some(registry))
}

/// The catalog records the version and file of the executable it was
/// imported from: the installation starts verified as of the import.
fn adopt_catalog_version(installation: &mut MameInstallation, info: &CatalogInfo) {
    installation.version = Some(MameVersion {
        release: info.mame_release.clone(),
        revision: info.mame_revision.clone(),
    });
    installation.identity = info.executable_size.map(|size| ExecutableIdentity {
        size,
        modified_at: info.executable_modified_at,
    });
    installation.verified_at = Some(info.imported_at);
}

/// Steps 2 to 4.
fn complete(
    layout: &AppLayout,
    store: &RegistryStore,
    mut registry: InstallationRegistry,
) -> Result<InstallationRegistry, MigrationError> {
    let Some(migration) = registry.legacy_migration.clone() else {
        return Ok(registry);
    };
    if let Some(catalog) = &migration.catalog {
        move_catalog(layout, catalog)?;
    }
    if let Some(id) = &migration.launch_config_installation_id {
        copy_launch_config(layout, id)?;
    }
    registry.legacy_migration = Some(LegacyMigration {
        state: MigrationState::Completed,
        completed_at: Some(now_millis()),
        ..migration
    });
    store.save(&registry)?;
    Ok(registry)
}

fn move_catalog(layout: &AppLayout, catalog: &MigratedCatalog) -> Result<(), MigrationError> {
    let legacy = CatalogStore::in_dir(&layout.data_dir);
    let target = CatalogStore::in_dir(&layout.catalog_dir(&catalog.installation_id));
    let from = legacy.catalog_path();
    let to = target.catalog_path();
    // When both exist, the move already happened and the old location was
    // written again (for example by an older YMDB): the moved catalog wins
    // and the other file is left untouched.
    if from.is_file() && !to.exists() {
        fs::create_dir_all(target.dir()).map_err(|source| io_error(target.dir(), source))?;
        fs::rename(&from, &to).map_err(|source| io_error(&from, source))?;
    }
    // An import interrupted in the old layout leaves only a partial file,
    // which is never a usable catalog.
    legacy
        .remove_partial()
        .map_err(|error| MigrationError::Verification(error.to_string()))?;

    let expected = catalog.mame_release.as_ref().zip(catalog.machines);
    match (target.read_info(), expected) {
        (Ok(Some(info)), Some((release, machines))) => {
            if &info.mame_release != release || info.statistics.machines != machines {
                return Err(MigrationError::Verification(format!(
                    "the moved catalog contains MAME {} with {} machines, expected MAME {release} \
                     with {machines} machines",
                    info.mame_release, info.statistics.machines
                )));
            }
        }
        (Ok(None) | Err(_), Some(_)) if to.exists() => {
            return Err(MigrationError::Verification(format!(
                "the moved catalog '{}' cannot be read",
                to.display()
            )));
        }
        // The catalog was unreadable before the move, or is gone: there is
        // nothing to compare.
        _ => {}
    }
    Ok(())
}

fn copy_launch_config(layout: &AppLayout, id: &str) -> Result<(), MigrationError> {
    let from = layout.config_dir.join(LAUNCH_FILE_NAME);
    let to = layout.launch_config_dir(id).join(LAUNCH_FILE_NAME);
    let content = match fs::read(&from) {
        Ok(content) => content,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(source) => return Err(io_error(&from, source)),
    };
    if to.exists() {
        // Copied by an interrupted run.
        return Ok(());
    }
    atomic_file::write(&to, &content).map_err(|source| io_error(&to, source))?;
    let copied = fs::read(&to).map_err(|source| io_error(&to, source))?;
    if copied != content {
        return Err(MigrationError::Verification(format!(
            "'{}' differs from '{}'",
            to.display(),
            from.display()
        )));
    }
    Ok(())
}

fn io_error(path: &Path, source: io::Error) -> MigrationError {
    MigrationError::Io {
        path: path.to_path_buf(),
        source,
    }
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}
