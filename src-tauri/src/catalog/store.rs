//! SQLite storage of the catalog.
//!
//! The catalog lives in its own database file (`catalog.sqlite`). It is
//! derived data: every import builds a complete new database in a separate
//! file (`catalog.sqlite.partial`) and, only after the import has succeeded
//! and passed its integrity checks, atomically renames it over the current
//! catalog. Until then the previous catalog stays untouched and readable. A
//! failed, cancelled or interrupted import only leaves a partial file, which
//! is deleted (at the latest by the next import).
//!
//! A software list import follows the same rule: it copies the current
//! catalog into the partial file, replaces the software list tables there and
//! swaps the file. A machine import starts from an empty file, so it never
//! keeps software lists of another MAME build. Every replacement of the
//! catalog therefore goes through [`CatalogWriter`] and the same atomic
//! rename, which also lets a future full update write machines and software
//! lists into one partial file.
//!
//! On Windows a file cannot be renamed over while another handle has it open.
//! Every connection to `catalog.sqlite` is opened and closed while holding the
//! store's file lock, which the rename also holds. External programs that keep
//! the file open make the replacement fail with [`StoreError::Replace`]; the
//! previous catalog is then kept.
//!
//! Data owned by the user (such as future favorites or collections) must not
//! be stored in this file, since it is replaced on every import; it should
//! reference machines by their stable short name instead.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use serde::Serialize;

use super::model::Machine;
use super::schema::{self, MigrationError, SCHEMA_VERSION};
use super::software_model::{Software, SoftwareListHeader};

pub const CATALOG_FILE_NAME: &str = "catalog.sqlite";
const PARTIAL_FILE_NAME: &str = "catalog.sqlite.partial";
/// Maximum number of integrity violations included in an error message.
const MAX_REPORTED_VIOLATIONS: usize = 5;

#[derive(Debug)]
pub enum StoreError {
    Sqlite(rusqlite::Error),
    Io {
        path: PathBuf,
        source: io::Error,
    },
    /// The catalog was created with a different schema version.
    UnsupportedSchema {
        found: i64,
    },
    /// The imported data violates the catalog constraints.
    Integrity(String),
    /// The new catalog could not replace the current one (for example
    /// because another program has the catalog file open).
    Replace {
        path: PathBuf,
        source: io::Error,
    },
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sqlite(error) => write!(f, "database error: {error}"),
            Self::Io { path, source } => write!(f, "file error on '{}': {source}", path.display()),
            Self::Replace { path, source } => write!(
                f,
                "could not replace '{}': {source}. Close any other program that has the \
                 catalog open and try again",
                path.display()
            ),
            Self::UnsupportedSchema { found } => write!(
                f,
                "catalog schema version {found} is not supported (expected {SCHEMA_VERSION})"
            ),
            Self::Integrity(message) => write!(f, "catalog integrity check failed: {message}"),
        }
    }
}

impl std::error::Error for StoreError {}

impl From<rusqlite::Error> for StoreError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Sqlite(error)
    }
}

impl From<MigrationError> for StoreError {
    fn from(error: MigrationError) -> Self {
        match error {
            MigrationError::Sqlite(error) => Self::Sqlite(error),
            MigrationError::UnsupportedVersion { found } => Self::UnsupportedSchema { found },
        }
    }
}

/// Information about the MAME build a catalog is imported from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogSource {
    pub mame_release: String,
    pub mame_revision: String,
    pub executable_path: PathBuf,
    pub executable_size: Option<u64>,
    /// Modification time of the executable, in milliseconds since the Unix epoch.
    pub executable_modified_at: Option<u64>,
}

/// Values recorded when an import completes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletedImport {
    pub source: CatalogSource,
    /// `build` attribute of the `<mame>` root element.
    pub mame_build: Option<String>,
    pub mame_config: Option<String>,
    pub mame_debug: bool,
    pub imported_at: u64,
    pub import_duration_ms: u64,
}

/// Values recorded when a software list import completes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletedSoftwareImport {
    pub source: CatalogSource,
    pub imported_at: u64,
    pub import_duration_ms: u64,
}

/// Metadata and summary of the software lists of the current catalog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SoftwareCatalogInfo {
    pub mame_release: String,
    pub mame_revision: String,
    /// Milliseconds since the Unix epoch.
    pub imported_at: u64,
    pub import_duration_ms: u64,
    /// Whether the software lists come from the same MAME build and
    /// executable as the machines. Always true for imports made by YMDB.
    pub matches_machine_catalog: bool,
    pub statistics: SoftwareStatistics,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SoftwareStatistics {
    pub lists: u64,
    pub software: u64,
    pub clones: u64,
    /// Software flagged `supported="partial"`.
    pub partially_supported: u64,
    /// Software flagged `supported="no"`.
    pub unsupported: u64,
    pub parts: u64,
    pub data_areas: u64,
    pub roms: u64,
    pub disk_areas: u64,
    pub disks: u64,
    /// Distinct list names referenced by machines but not provided by MAME.
    pub missing_referenced_lists: u64,
}

