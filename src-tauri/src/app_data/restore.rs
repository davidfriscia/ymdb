//! Import: full restore of a backup, replacing the current application data.
//!
//! Nothing is replaced while YMDB runs. The procedure has two parts:
//!
//! 1. [`stage`] (while YMDB runs, nothing current is modified): the archive
//!    is checked (`archive::open`), every item is extracted into
//!    `<root>/.ymdb-restore/staged/` with its size and checksum verified,
//!    then every extracted file is loaded by the store that reads it in
//!    normal use and every database is checked by SQLite. The result is the
//!    preview shown to the user. Cancelling removes the staging directories.
//! 2. [`schedule`] (after the user's confirmation, with the data gate held
//!    and no MAME session, import or download running) writes the
//!    transaction file `<config>/.ymdb-restore/transaction.json`; YMDB then
//!    restarts. [`recover`] runs at the next start, before any data is
//!    opened: it moves the current data to `.ymdb-restore/rollback/`, moves
//!    the staged data into place and checks it. On failure the previous data
//!    is put back. The transaction file records the phase and which items
//!    existed before, so a restore interrupted at any point (crash, power
//!    loss) is rolled back at the following start.
//!
//! Every move is a rename within the same root, so no data is copied or
//! partially written by the swap itself. A rollback can still fail when the
//! system refuses a rename (for example a file kept open by another
//! program); the transaction file is then kept and recovery is retried at the
//! next start. Files outside the replaced names of each root (window state,
//! launch logs, generated controller files, MAME's own files, ROMs) are never
//! touched.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{self, BufWriter};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::archive::{self, Limits};
use super::inventory::{self, ExternalReference};
use super::manifest::{Manifest, Platform};
use super::scope::{self, Category, ItemKind, RESTORE_WORK_DIR, replaced_names};
use super::sqlite;
use super::{AppDirs, BackupError, Phase, Progress, Root};
use crate::atomic_file;
use crate::controllers::store::ControllersStore;
use crate::installations::store::RegistryStore;
use crate::launcher::store::LaunchConfigStore;
use crate::lists::store::ListsStore;
use crate::media::config::MediaConfigStore;
use crate::remote::connections::ConnectionsStore;
use crate::remote::sources::RemoteSourcesStore;
use crate::saved_filters::SavedFiltersStore;

const STAGED_DIR: &str = "staged";
const ROLLBACK_DIR: &str = "rollback";
const TRANSACTION_FILE: &str = "transaction.json";
const MANIFEST_COPY: &str = "manifest.json";
const TRANSACTION_VERSION: u32 = 1;
/// Free space kept in addition to the extracted size.
const SPACE_MARGIN: u64 = 64 * 1024 * 1024;
/// Retries of a failed rename (100 ms apart), for files briefly in use.
const RENAME_ATTEMPTS: u32 = 50;

fn work_dir(dirs: &AppDirs, root: Root) -> PathBuf {
    dirs.root(root).join(RESTORE_WORK_DIR)
}

fn staged_dir(dirs: &AppDirs, root: Root) -> PathBuf {
    work_dir(dirs, root).join(STAGED_DIR)
}

fn rollback_dir(dirs: &AppDirs, root: Root) -> PathBuf {
    work_dir(dirs, root).join(ROLLBACK_DIR)
}

fn transaction_path(dirs: &AppDirs) -> PathBuf {
    work_dir(dirs, Root::Config).join(TRANSACTION_FILE)
}

fn preferences_path(dirs: &AppDirs) -> PathBuf {
    work_dir(dirs, Root::Config).join(scope::INTERFACE_PREFERENCES)
}

/// Files of one category, for the preview.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CategorySummary {
    pub category: Category,
    pub label: &'static str,
    pub files: u64,
    pub size: u64,
}

/// What a restore would do, shown before the user confirms it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestorePreview {
    pub archive_path: PathBuf,
    pub format_version: u32,
    pub app_version: String,
    pub created_at: u64,
    pub platform: Platform,
    pub same_platform: bool,
    pub total_size: u64,
    /// The data in the backup.
    pub categories: Vec<CategorySummary>,
    /// The current data that the restore replaces.
    pub replaced: Vec<CategorySummary>,
    /// Paths referenced by the backup's configuration, checked on this computer.
    pub external_references: Vec<ExternalReference>,
    /// Non-blocking remarks (older version, other platform).
    pub notes: Vec<String>,
}

