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
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sqlite(error) => write!(f, "database error: {error}"),
            Self::Io { path, source } => write!(f, "file error on '{}': {source}", path.display()),
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
/// Clones share a lock that serializes reads of the catalog file with its
/// replacement, so the file is never replaced while it is open.
#[derive(Debug, Clone)]
pub struct CatalogStore {
    dir: PathBuf,
    file_lock: Arc<Mutex<()>>,
}

impl CatalogStore {
    pub fn in_dir(dir: &Path) -> Self {
        Self {
            dir: dir.to_path_buf(),
            file_lock: Arc::new(Mutex::new(())),
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
        let _guard = self
            .file_lock
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if !path.is_file() {
            return Ok(None);
        }
        let connection = Connection::open_with_flags(
            &path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        let found = schema::schema_version(&connection)?;
        if found != SCHEMA_VERSION {
            return Err(StoreError::UnsupportedSchema { found });
        }
        let database_size = fs::metadata(&path).map_or(0, |metadata| metadata.len());
        read_info(&connection, database_size)
    }

    /// Starts building a new catalog in the partial file.
    pub fn begin_import(&self) -> Result<CatalogWriter, StoreError> {
        let partial = self.partial_path();
        fs::create_dir_all(&self.dir).map_err(|source| StoreError::Io {
            path: self.dir.clone(),
            source,
        })?;
        remove_if_exists(&partial)?;

        let mut connection = Connection::open(&partial)?;
        // The partial file is discarded on any failure and synced explicitly
        // before it replaces the catalog, so journaling is not needed.
        connection.execute_batch(
            "PRAGMA journal_mode = OFF;
             PRAGMA synchronous = OFF;
             PRAGMA foreign_keys = ON;",
        )?;
        schema::migrate(&mut connection)?;
        connection.execute_batch("BEGIN")?;

        Ok(CatalogWriter {
            connection: Some(connection),
            store: self.clone(),
            machines: 0,
        })
    }
}

/// Writes a new catalog. Dropping it without calling
/// [`finish`](Self::finish) discards the partial catalog.
pub struct CatalogWriter {
    connection: Option<Connection>,
    store: CatalogStore,
    machines: u64,
}

impl CatalogWriter {
    pub fn machine_count(&self) -> u64 {
        self.machines
    }

    pub fn insert_machine(&mut self, machine: &Machine) -> Result<(), StoreError> {
        let connection = self
            .connection
            .as_ref()
            .expect("connection is open until finish");
        insert_machine(connection, machine)?;
        self.machines += 1;
        Ok(())
    }

    /// Records the metadata, verifies the catalog, commits it and atomically
    /// replaces the current catalog with it.
    pub fn finish(mut self, completed: &CompletedImport) -> Result<CatalogInfo, StoreError> {
        let connection = self
            .connection
            .take()
            .expect("connection is open until finish");
        let committed = commit(&connection, completed, self.machines);
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
        let _guard = self
            .store
            .file_lock
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        fs::rename(partial, &target).map_err(|source| StoreError::Io {
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