/// Metadata and summary of the current catalog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogInfo {
    pub schema_version: i64,
    pub mame_release: String,
    pub mame_revision: String,
    pub mame_build: Option<String>,
    pub executable_path: PathBuf,
    #[serde(skip)]
    pub executable_size: Option<u64>,
    #[serde(skip)]
    pub executable_modified_at: Option<u64>,
    /// Milliseconds since the Unix epoch.
    pub imported_at: u64,
    pub import_duration_ms: u64,
    pub database_size: u64,
    pub statistics: CatalogStatistics,
    /// `None` until software lists are imported for this catalog.
    pub software: Option<SoftwareCatalogInfo>,
}

/// Counts describing the catalog content. Categories follow the flags of the
/// XML; machines are not classified further (for example as arcade or console).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogStatistics {
    /// All `<machine>` entries, including devices.
    pub machines: u64,
    /// Entries flagged `isdevice`.
    pub devices: u64,
    /// Entries flagged `isbios`.
    pub bios: u64,
    /// Entries flagged `ismechanical`.
    pub mechanical: u64,
    /// Non-device entries that are clones of another machine.
    pub clones: u64,
    /// Runnable entries that are not devices or BIOS sets.
    pub runnable_systems: u64,
    pub roms: u64,
    pub disks: u64,
    /// Distinct software lists referenced by machines.
    pub software_lists: u64,
}

/// Location of the catalog database files.
///
/// Clones share one lock that serializes every access to the catalog file
/// with its replacement. The lock also holds the cached read-only connection
/// used by queries (summary, search), so its page cache survives between
/// queries; the connection is closed before the file is replaced.
#[derive(Debug, Clone)]
pub struct CatalogStore {
    dir: PathBuf,
    file_lock: Arc<Mutex<Option<Connection>>>,
}

impl CatalogStore {
    pub fn in_dir(dir: &Path) -> Self {
        Self {
            dir: dir.to_path_buf(),
            file_lock: Arc::new(Mutex::new(None)),
        }
    }

    pub fn catalog_path(&self) -> PathBuf {
        self.dir.join(CATALOG_FILE_NAME)
    }

    fn partial_path(&self) -> PathBuf {
        self.dir.join(PARTIAL_FILE_NAME)
    }

    /// Reads the metadata and statistics of the current catalog, or `None`
    /// when no catalog has been imported yet.
    pub fn read_info(&self) -> Result<Option<CatalogInfo>, StoreError> {
        let path = self.catalog_path();
        self.with_reader(|connection| {
            let database_size = fs::metadata(&path).map_or(0, |metadata| metadata.len());
            read_info(connection, database_size)
        })
        .map(Option::flatten)
    }

    /// Runs `query` with a read-only connection to the current catalog, or
    /// returns `None` when no catalog has been imported yet.
    ///
    /// Catalogs created by an older release are upgraded in place first:
    /// migrations only add tables and indexes, so existing data stays valid.
    pub fn with_reader<T>(
        &self,
        query: impl FnOnce(&Connection) -> Result<T, StoreError>,
    ) -> Result<Option<T>, StoreError> {
        let path = self.catalog_path();
        let mut cached = self
            .file_lock
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if !path.is_file() {
            *cached = None;
            return Ok(None);
        }
        // The file may have been changed outside this store: reopen (and
        // upgrade) when the schema version is no longer the current one.
        let current = cached.as_ref().is_some_and(|connection| {
            schema::schema_version(connection).ok() == Some(SCHEMA_VERSION)
        });
        if !current {
            *cached = None;
            *cached = Some(open_reader(&path)?);
        }
        let connection = cached.as_ref().expect("connection was just opened");
        let result = query(connection);
        if result.is_err() {
            // Do not keep a connection in an unknown state.
            *cached = None;
        }
        result.map(Some)
    }

    /// Starts building a new catalog in the partial file.
    pub fn begin_import(&self) -> Result<CatalogWriter, StoreError> {
        let partial = self.prepare_partial()?;
        let mut connection = open_partial(&partial)?;
        schema::migrate(&mut connection)?;
        connection.execute_batch("PRAGMA foreign_keys = ON; BEGIN")?;
        Ok(CatalogWriter::new(connection, self.clone()))
    }