/// Result of a restore, reported once after the restart.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum RestoreOutcome {
    Restored {
        created_at: u64,
        app_version: String,
        /// To be written back to the webview storage by the frontend.
        interface_preferences: Option<serde_json::Value>,
    },
    Failed {
        message: String,
        /// Whether the data in place is the data from before the restore.
        previous_data_kept: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Transaction {
    version: u32,
    phase: TransactionPhase,
    backup_created_at: u64,
    backup_app_version: String,
    /// Replaced names that existed in each root before the swap.
    #[serde(default)]
    originals: BTreeMap<Root, Vec<String>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
enum TransactionPhase {
    /// Staged and confirmed: the swap runs at the next start.
    Staged,
    /// Current data is being moved aside and replaced.
    Swapping,
    /// The restored data is in place; only cleanup remains.
    Committed,
}

/// Whether a confirmed restore waits for the next start.
pub fn has_pending(dirs: &AppDirs) -> bool {
    transaction_path(dirs).exists()
}

/// Checks and extracts a backup into the staging directories and returns
/// its preview. The current data is not modified.
pub fn stage(
    archive_path: &Path,
    dirs: &AppDirs,
    app_version: &str,
    progress: &Progress,
) -> Result<RestorePreview, BackupError> {
    if has_pending(dirs) {
        return Err(BackupError::new(
            "a restore is already waiting for YMDB to restart",
        ));
    }
    discard(dirs);
    let result = stage_checked(archive_path, dirs, app_version, progress);
    if result.is_err() {
        discard(dirs);
    }
    result
}

fn stage_checked(
    archive_path: &Path,
    dirs: &AppDirs,
    app_version: &str,
    progress: &Progress,
) -> Result<RestorePreview, BackupError> {
    progress.start(Phase::Checking, 0);
    let mut checked = archive::open(archive_path, &Limits::default())?;
    let manifest = checked.manifest.clone();
    manifest.check_compatibility(app_version)?;
    check_space(dirs, &manifest)?;

    progress.start(Phase::Extracting, manifest.total_size());
    let targets: Vec<PathBuf> = checked
        .items()
        .map(|(_, classified)| match classified.root {
            Some(root) => classified
                .components
                .iter()
                .fold(staged_dir(dirs, root), |path, component| {
                    path.join(component)
                }),
            None => preferences_path(dirs),
        })
        .collect();
    for (index, target) in targets.iter().enumerate() {
        progress.check_cancel()?;
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| BackupError::io("create", parent, &error))?;
        }
        let file =
            File::create_new(target).map_err(|error| BackupError::io("create", target, &error))?;
        let mut writer = BufWriter::new(file);
        checked.copy_item(index, &mut writer, progress)?;
        let file = writer
            .into_inner()
            .map_err(|error| BackupError::io("write", target, error.error()))?;
        file.sync_all()
            .map_err(|error| BackupError::io("write", target, &error))?;
    }
    drop(checked);
    let manifest_copy = work_dir(dirs, Root::Config).join(MANIFEST_COPY);
    let content = serde_json::to_vec_pretty(&manifest)
        .map_err(|error| BackupError::new(error.to_string()))?;
    atomic_file::write(&manifest_copy, &content)
        .map_err(|error| BackupError::io("write", &manifest_copy, &error))?;

    progress.start(Phase::Checking, 0);
    verify_staged(dirs, &manifest)?;
    Ok(preview(archive_path, dirs, &manifest, app_version))
}

fn check_space(dirs: &AppDirs, manifest: &Manifest) -> Result<(), BackupError> {
    for root in Root::ALL {
        let needed: u64 = manifest
            .items
            .iter()
            .filter(|item| {
                scope::classify(&item.path)
                    .is_some_and(|classified| classified.root.unwrap_or(Root::Config) == root)
            })
            .map(|item| item.size)
            .sum();
        let mut probe = dirs.root(root).to_path_buf();
        while !probe.exists() {
            if !probe.pop() {
                break;
            }
        }
        // Unknown free space (unusual file systems): let the copy report it.
        if let Ok(available) = fs4::available_space(&probe)
            && available < needed.saturating_add(SPACE_MARGIN)
        {
            return Err(BackupError::new(format!(
                "not enough free space on the disk of '{}': {needed} bytes are needed",
                dirs.root(root).display()
            )));
        }
    }
    Ok(())
}