    /// Starts replacing the software lists of the current catalog: the
    /// catalog is copied into the partial file and its software list tables
    /// are emptied there. The current catalog is not modified.
    pub fn begin_software_import(&self) -> Result<CatalogWriter, StoreError> {
        let partial = self.prepare_partial()?;
        let catalog = self.catalog_path();
        {
            let _guard = self
                .file_lock
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            fs::copy(&catalog, &partial).map_err(|source| StoreError::Io {
                path: catalog.clone(),
                source,
            })?;
        }
        let writer_store = self.clone();
        let mut connection = open_partial(&partial)?;
        schema::migrate(&mut connection)?;
        // Foreign keys are off while emptying so that SQLite can truncate the
        // tables instead of checking every deleted row.
        connection.execute_batch(
            "DELETE FROM software_catalog_metadata;
             DELETE FROM software_disks;
             DELETE FROM software_disk_areas;
             DELETE FROM software_roms;
             DELETE FROM software_data_areas;
             DELETE FROM software_part_features;
             DELETE FROM software_parts;
             DELETE FROM software_shared_features;
             DELETE FROM software_info;
             DELETE FROM software;
             DELETE FROM software_lists;
             PRAGMA foreign_keys = ON;
             BEGIN",
        )?;
        Ok(CatalogWriter::new(connection, writer_store))
    }

    /// Creates the data directory and removes a leftover partial file.
    fn prepare_partial(&self) -> Result<PathBuf, StoreError> {
        fs::create_dir_all(&self.dir).map_err(|source| StoreError::Io {
            path: self.dir.clone(),
            source,
        })?;
        let partial = self.partial_path();
        remove_if_exists(&partial)?;
        Ok(partial)
    }
}

/// Opens the current catalog read-only, upgrading an older schema first.
fn open_reader(path: &Path) -> Result<Connection, StoreError> {
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let connection = Connection::open_with_flags(path, flags)?;
    let found = schema::schema_version(&connection)?;
    if found > SCHEMA_VERSION {
        return Err(StoreError::UnsupportedSchema { found });
    }
    if found == SCHEMA_VERSION {
        return Ok(connection);
    }
    drop(connection);
    let mut upgraded = Connection::open(path)?;
    schema::migrate(&mut upgraded)?;
    upgraded.close().map_err(|(_, error)| error)?;
    Ok(Connection::open_with_flags(path, flags)?)
}

/// Opens the partial file for writing. It is discarded on any failure and
/// synced explicitly before it replaces the catalog, so journaling is not
/// needed. Foreign keys stay off until the caller enables them.
fn open_partial(partial: &Path) -> Result<Connection, StoreError> {
    let connection = Connection::open(partial)?;
    connection.execute_batch(
        "PRAGMA journal_mode = OFF;
         PRAGMA synchronous = OFF;
         PRAGMA foreign_keys = OFF;",
    )?;
    Ok(connection)
}

/// Writes a new catalog. Dropping it without calling one of the `finish`
/// methods discards the partial catalog.
pub struct CatalogWriter {
    connection: Option<Connection>,
    store: CatalogStore,
    machines: u64,
    software_lists: u64,
    software: u64,
    /// List of the software inserted next.
    current_list: Option<String>,
}

impl CatalogWriter {
    fn new(connection: Connection, store: CatalogStore) -> Self {
        Self {
            connection: Some(connection),
            store,
            machines: 0,
            software_lists: 0,
            software: 0,
            current_list: None,
        }
    }

    fn connection(&self) -> &Connection {
        self.connection
            .as_ref()
            .expect("connection is open until finish")
    }

    pub fn machine_count(&self) -> u64 {
        self.machines
    }

    pub fn software_count(&self) -> u64 {
        self.software
    }

    pub fn insert_machine(&mut self, machine: &Machine) -> Result<(), StoreError> {
        insert_machine(self.connection(), machine)?;
        self.machines += 1;
        Ok(())
    }

    /// Inserts a software list; the software inserted next belong to it.
    pub fn insert_software_list(&mut self, list: &SoftwareListHeader) -> Result<(), StoreError> {
        self.connection()
            .prepare_cached(
                "INSERT INTO software_lists (name, description, notes) VALUES (?1, ?2, ?3)",
            )?
            .execute(params![list.name, list.description, list.notes])?;
        self.software_lists += 1;
        self.current_list = Some(list.name.clone());
        Ok(())
    }

    pub fn insert_software(&mut self, software: &Software) -> Result<(), StoreError> {
        let list = self.current_list.as_deref().ok_or_else(|| {
            StoreError::Integrity(format!(
                "software '{}' outside a software list",
                software.name
            ))
        })?;
        insert_software(self.connection(), list, software)?;
        self.software += 1;
        Ok(())
    }

    /// Records the machine catalog metadata, verifies the catalog, commits it
    /// and atomically replaces the current catalog with it.
    pub fn finish(self, completed: &CompletedImport) -> Result<CatalogInfo, StoreError> {
        let machines = self.machines;
        self.commit_and_replace(|connection| commit(connection, completed, machines))
    }

    /// Records the software list metadata, verifies the catalog, commits it
    /// and atomically replaces the current catalog with it.
    pub fn finish_software(
        self,
        completed: &CompletedSoftwareImport,
    ) -> Result<CatalogInfo, StoreError> {
        let (lists, software) = (self.software_lists, self.software);
        self.commit_and_replace(|connection| {
            commit_software(connection, completed, lists, software)
        })
    }

    fn commit_and_replace(
        mut self,
        commit: impl FnOnce(&Connection) -> Result<(), StoreError>,
    ) -> Result<CatalogInfo, StoreError> {
        let connection = self
            .connection
            .take()
            .expect("connection is open until finish");
        let committed = commit(&connection);
        // Close before the file is synced and renamed.
        let closed = connection
            .close()
            .map_err(|(_, error)| StoreError::from(error));

        let partial = self.store.partial_path();
        let replaced = committed
            .and(closed)
            .and_then(|()| sync_file(&partial))
            .and_then(|()| self.replace_catalog(&partial));
        if replaced.is_err() {
            // Best effort: a leftover file is removed by the next import.
            let _ = fs::remove_file(&partial);
        }
        replaced?;
        sync_dir(&self.store.dir);

        self.store
            .read_info()?
            .ok_or_else(|| StoreError::Integrity("the new catalog is missing".to_owned()))
    }
}

impl CatalogWriter {
    fn replace_catalog(&self, partial: &Path) -> Result<(), StoreError> {
        let target = self.store.catalog_path();
        let mut cached = self
            .store
            .file_lock
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        // Windows cannot rename over a file that is still open.
        *cached = None;
        fs::rename(partial, &target).map_err(|source| StoreError::Replace {
            path: target,
            source,
        })
    }
}

impl Drop for CatalogWriter {
    fn drop(&mut self) {
        if let Some(connection) = self.connection.take() {
            drop(connection);
            // Best effort: a leftover file is removed by the next import.
            let _ = fs::remove_file(self.store.partial_path());
        }
    }
}

fn commit(
    connection: &Connection,
    completed: &CompletedImport,
    machines: u64,
) -> Result<(), StoreError> {
    let stored =
        connection.query_row("SELECT COUNT(*) FROM machines", [], |row| get_count(row, 0))?;
    if stored != machines {
        return Err(StoreError::Integrity(format!(
            "{machines} machines were parsed but {stored} were stored"
        )));
    }
    check_foreign_keys(connection)?;

    let source = &completed.source;
    connection.execute(
        "INSERT INTO catalog_metadata (
            id, status, mame_release, mame_revision, mame_build, mame_config, mame_debug,
            executable_path, executable_size, executable_modified_at,
            imported_at, import_duration_ms, machine_count
        ) VALUES (1, 'complete', ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            source.mame_release,
            source.mame_revision,
            completed.mame_build,
            completed.mame_config,
            completed.mame_debug,
            path_text(&source.executable_path)?,
            source.executable_size.map(to_i64),
            source.executable_modified_at.map(to_i64),
            to_i64(completed.imported_at),
            to_i64(completed.import_duration_ms),
            to_i64(machines),
        ],
    )?;
    connection.execute_batch(schema::REBUILD_SEARCH_INDEXES)?;
    connection.execute_batch("COMMIT")?;
    Ok(())
}