/// Loads every staged file with the store that reads it in normal use and
/// checks every database: a restore is only offered when the data can be used.
fn verify_staged(dirs: &AppDirs, manifest: &Manifest) -> Result<(), BackupError> {
    fn check<T, E: std::fmt::Display>(result: Result<T, E>) -> Result<(), BackupError> {
        result
            .map(drop)
            .map_err(|error| BackupError::new(format!("the backup cannot be used: {error}")))
    }
    let config = staged_dir(dirs, Root::Config);
    check(RegistryStore::in_dir(&config).load())?;
    check(ListsStore::in_dir(&config).load())?;
    check(ControllersStore::in_dir(&config).load())?;
    check(SavedFiltersStore::in_dir(&config).load())?;
    check(MediaConfigStore::in_dir(&config).load())?;
    check(ConnectionsStore::in_dir(&config).load())?;
    for id in super::export::installation_dirs(&config.join(scope::INSTALLATIONS_DIR))? {
        let dir = config.join(scope::INSTALLATIONS_DIR).join(id);
        check(LaunchConfigStore::in_dir(&dir).load())?;
        check(RemoteSourcesStore::in_dir(&dir).load())?;
    }
    for item in &manifest.items {
        let Some(classified) = scope::classify(&item.path) else {
            continue;
        };
        let Some(root) = classified.root else {
            let content = fs::read(preferences_path(dirs))
                .map_err(|error| BackupError::new(error.to_string()))?;
            let value: serde_json::Value = serde_json::from_slice(&content)
                .map_err(|_| BackupError::new("the interface preferences are invalid"))?;
            super::export::check_interface_preferences(&value)?;
            continue;
        };
        if classified.kind != ItemKind::Sqlite {
            continue;
        }
        let path = classified
            .components
            .iter()
            .fold(staged_dir(dirs, root), |path, component| {
                path.join(component)
            });
        let version = sqlite::check_file(&path)
            .map_err(|error| BackupError::new(format!("'{}': {error}", item.path)))?;
        let supported = match classified.category {
            Category::Catalogs => crate::catalog::schema::SCHEMA_VERSION,
            _ => crate::support_files::repository::SCHEMA_VERSION,
        };
        if version > supported {
            return Err(BackupError::new(format!(
                "'{}' uses a database version newer than this YMDB supports",
                item.path
            )));
        }
    }
    Ok(())
}

fn preview(
    archive_path: &Path,
    dirs: &AppDirs,
    manifest: &Manifest,
    app_version: &str,
) -> RestorePreview {
    let mut notes = Vec::new();
    let current = Platform::current();
    let same_platform = manifest.platform.os == current.os;
    if !same_platform {
        notes.push(format!(
            "The backup was created on {}. Paths of MAME executables, media directories and \
             ROM destinations probably do not exist on this computer.",
            manifest.platform.os
        ));
    }
    if manifest.app_version != app_version {
        notes.push(format!(
            "The backup was created by YMDB {}. Its data is upgraded to this version when YMDB \
             reads it.",
            manifest.app_version
        ));
    }
    if manifest.items.is_empty() {
        notes.push("The backup contains no data: restoring it resets YMDB.".to_owned());
    }
    let items: Vec<(Category, u64)> = manifest
        .items
        .iter()
        .map(|item| (item.category, item.size))
        .collect();
    RestorePreview {
        archive_path: archive_path.to_path_buf(),
        format_version: manifest.format_version,
        app_version: manifest.app_version.clone(),
        created_at: manifest.created_at,
        platform: manifest.platform.clone(),
        same_platform,
        total_size: manifest.total_size(),
        categories: summarize(&items),
        replaced: summarize(&current_items(dirs)),
        external_references: inventory::external_references(&staged_dir(dirs, Root::Config)),
        notes,
    }
}

fn summarize(items: &[(Category, u64)]) -> Vec<CategorySummary> {
    let mut summaries: BTreeMap<Category, (u64, u64)> = BTreeMap::new();
    for (category, size) in items {
        let entry = summaries.entry(*category).or_default();
        entry.0 += 1;
        entry.1 += size;
    }
    summaries
        .into_iter()
        .map(|(category, (files, size))| CategorySummary {
            category,
            label: category.label(),
            files,
            size,
        })
        .collect()
}

/// The current files a restore replaces, by category.
fn current_items(dirs: &AppDirs) -> Vec<(Category, u64)> {
    let mut items = Vec::new();
    for root in Root::ALL {
        for name in replaced_names(root) {
            collect(
                &dirs.root(root).join(&name),
                &format!("{}/{name}", root.name()),
                &mut items,
            );
        }
    }
    items
}

fn collect(path: &Path, logical: &str, items: &mut Vec<(Category, u64)>) {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return;
    };
    if metadata.is_dir() {
        for entry in fs::read_dir(path).into_iter().flatten().flatten() {
            if let Ok(name) = entry.file_name().into_string() {
                collect(&entry.path(), &format!("{logical}/{name}"), items);
            }
        }
    } else if let Some(classified) = scope::classify(logical) {
        items.push((classified.category, metadata.len()));
    }
}