fn commit_software(
    connection: &Connection,
    completed: &CompletedSoftwareImport,
    lists: u64,
    software: u64,
) -> Result<(), StoreError> {
    let (stored_lists, stored_software) = connection.query_row(
        "SELECT (SELECT COUNT(*) FROM software_lists), (SELECT COUNT(*) FROM software)",
        [],
        |row| Ok((get_count(row, 0)?, get_count(row, 1)?)),
    )?;
    if (stored_lists, stored_software) != (lists, software) {
        return Err(StoreError::Integrity(format!(
            "{lists} software lists and {software} software were parsed but \
             {stored_lists} and {stored_software} were stored"
        )));
    }
    // The software lists must come from the build that produced the machines.
    let source = &completed.source;
    let matches: bool = connection.query_row(
        "SELECT mame_release = ?1 AND mame_revision = ?2 AND executable_path = ?3
         FROM catalog_metadata WHERE id = 1",
        params![
            source.mame_release,
            source.mame_revision,
            path_text(&source.executable_path)?
        ],
        |row| row.get(0),
    )?;
    if !matches {
        return Err(StoreError::Integrity(
            "the software lists do not come from the MAME build of the machine catalog".to_owned(),
        ));
    }
    check_foreign_keys(connection)?;

    connection.execute(
        "INSERT INTO software_catalog_metadata (
            id, status, mame_release, mame_revision, executable_path, executable_size,
            executable_modified_at, imported_at, import_duration_ms, list_count, software_count
        ) VALUES (1, 'complete', ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            source.mame_release,
            source.mame_revision,
            path_text(&source.executable_path)?,
            source.executable_size.map(to_i64),
            source.executable_modified_at.map(to_i64),
            to_i64(completed.imported_at),
            to_i64(completed.import_duration_ms),
            to_i64(lists),
            to_i64(software),
        ],
    )?;
    connection.execute_batch(schema::REBUILD_SOFTWARE_SEARCH_INDEX)?;
    connection.execute_batch("COMMIT")?;
    Ok(())
}

/// Reports dangling references with a readable message before the commit
/// would reject them.
fn check_foreign_keys(connection: &Connection) -> Result<(), StoreError> {
    let mut statement = connection.prepare("PRAGMA foreign_key_check")?;
    let violations: Vec<(String, Option<i64>, String)> = statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
        .take(MAX_REPORTED_VIOLATIONS + 1)
        .collect::<Result<_, _>>()?;
    if violations.is_empty() {
        return Ok(());
    }
    let mut details: Vec<String> = violations
        .iter()
        .take(MAX_REPORTED_VIOLATIONS)
        .map(|(table, rowid, parent)| {
            let row = rowid.map_or_else(String::new, |id| format!(" row {id}"));
            format!("{table}{row} references a missing {parent}")
        })
        .collect();
    if violations.len() > MAX_REPORTED_VIOLATIONS {
        details.push("...".to_owned());
    }
    Err(StoreError::Integrity(details.join("; ")))
}

fn insert_software(
    connection: &Connection,
    list: &str,
    software: &Software,
) -> rusqlite::Result<()> {
    let software_id = connection
        .prepare_cached(
            "INSERT INTO software (
                list_name, name, clone_of, supported, description, year, publisher, notes
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )?
        .insert(params![
            list,
            software.name,
            software.clone_of,
            software.supported,
            software.description,
            software.year,
            software.publisher,
            software.notes,
        ])?;

    if !software.info.is_empty() {
        let mut statement = connection.prepare_cached(
            "INSERT INTO software_info (software_id, name, value) VALUES (?1, ?2, ?3)",
        )?;
        for info in &software.info {
            statement.execute(params![software_id, info.name, info.value])?;
        }
    }
    if !software.shared_features.is_empty() {
        let mut statement = connection.prepare_cached(
            "INSERT INTO software_shared_features (software_id, name, value) VALUES (?1, ?2, ?3)",
        )?;
        for feature in &software.shared_features {
            statement.execute(params![software_id, feature.name, feature.value])?;
        }
    }

    for part in &software.parts {
        let part_id = connection
            .prepare_cached(
                "INSERT INTO software_parts (software_id, name, interface) VALUES (?1, ?2, ?3)",
            )?
            .insert(params![software_id, part.name, part.interface])?;
        if !part.features.is_empty() {
            let mut statement = connection.prepare_cached(
                "INSERT INTO software_part_features (part_id, name, value) VALUES (?1, ?2, ?3)",
            )?;
            for feature in &part.features {
                statement.execute(params![part_id, feature.name, feature.value])?;
            }
        }
        for area in &part.data_areas {
            let area_id = connection
                .prepare_cached(
                    "INSERT INTO software_data_areas (part_id, name, size, databits, endian)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                )?
                .insert(params![
                    part_id,
                    area.name,
                    to_i64(area.size),
                    area.databits,
                    area.endian
                ])?;
            if !area.roms.is_empty() {
                let mut statement = connection.prepare_cached(
                    "INSERT INTO software_roms (
                        data_area_id, name, size, length, crc, sha1, offset, value, status,
                        load_flag
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                )?;
                for rom in &area.roms {
                    statement.execute(params![
                        area_id,
                        rom.name,
                        rom.size.map(to_i64),
                        rom.length.map(to_i64),
                        rom.crc,
                        rom.sha1,
                        rom.offset,
                        rom.value,
                        rom.status,
                        rom.load_flag,
                    ])?;
                }
            }
        }
        for area in &part.disk_areas {
            let area_id = connection
                .prepare_cached("INSERT INTO software_disk_areas (part_id, name) VALUES (?1, ?2)")?
                .insert(params![part_id, area.name])?;
            if !area.disks.is_empty() {
                let mut statement = connection.prepare_cached(
                    "INSERT INTO software_disks (disk_area_id, name, sha1, status, is_writeable)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                )?;
                for disk in &area.disks {
                    statement.execute(params![
                        area_id,
                        disk.name,
                        disk.sha1,
                        disk.status,
                        disk.writeable
                    ])?;
                }
            }
        }
    }
    Ok(())
}

fn insert_machine(connection: &Connection, machine: &Machine) -> rusqlite::Result<()> {
    let input = machine.input.as_ref();
    let driver = machine.driver.as_ref();
    connection
        .prepare_cached(
            "INSERT INTO machines (
                name, description, year, manufacturer, source_file,
                is_bios, is_device, is_mechanical, is_runnable, clone_of, rom_of, sample_of,
                sound_channels, input_players, input_coins, input_service, input_tilt,
                driver_status, driver_emulation, driver_cocktail, driver_save_state,
                driver_requires_artwork, driver_unofficial, driver_no_sound_hardware,
                driver_incomplete
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16,
                      ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25)",
        )?
        .execute(params![
            machine.name,
            machine.description,
            machine.year,
            machine.manufacturer,
            machine.source_file,
            machine.is_bios,
            machine.is_device,
            machine.is_mechanical,
            machine.runnable,
            machine.clone_of,
            machine.rom_of,
            machine.sample_of,
            machine.sound_channels,
            input.map(|i| i.players),
            input.and_then(|i| i.coins),
            input.map(|i| i.service),
            input.map(|i| i.tilt),
            driver.map(|d| &d.status),
            driver.and_then(|d| d.emulation.as_ref()),
            driver.and_then(|d| d.cocktail.as_ref()),
            driver.and_then(|d| d.save_state.as_ref()),
            driver.map(|d| d.requires_artwork),
            driver.map(|d| d.unofficial),
            driver.map(|d| d.no_sound_hardware),
            driver.map(|d| d.incomplete),
        ])?;

    let name = &machine.name;
    if !machine.bios_sets.is_empty() {
        let mut statement = connection.prepare_cached(
            "INSERT INTO machine_bios_sets (machine_name, name, description, is_default)
             VALUES (?1, ?2, ?3, ?4)",
        )?;
        for bios in &machine.bios_sets {
            statement.execute(params![name, bios.name, bios.description, bios.is_default])?;
        }
    }
    if !machine.roms.is_empty() {
        let mut statement = connection.prepare_cached(
            "INSERT INTO machine_roms (
                machine_name, name, bios, size, crc, sha1, merge_name, region, offset,
                status, is_optional
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        )?;
        for rom in &machine.roms {
            statement.execute(params![
                name,
                rom.name,
                rom.bios,
                to_i64(rom.size),
                rom.crc,
                rom.sha1,
                rom.merge,
                rom.region,
                rom.offset,
                rom.status,
                rom.optional,
            ])?;
        }
    }
    if !machine.disks.is_empty() {
        let mut statement = connection.prepare_cached(
            "INSERT INTO machine_disks (
                machine_name, name, sha1, merge_name, region, disk_index, is_writable,
                status, is_optional
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        )?;
        for disk in &machine.disks {
            statement.execute(params![
                name,
                disk.name,
                disk.sha1,
                disk.merge,
                disk.region,
                disk.index,
                disk.writable,
                disk.status,
                disk.optional,
            ])?;
        }
    }
    if !machine.device_refs.is_empty() {
        let mut statement = connection.prepare_cached(
            "INSERT INTO machine_device_refs (machine_name, device_name, instance_count)
             VALUES (?1, ?2, ?3)",
        )?;
        for device in &machine.device_refs {
            statement.execute(params![name, device.name, device.count])?;
        }
    }
    if !machine.samples.is_empty() {
        let mut statement = connection
            .prepare_cached("INSERT INTO machine_samples (machine_name, name) VALUES (?1, ?2)")?;
        for sample in &machine.samples {
            statement.execute(params![name, sample])?;
        }
    }
    if !machine.chips.is_empty() {
        let mut statement = connection.prepare_cached(
            "INSERT INTO machine_chips (machine_name, chip_type, name, tag, clock)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;
        for chip in &machine.chips {
            statement.execute(params![
                name,
                chip.chip_type,
                chip.name,
                chip.tag,
                chip.clock.map(to_i64),
            ])?;
        }
    }
    if !machine.displays.is_empty() {
        let mut statement = connection.prepare_cached(
            "INSERT INTO machine_displays (
                machine_name, tag, display_type, rotate, flip_x, width, height, refresh
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )?;
        for display in &machine.displays {
            statement.execute(params![
                name,
                display.tag,
                display.display_type,
                display.rotate,
                display.flip_x,
                display.width,
                display.height,
                display.refresh,
            ])?;
        }
    }
    if let Some(input) = input.filter(|input| !input.controls.is_empty()) {
        let mut statement = connection.prepare_cached(
            "INSERT INTO machine_controls (
                machine_name, control_type, player, buttons, required_buttons, ways, ways2, ways3
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )?;
        for control in &input.controls {
            statement.execute(params![
                name,
                control.control_type,
                control.player,
                control.buttons,
                control.required_buttons,
                control.ways,
                control.ways2,
                control.ways3,
            ])?;
        }
    }
    if !machine.features.is_empty() {
        let mut statement = connection.prepare_cached(
            "INSERT INTO machine_features (machine_name, feature_type, status, overall)
             VALUES (?1, ?2, ?3, ?4)",
        )?;
        for feature in &machine.features {
            statement.execute(params![
                name,
                feature.feature_type,
                feature.status,
                feature.overall
            ])?;
        }
    }
    if !machine.media_devices.is_empty() {
        let mut device_statement = connection.prepare_cached(
            "INSERT INTO machine_media_devices (
                machine_name, device_type, tag, interface, mandatory, fixed_image,
                instance_name, instance_brief_name
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )?;
        let mut extension_statement = connection.prepare_cached(
            "INSERT INTO machine_media_device_extensions (media_device_id, extension)
             VALUES (?1, ?2)",
        )?;
        for device in &machine.media_devices {
            let device_id = device_statement.insert(params![
                name,
                device.device_type,
                device.tag,
                device.interface,
                device.mandatory,
                device.fixed_image,
                device.instance_name,
                device.instance_brief_name,
            ])?;
            for extension in &device.extensions {
                extension_statement.execute(params![device_id, extension])?;
            }
        }
    }
    if !machine.software_lists.is_empty() {
        let mut statement = connection.prepare_cached(
            "INSERT INTO machine_software_lists (machine_name, list_name, tag, status, filter)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;
        for list in &machine.software_lists {
            statement.execute(params![name, list.name, list.tag, list.status, list.filter])?;
        }
    }
    if !machine.ram_options.is_empty() {
        let mut statement = connection.prepare_cached(
            "INSERT INTO machine_ram_options (machine_name, name, size, is_default)
             VALUES (?1, ?2, ?3, ?4)",
        )?;
        for ram in &machine.ram_options {
            statement.execute(params![name, ram.name, to_i64(ram.size), ram.is_default])?;
        }
    }
    Ok(())
}

fn read_info(
    connection: &Connection,
    database_size: u64,
) -> Result<Option<CatalogInfo>, StoreError> {
    let metadata = connection
        .query_row(
            "SELECT mame_release, mame_revision, mame_build, executable_path, executable_size,
                    executable_modified_at, imported_at, import_duration_ms
             FROM catalog_metadata WHERE id = 1",
            [],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<i64>>(4)?,
                    row.get::<_, Option<i64>>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, i64>(7)?,
                ))
            },
        )
        .optional()?;
    let Some((release, revision, build, path, size, modified_at, imported_at, duration)) = metadata
    else {
        return Err(StoreError::Integrity(
            "catalog metadata is missing".to_owned(),
        ));
    };

    let statistics = connection.query_row(
        "SELECT
            COUNT(*),
            COALESCE(SUM(is_device), 0),
            COALESCE(SUM(is_bios), 0),
            COALESCE(SUM(is_mechanical), 0),
            COALESCE(SUM(clone_of IS NOT NULL AND is_device = 0), 0),
            COALESCE(SUM(is_runnable = 1 AND is_device = 0 AND is_bios = 0), 0),
            (SELECT COUNT(*) FROM machine_roms),
            (SELECT COUNT(*) FROM machine_disks),
            (SELECT COUNT(DISTINCT list_name) FROM machine_software_lists)
         FROM machines",
        [],
        |row| {
            Ok(CatalogStatistics {
                machines: get_count(row, 0)?,
                devices: get_count(row, 1)?,
                bios: get_count(row, 2)?,
                mechanical: get_count(row, 3)?,
                clones: get_count(row, 4)?,
                runnable_systems: get_count(row, 5)?,
                roms: get_count(row, 6)?,
                disks: get_count(row, 7)?,
                software_lists: get_count(row, 8)?,
            })
        },
    )?;

    let software = read_software_info(connection)?;

    Ok(Some(CatalogInfo {
        schema_version: SCHEMA_VERSION,
        mame_release: release,
        mame_revision: revision,
        mame_build: build,
        executable_path: PathBuf::from(path),
        executable_size: size.and_then(|value| u64::try_from(value).ok()),
        executable_modified_at: modified_at.and_then(|value| u64::try_from(value).ok()),
        imported_at: u64::try_from(imported_at).unwrap_or(0),
        import_duration_ms: u64::try_from(duration).unwrap_or(0),
        database_size,
        statistics,
        software,
    }))
}