/// Removes the staging directories of a restore that was not confirmed,
/// and leftovers of a completed one. Never touches a pending restore.
pub fn discard(dirs: &AppDirs) {
    if has_pending(dirs) {
        return;
    }
    for root in Root::ALL {
        let _ = fs::remove_dir_all(work_dir(dirs, root));
    }
}

/// Confirms the staged restore; it is applied at the next start.
pub fn schedule(dirs: &AppDirs) -> Result<(), BackupError> {
    let manifest = read_manifest_copy(dirs)?;
    write_transaction(
        dirs,
        &Transaction {
            version: TRANSACTION_VERSION,
            phase: TransactionPhase::Staged,
            backup_created_at: manifest.created_at,
            backup_app_version: manifest.app_version,
            originals: BTreeMap::new(),
        },
    )
}

fn read_manifest_copy(dirs: &AppDirs) -> Result<Manifest, BackupError> {
    let path = work_dir(dirs, Root::Config).join(MANIFEST_COPY);
    let content = fs::read(&path).map_err(|_| BackupError::new("no restore is prepared"))?;
    serde_json::from_slice(&content)
        .map_err(|error| BackupError::new(format!("the prepared restore is invalid: {error}")))
}

fn write_transaction(dirs: &AppDirs, transaction: &Transaction) -> Result<(), BackupError> {
    let path = transaction_path(dirs);
    let content = serde_json::to_vec_pretty(transaction)
        .map_err(|error| BackupError::new(error.to_string()))?;
    atomic_file::write(&path, &content).map_err(|error| BackupError::io("write", &path, &error))
}

/// Applies, finishes or rolls back a restore left by the previous run. Runs
/// at startup before any application data is opened. `None` when there was
/// nothing to do (leftover work directories are removed).
pub fn recover(dirs: &AppDirs) -> Option<RestoreOutcome> {
    let path = transaction_path(dirs);
    let content = match fs::read(&path) {
        Ok(content) => content,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            discard(dirs);
            return None;
        }
        Err(error) => return Some(unreadable_transaction(&path, &error.to_string())),
    };
    let transaction: Transaction = match serde_json::from_slice(&content) {
        Ok(transaction) => transaction,
        Err(error) => return Some(unreadable_transaction(&path, &error.to_string())),
    };
    Some(match transaction.phase {
        TransactionPhase::Staged => swap(dirs, transaction),
        TransactionPhase::Swapping => roll_back_after(
            dirs,
            &transaction,
            "the previous restore was interrupted".to_owned(),
        ),
        TransactionPhase::Committed => finish(dirs, &transaction),
    })
}

fn unreadable_transaction(path: &Path, error: &str) -> RestoreOutcome {
    RestoreOutcome::Failed {
        message: format!(
            "the state of a restore ('{}') cannot be read: {error}. YMDB changed nothing; \
             the files in its '{RESTORE_WORK_DIR}' directories were kept",
            path.display()
        ),
        previous_data_kept: false,
    }
}

fn swap(dirs: &AppDirs, mut transaction: Transaction) -> RestoreOutcome {
    let manifest = match read_manifest_copy(dirs) {
        Ok(manifest) => manifest,
        Err(error) => return abandon(dirs, error.to_string()),
    };
    transaction.originals = Root::ALL
        .into_iter()
        .map(|root| {
            let existing = replaced_names(root)
                .into_iter()
                .filter(|name| fs::symlink_metadata(dirs.root(root).join(name)).is_ok())
                .collect();
            (root, existing)
        })
        .collect();
    transaction.phase = TransactionPhase::Swapping;
    if let Err(error) = write_transaction(dirs, &transaction) {
        return abandon(dirs, error.to_string());
    }
    match move_into_place(dirs, &transaction, &manifest) {
        Ok(()) => {
            transaction.phase = TransactionPhase::Committed;
            // Without this record a crash would be rolled back: still safe.
            let _ = write_transaction(dirs, &transaction);
            finish(dirs, &transaction)
        }
        Err(error) => roll_back_after(dirs, &transaction, error.to_string()),
    }
}

/// Gives up a restore before any current data was moved.
fn abandon(dirs: &AppDirs, message: String) -> RestoreOutcome {
    let _ = fs::remove_file(transaction_path(dirs));
    discard(dirs);
    RestoreOutcome::Failed {
        message,
        previous_data_kept: true,
    }
}