fn read_software_info(connection: &Connection) -> Result<Option<SoftwareCatalogInfo>, StoreError> {
    let metadata = connection
        .query_row(
            "SELECT s.mame_release, s.mame_revision, s.imported_at, s.import_duration_ms,
                    s.mame_release = c.mame_release
                        AND s.mame_revision = c.mame_revision
                        AND s.executable_path = c.executable_path
                        AND s.executable_size IS c.executable_size
                        AND s.executable_modified_at IS c.executable_modified_at
             FROM software_catalog_metadata s, catalog_metadata c
             WHERE s.id = 1 AND c.id = 1",
            [],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, bool>(4)?,
                ))
            },
        )
        .optional()?;
    let Some((release, revision, imported_at, duration, matches)) = metadata else {
        return Ok(None);
    };

    let statistics = connection.query_row(
        "SELECT
            (SELECT COUNT(*) FROM software_lists),
            (SELECT COUNT(*) FROM software),
            (SELECT COUNT(*) FROM software WHERE clone_of IS NOT NULL),
            (SELECT COUNT(*) FROM software WHERE supported = 'partial'),
            (SELECT COUNT(*) FROM software WHERE supported = 'no'),
            (SELECT COUNT(*) FROM software_parts),
            (SELECT COUNT(*) FROM software_data_areas),
            (SELECT COUNT(*) FROM software_roms),
            (SELECT COUNT(*) FROM software_disk_areas),
            (SELECT COUNT(*) FROM software_disks),
            (SELECT COUNT(DISTINCT m.list_name) FROM machine_software_lists m
             WHERE NOT EXISTS (SELECT 1 FROM software_lists l WHERE l.name = m.list_name))",
        [],
        |row| {
            Ok(SoftwareStatistics {
                lists: get_count(row, 0)?,
                software: get_count(row, 1)?,
                clones: get_count(row, 2)?,
                partially_supported: get_count(row, 3)?,
                unsupported: get_count(row, 4)?,
                parts: get_count(row, 5)?,
                data_areas: get_count(row, 6)?,
                roms: get_count(row, 7)?,
                disk_areas: get_count(row, 8)?,
                disks: get_count(row, 9)?,
                missing_referenced_lists: get_count(row, 10)?,
            })
        },
    )?;

    Ok(Some(SoftwareCatalogInfo {
        mame_release: release,
        mame_revision: revision,
        imported_at: u64::try_from(imported_at).unwrap_or(0),
        import_duration_ms: u64::try_from(duration).unwrap_or(0),
        matches_machine_catalog: matches,
        statistics,
    }))
}