fn move_into_place(
    dirs: &AppDirs,
    transaction: &Transaction,
    manifest: &Manifest,
) -> Result<(), BackupError> {
    for root in Root::ALL {
        let rollback = rollback_dir(dirs, root);
        fs::create_dir_all(&rollback)
            .map_err(|error| BackupError::io("create", &rollback, &error))?;
        for name in transaction.originals.get(&root).into_iter().flatten() {
            let current = dirs.root(root).join(name);
            rename(&current, rollback.join(name))
                .map_err(|error| BackupError::io("move aside", &current, &error))?;
        }
    }
    for root in Root::ALL {
        let staged = staged_dir(dirs, root);
        let Ok(entries) = fs::read_dir(&staged) else {
            continue;
        };
        let names = replaced_names(root);
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !names.contains(&name) {
                return Err(BackupError::new(format!(
                    "unexpected staged file '{}'",
                    entry.path().display()
                )));
            }
            rename(entry.path(), dirs.root(root).join(&name))
                .map_err(|error| BackupError::io("move", &entry.path(), &error))?;
        }
    }
    // Every file of the backup must now be in place with its size.
    for item in &manifest.items {
        let Some(classified) = scope::classify(&item.path) else {
            return Err(BackupError::new(format!("invalid item '{}'", item.path)));
        };
        let Some(root) = classified.root else {
            continue;
        };
        let path = classified
            .components
            .iter()
            .fold(dirs.root(root).to_path_buf(), |path, component| {
                path.join(component)
            });
        let size = fs::metadata(&path).map(|metadata| metadata.len()).ok();
        if size != Some(item.size) {
            return Err(BackupError::new(format!(
                "'{}' is missing or incomplete after the restore",
                path.display()
            )));
        }
    }
    Ok(())
}

fn roll_back_after(dirs: &AppDirs, transaction: &Transaction, message: String) -> RestoreOutcome {
    match roll_back(dirs, transaction) {
        Ok(()) => {
            let _ = fs::remove_file(transaction_path(dirs));
            discard(dirs);
            RestoreOutcome::Failed {
                message: format!("{message}. The previous data was put back"),
                previous_data_kept: true,
            }
        }
        // The transaction file stays: the next start tries again.
        Err(error) => RestoreOutcome::Failed {
            message: format!(
                "{message}. The previous data could not be put back yet ({error}); YMDB tries \
                 again at the next start. Nothing was deleted: it is in the '{RESTORE_WORK_DIR}' \
                 directories"
            ),
            previous_data_kept: false,
        },
    }
}

/// Puts the data from before the swap back in place. Safe to repeat: an
/// item is either in the rollback copy (moved aside), still in place (never
/// moved), or absent before the restore (anything there now came from the
/// backup, which still exists, and is removed).
fn roll_back(dirs: &AppDirs, transaction: &Transaction) -> Result<(), BackupError> {
    for root in Root::ALL {
        let rollback = rollback_dir(dirs, root);
        let originals = transaction.originals.get(&root);
        for name in replaced_names(root) {
            let place = dirs.root(root).join(&name);
            let saved = rollback.join(&name);
            if fs::symlink_metadata(&saved).is_ok() {
                remove_any(&place)?;
                rename(&saved, &place)
                    .map_err(|error| BackupError::io("put back", &place, &error))?;
            } else if !originals.is_some_and(|names| names.contains(&name)) {
                remove_any(&place)?;
            }
        }
    }
    Ok(())
}

/// Renames, retrying for a few seconds: right after the restart the previous
/// process may still be closing its files, and some programs (antivirus,
/// indexers) open new files briefly.
fn rename(from: impl AsRef<Path>, to: impl AsRef<Path>) -> io::Result<()> {
    let mut attempts = 0;
    loop {
        match fs::rename(&from, &to) {
            Err(error) if attempts < RENAME_ATTEMPTS && error.kind() != io::ErrorKind::NotFound => {
                attempts += 1;
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            result => return result,
        }
    }
}

fn remove_any(path: &Path) -> Result<(), BackupError> {
    let result = match fs::symlink_metadata(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => Err(error),
        Ok(metadata) if metadata.is_dir() => fs::remove_dir_all(path),
        Ok(_) => fs::remove_file(path),
    };
    result.map_err(|error| BackupError::io("remove", path, &error))
}

fn finish(dirs: &AppDirs, transaction: &Transaction) -> RestoreOutcome {
    let interface_preferences = fs::read(preferences_path(dirs))
        .ok()
        .and_then(|content| serde_json::from_slice(&content).ok());
    for root in Root::ALL {
        let _ = fs::remove_dir_all(rollback_dir(dirs, root));
    }
    let _ = fs::remove_file(transaction_path(dirs));
    discard(dirs);
    RestoreOutcome::Restored {
        created_at: transaction.backup_created_at,
        app_version: transaction.backup_app_version.clone(),
        interface_preferences,
    }
}