fn path_text(path: &Path) -> Result<&str, StoreError> {
    path.to_str().ok_or_else(|| {
        StoreError::Integrity(format!(
            "executable path '{}' is not valid Unicode",
            path.display()
        ))
    })
}

/// Reads a non-negative count (SQLite integers are signed).
fn get_count(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<u64> {
    row.get::<_, i64>(index)
        .map(|value| u64::try_from(value).unwrap_or(0))
}

/// SQLite integers are signed 64-bit; values above `i64::MAX` do not occur
/// in practice and are clamped.
fn to_i64(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

fn remove_if_exists(path: &Path) -> Result<(), StoreError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(StoreError::Io {
            path: path.to_path_buf(),
            source,
        }),
    }
}

fn sync_file(path: &Path) -> Result<(), StoreError> {
    // Flushing requires write access on Windows.
    fs::OpenOptions::new()
        .write(true)
        .open(path)
        .and_then(|file| file.sync_all())
        .map_err(|source| StoreError::Io {
            path: path.to_path_buf(),
            source,
        })
}

/// Persists the rename on file systems that require syncing the directory.
fn sync_dir(dir: &Path) {
    #[cfg(unix)]
    if let Ok(dir) = fs::File::open(dir) {
        let _ = dir.sync_all();
    }
    #[cfg(not(unix))]
    let _ = dir;
}
