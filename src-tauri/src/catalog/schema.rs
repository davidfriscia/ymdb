//! Versioned schema of the catalog database.
//!
//! The schema version is stored in `PRAGMA user_version`. Migrations are
//! applied in order, each in its own transaction. Version `N` is reached by
//! applying the first `N` entries of [`MIGRATIONS`]; existing entries must
//! never be modified once released.

use rusqlite::Connection;

/// Ordered list of migrations. The schema version equals its length.
const MIGRATIONS: &[&str] = &[MIGRATION_1, MIGRATION_2];

pub const SCHEMA_VERSION: i64 = MIGRATIONS.len() as i64;

/// Initial catalog schema: machines from `mame -listxml` and import metadata.
///
/// Conventions:
/// - Machines are referenced by their MAME short name (`machines.name`), which
///   is stable across MAME versions; integer ids are local to one import.
/// - Relations between machines (`clone_of`, `rom_of`, device references) are
///   deferred foreign keys because the XML may reference machines listed later.
/// - `sample_of` is not a foreign key: it names a sample set, which is not
///   always a machine.
/// - Child tables have no uniqueness constraints: MAME may list the same item
///   more than once for a machine (for example ROMs with the same name).
/// - Enumerated values that grow across MAME versions (driver status, feature
///   types, display types) are stored as text without CHECK constraints.
const MIGRATION_1: &str = "
CREATE TABLE catalog_metadata (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    status TEXT NOT NULL CHECK (status IN ('complete')),
    mame_release TEXT NOT NULL,
    mame_revision TEXT NOT NULL,
    mame_build TEXT,
    mame_config TEXT,
    mame_debug INTEGER NOT NULL CHECK (mame_debug IN (0, 1)),
    executable_path TEXT NOT NULL,
    executable_size INTEGER,
    executable_modified_at INTEGER,
    imported_at INTEGER NOT NULL,
    import_duration_ms INTEGER NOT NULL,
    machine_count INTEGER NOT NULL CHECK (machine_count >= 0)
) STRICT;

CREATE TABLE machines (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    description TEXT NOT NULL,
    year TEXT,
    manufacturer TEXT,
    source_file TEXT,
    is_bios INTEGER NOT NULL CHECK (is_bios IN (0, 1)),
    is_device INTEGER NOT NULL CHECK (is_device IN (0, 1)),
    is_mechanical INTEGER NOT NULL CHECK (is_mechanical IN (0, 1)),
    is_runnable INTEGER NOT NULL CHECK (is_runnable IN (0, 1)),
    clone_of TEXT REFERENCES machines (name) DEFERRABLE INITIALLY DEFERRED,
    rom_of TEXT REFERENCES machines (name) DEFERRABLE INITIALLY DEFERRED,
    sample_of TEXT,
    sound_channels INTEGER,
    input_players INTEGER,
    input_coins INTEGER,
    input_service INTEGER CHECK (input_service IN (0, 1)),
    input_tilt INTEGER CHECK (input_tilt IN (0, 1)),
    driver_status TEXT,
    driver_emulation TEXT,
    driver_cocktail TEXT,
    driver_save_state TEXT,
    driver_requires_artwork INTEGER CHECK (driver_requires_artwork IN (0, 1)),
    driver_unofficial INTEGER CHECK (driver_unofficial IN (0, 1)),
    driver_no_sound_hardware INTEGER CHECK (driver_no_sound_hardware IN (0, 1)),
    driver_incomplete INTEGER CHECK (driver_incomplete IN (0, 1)),
    CHECK (clone_of IS NULL OR clone_of <> name)
) STRICT;

CREATE INDEX machines_clone_of ON machines (clone_of);
CREATE INDEX machines_rom_of ON machines (rom_of);

CREATE TABLE machine_bios_sets (
    id INTEGER PRIMARY KEY,
    machine_name TEXT NOT NULL REFERENCES machines (name) ON DELETE CASCADE,
    name TEXT NOT NULL,
    description TEXT NOT NULL,
    is_default INTEGER NOT NULL CHECK (is_default IN (0, 1))
) STRICT;
CREATE INDEX machine_bios_sets_machine ON machine_bios_sets (machine_name);

CREATE TABLE machine_roms (
    id INTEGER PRIMARY KEY,
    machine_name TEXT NOT NULL REFERENCES machines (name) ON DELETE CASCADE,
    name TEXT NOT NULL,
    bios TEXT,
    size INTEGER NOT NULL CHECK (size >= 0),
    crc TEXT,
    sha1 TEXT,
    merge_name TEXT,
    region TEXT,
    offset TEXT,
    status TEXT NOT NULL,
    is_optional INTEGER NOT NULL CHECK (is_optional IN (0, 1))
) STRICT;
CREATE INDEX machine_roms_machine ON machine_roms (machine_name);

CREATE TABLE machine_disks (
    id INTEGER PRIMARY KEY,
    machine_name TEXT NOT NULL REFERENCES machines (name) ON DELETE CASCADE,
    name TEXT NOT NULL,
    sha1 TEXT,
    merge_name TEXT,
    region TEXT,
    disk_index INTEGER,
    is_writable INTEGER NOT NULL CHECK (is_writable IN (0, 1)),
    status TEXT NOT NULL,
    is_optional INTEGER NOT NULL CHECK (is_optional IN (0, 1))
) STRICT;
CREATE INDEX machine_disks_machine ON machine_disks (machine_name);

CREATE TABLE machine_device_refs (
    machine_name TEXT NOT NULL REFERENCES machines (name) ON DELETE CASCADE,
    device_name TEXT NOT NULL REFERENCES machines (name) DEFERRABLE INITIALLY DEFERRED,
    instance_count INTEGER NOT NULL CHECK (instance_count > 0),
    PRIMARY KEY (machine_name, device_name)
) STRICT, WITHOUT ROWID;
CREATE INDEX machine_device_refs_device ON machine_device_refs (device_name);

CREATE TABLE machine_samples (
    id INTEGER PRIMARY KEY,
    machine_name TEXT NOT NULL REFERENCES machines (name) ON DELETE CASCADE,
    name TEXT NOT NULL
) STRICT;
CREATE INDEX machine_samples_machine ON machine_samples (machine_name);

CREATE TABLE machine_chips (
    id INTEGER PRIMARY KEY,
    machine_name TEXT NOT NULL REFERENCES machines (name) ON DELETE CASCADE,
    chip_type TEXT NOT NULL,
    name TEXT NOT NULL,
    tag TEXT,
    clock INTEGER
) STRICT;
CREATE INDEX machine_chips_machine ON machine_chips (machine_name);

CREATE TABLE machine_displays (
    id INTEGER PRIMARY KEY,
    machine_name TEXT NOT NULL REFERENCES machines (name) ON DELETE CASCADE,
    tag TEXT,
    display_type TEXT NOT NULL,
    rotate INTEGER,
    flip_x INTEGER NOT NULL CHECK (flip_x IN (0, 1)),
    width INTEGER,
    height INTEGER,
    refresh REAL
) STRICT;
CREATE INDEX machine_displays_machine ON machine_displays (machine_name);

CREATE TABLE machine_controls (
    id INTEGER PRIMARY KEY,
    machine_name TEXT NOT NULL REFERENCES machines (name) ON DELETE CASCADE,
    control_type TEXT NOT NULL,
    player INTEGER,
    buttons INTEGER,
    required_buttons INTEGER,
    ways TEXT,
    ways2 TEXT,
    ways3 TEXT
) STRICT;
CREATE INDEX machine_controls_machine ON machine_controls (machine_name);

CREATE TABLE machine_features (
    id INTEGER PRIMARY KEY,
    machine_name TEXT NOT NULL REFERENCES machines (name) ON DELETE CASCADE,
    feature_type TEXT NOT NULL,
    status TEXT,
    overall TEXT
) STRICT;
CREATE INDEX machine_features_machine ON machine_features (machine_name);

CREATE TABLE machine_media_devices (
    id INTEGER PRIMARY KEY,
    machine_name TEXT NOT NULL REFERENCES machines (name) ON DELETE CASCADE,
    device_type TEXT NOT NULL,
    tag TEXT,
    interface TEXT,
    mandatory TEXT,
    fixed_image TEXT,
    instance_name TEXT,
    instance_brief_name TEXT
) STRICT;
CREATE INDEX machine_media_devices_machine ON machine_media_devices (machine_name);

CREATE TABLE machine_media_device_extensions (
    id INTEGER PRIMARY KEY,
    media_device_id INTEGER NOT NULL REFERENCES machine_media_devices (id) ON DELETE CASCADE,
    extension TEXT NOT NULL
) STRICT;
CREATE INDEX machine_media_device_extensions_device
    ON machine_media_device_extensions (media_device_id);

-- `list_name` will reference the software list table once software lists
-- are imported (planned).
CREATE TABLE machine_software_lists (
    id INTEGER PRIMARY KEY,
    machine_name TEXT NOT NULL REFERENCES machines (name) ON DELETE CASCADE,
    list_name TEXT NOT NULL,
    tag TEXT,
    status TEXT NOT NULL,
    filter TEXT
) STRICT;
CREATE INDEX machine_software_lists_machine ON machine_software_lists (machine_name);
CREATE INDEX machine_software_lists_list ON machine_software_lists (list_name);

CREATE TABLE machine_ram_options (
    id INTEGER PRIMARY KEY,
    machine_name TEXT NOT NULL REFERENCES machines (name) ON DELETE CASCADE,
    name TEXT,
    size INTEGER NOT NULL CHECK (size >= 0),
    is_default INTEGER NOT NULL CHECK (is_default IN (0, 1))
) STRICT;
CREATE INDEX machine_ram_options_machine ON machine_ram_options (machine_name);
";

/// Software lists from `mame -listsoftware`.
///
/// Conventions (in addition to those of [`MIGRATION_1`]):
/// - A software is identified by its list and its short name: names are only
///   unique within a list.
/// - `software.clone_of` references a software of the same list through a
///   deferred composite foreign key (parents may be listed after clones).
/// - Machines are linked to software lists by name through
///   `machine_software_lists.list_name` = `software_lists.name`. This is not a
///   foreign key because machines may reference lists that MAME does not ship.
/// - `software_catalog_metadata` exists only when the software lists were
///   imported from the same MAME build as the machines in this catalog.
const MIGRATION_2: &str = "
CREATE TABLE software_catalog_metadata (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    status TEXT NOT NULL CHECK (status IN ('complete')),
    mame_release TEXT NOT NULL,
    mame_revision TEXT NOT NULL,
    executable_path TEXT NOT NULL,
    executable_size INTEGER,
    executable_modified_at INTEGER,
    imported_at INTEGER NOT NULL,
    import_duration_ms INTEGER NOT NULL,
    list_count INTEGER NOT NULL CHECK (list_count >= 0),
    software_count INTEGER NOT NULL CHECK (software_count >= 0)
) STRICT;

CREATE TABLE software_lists (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    description TEXT,
    notes TEXT
) STRICT;

CREATE TABLE software (
    id INTEGER PRIMARY KEY,
    list_name TEXT NOT NULL REFERENCES software_lists (name) ON DELETE CASCADE,
    name TEXT NOT NULL,
    clone_of TEXT,
    supported TEXT NOT NULL,
    description TEXT NOT NULL,
    year TEXT,
    publisher TEXT,
    notes TEXT,
    UNIQUE (list_name, name),
    CHECK (clone_of IS NULL OR clone_of <> name),
    FOREIGN KEY (list_name, clone_of) REFERENCES software (list_name, name)
        DEFERRABLE INITIALLY DEFERRED
) STRICT;
CREATE INDEX software_clone_of ON software (list_name, clone_of);

CREATE TABLE software_info (
    id INTEGER PRIMARY KEY,
    software_id INTEGER NOT NULL REFERENCES software (id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    value TEXT
) STRICT;
CREATE INDEX software_info_software ON software_info (software_id);

CREATE TABLE software_shared_features (
    id INTEGER PRIMARY KEY,
    software_id INTEGER NOT NULL REFERENCES software (id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    value TEXT
) STRICT;
CREATE INDEX software_shared_features_software ON software_shared_features (software_id);

CREATE TABLE software_parts (
    id INTEGER PRIMARY KEY,
    software_id INTEGER NOT NULL REFERENCES software (id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    interface TEXT NOT NULL
) STRICT;
CREATE INDEX software_parts_software ON software_parts (software_id);
CREATE INDEX software_parts_interface ON software_parts (interface);

CREATE TABLE software_part_features (
    id INTEGER PRIMARY KEY,
    part_id INTEGER NOT NULL REFERENCES software_parts (id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    value TEXT
) STRICT;
CREATE INDEX software_part_features_part ON software_part_features (part_id);

CREATE TABLE software_data_areas (
    id INTEGER PRIMARY KEY,
    part_id INTEGER NOT NULL REFERENCES software_parts (id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    size INTEGER NOT NULL CHECK (size >= 0),
    databits INTEGER NOT NULL,
    endian TEXT NOT NULL
) STRICT;
CREATE INDEX software_data_areas_part ON software_data_areas (part_id);

CREATE TABLE software_roms (
    id INTEGER PRIMARY KEY,
    data_area_id INTEGER NOT NULL REFERENCES software_data_areas (id) ON DELETE CASCADE,
    name TEXT,
    size INTEGER CHECK (size >= 0),
    length INTEGER,
    crc TEXT,
    sha1 TEXT,
    offset TEXT,
    value TEXT,
    status TEXT NOT NULL,
    load_flag TEXT
) STRICT;
CREATE INDEX software_roms_data_area ON software_roms (data_area_id);

CREATE TABLE software_disk_areas (
    id INTEGER PRIMARY KEY,
    part_id INTEGER NOT NULL REFERENCES software_parts (id) ON DELETE CASCADE,
    name TEXT NOT NULL
) STRICT;
CREATE INDEX software_disk_areas_part ON software_disk_areas (part_id);

CREATE TABLE software_disks (
    id INTEGER PRIMARY KEY,
    disk_area_id INTEGER NOT NULL REFERENCES software_disk_areas (id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    sha1 TEXT,
    status TEXT NOT NULL,
    is_writeable INTEGER NOT NULL CHECK (is_writeable IN (0, 1))
) STRICT;
CREATE INDEX software_disks_disk_area ON software_disks (disk_area_id);
";

#[derive(Debug)]
pub enum MigrationError {
    Sqlite(rusqlite::Error),
    /// The database was created by a newer version of the application.
    UnsupportedVersion {
        found: i64,
    },
}

impl From<rusqlite::Error> for MigrationError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Sqlite(error)
    }
}

pub fn schema_version(connection: &Connection) -> rusqlite::Result<i64> {
    connection.pragma_query_value(None, "user_version", |row| row.get(0))
}

/// Applies the pending migrations. Returns the number of migrations applied.
pub fn migrate(connection: &mut Connection) -> Result<usize, MigrationError> {
    let current = schema_version(connection)?;
    if current > SCHEMA_VERSION {
        return Err(MigrationError::UnsupportedVersion { found: current });
    }
    let pending = MIGRATIONS
        .iter()
        .enumerate()
        .skip(usize::try_from(current).unwrap_or(0));
    let mut applied = 0;
    for (index, sql) in pending {
        let transaction = connection.transaction()?;
        transaction.execute_batch(sql)?;
        transaction.pragma_update(
            None,
            "user_version",
            i64::try_from(index + 1).unwrap_or(i64::MAX),
        )?;
        transaction.commit()?;
        applied += 1;
    }
    Ok(applied)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open() -> Connection {
        let connection = Connection::open_in_memory().expect("open");
        connection
            .pragma_update(None, "foreign_keys", true)
            .expect("foreign keys");
        connection
    }

    fn table_names(connection: &Connection) -> Vec<String> {
        let mut statement = connection
            .prepare("SELECT name FROM sqlite_schema WHERE type = 'table' ORDER BY name")
            .expect("prepare");
        statement
            .query_map([], |row| row.get(0))
            .expect("query")
            .collect::<Result<_, _>>()
            .expect("rows")
    }

    #[test]
    fn creates_schema_on_empty_database() {
        let mut connection = open();

        let applied = migrate(&mut connection).expect("migrate");

        assert_eq!(applied, MIGRATIONS.len());
        assert_eq!(
            schema_version(&connection).expect("version"),
            SCHEMA_VERSION
        );
        let tables = table_names(&connection);
        for table in [
            "catalog_metadata",
            "machines",
            "machine_roms",
            "machine_disks",
            "machine_bios_sets",
            "machine_device_refs",
            "machine_software_lists",
            "machine_media_devices",
        ] {
            assert!(tables.iter().any(|t| t == table), "missing table {table}");
        }
    }

    #[test]
    fn migration_is_idempotent() {
        let mut connection = open();
        migrate(&mut connection).expect("first");

        let applied = migrate(&mut connection).expect("second");

        assert_eq!(applied, 0);
        assert_eq!(
            schema_version(&connection).expect("version"),
            SCHEMA_VERSION
        );
    }

    #[test]
    fn rejects_database_from_newer_version() {
        let mut connection = open();
        connection
            .pragma_update(None, "user_version", SCHEMA_VERSION + 1)
            .expect("set version");

        let result = migrate(&mut connection);

        assert!(matches!(
            result,
            Err(MigrationError::UnsupportedVersion { found }) if found == SCHEMA_VERSION + 1
        ));
    }

    #[test]
    fn enforces_deferred_machine_references_at_commit() {
        let mut connection = open();
        migrate(&mut connection).expect("migrate");

        // A forward reference is accepted while the transaction is open...
        let transaction = connection.transaction().expect("begin");
        transaction
            .execute_batch(
                "INSERT INTO machines (name, description, is_bios, is_device, is_mechanical, is_runnable, clone_of)
                 VALUES ('pacman', 'Pac-Man', 0, 0, 0, 1, 'puckman');
                 INSERT INTO machines (name, description, is_bios, is_device, is_mechanical, is_runnable)
                 VALUES ('puckman', 'Puck Man', 0, 0, 0, 1);",
            )
            .expect("insert");
        transaction.commit().expect("valid references commit");

        // ...but a dangling reference fails at commit.
        let transaction = connection.transaction().expect("begin");
        transaction
            .execute(
                "INSERT INTO machines (name, description, is_bios, is_device, is_mechanical, is_runnable, clone_of)
                 VALUES ('orphan', 'Orphan', 0, 0, 0, 1, 'missing')",
                [],
            )
            .expect("insert is deferred");
        assert!(transaction.commit().is_err());
    }

    #[test]
    fn enforces_types_and_checks() {
        let mut connection = open();
        migrate(&mut connection).expect("migrate");

        let invalid_flag = connection.execute(
            "INSERT INTO machines (name, description, is_bios, is_device, is_mechanical, is_runnable)
             VALUES ('a', 'A', 2, 0, 0, 1)",
            [],
        );
        let wrong_type = connection.execute(
            "INSERT INTO machines (name, description, is_bios, is_device, is_mechanical, is_runnable)
             VALUES ('b', 'B', 'yes', 0, 0, 1)",
            [],
        );
        let self_clone = connection.execute(
            "INSERT INTO machines (name, description, is_bios, is_device, is_mechanical, is_runnable, clone_of)
             VALUES ('c', 'C', 0, 0, 0, 1, 'c')",
            [],
        );
        let orphan_rom = connection.execute(
            "INSERT INTO machine_roms (machine_name, name, size, status, is_optional)
             VALUES ('nobody', 'r', 1, 'good', 0)",
            [],
        );

        assert!(invalid_flag.is_err());
        assert!(wrong_type.is_err());
        assert!(self_clone.is_err());
        assert!(orphan_rom.is_err());
    }

    /// Applies only the first `version` migrations, as an older release did.
    fn migrate_to(connection: &mut Connection, version: usize) {
        for (index, sql) in MIGRATIONS.iter().enumerate().take(version) {
            let transaction = connection.transaction().expect("begin");
            transaction.execute_batch(sql).expect("migration");
            transaction
                .pragma_update(
                    None,
                    "user_version",
                    i64::try_from(index + 1).expect("version"),
                )
                .expect("version");
            transaction.commit().expect("commit");
        }
    }

    #[test]
    fn migrates_version_1_catalog_and_keeps_machines() {
        let mut connection = open();
        migrate_to(&mut connection, 1);
        connection
            .execute_batch(
                "INSERT INTO machines (name, description, is_bios, is_device, is_mechanical, is_runnable)
                 VALUES ('c64', 'Commodore 64', 0, 0, 0, 1);
                 INSERT INTO machine_software_lists (machine_name, list_name, status)
                 VALUES ('c64', 'c64_cart', 'original');",
            )
            .expect("version 1 data");

        let applied = migrate(&mut connection).expect("migrate");

        assert_eq!(applied, MIGRATIONS.len() - 1);
        assert_eq!(
            schema_version(&connection).expect("version"),
            SCHEMA_VERSION
        );
        let machines: i64 = connection
            .query_row("SELECT COUNT(*) FROM machines", [], |row| row.get(0))
            .expect("count");
        let software_lists: i64 = connection
            .query_row("SELECT COUNT(*) FROM software_lists", [], |row| row.get(0))
            .expect("count");
        assert_eq!((machines, software_lists), (1, 0));
    }

    fn insert_software(
        connection: &Connection,
        list: &str,
        name: &str,
        clone_of: Option<&str>,
    ) -> rusqlite::Result<usize> {
        connection.execute(
            "INSERT INTO software (list_name, name, clone_of, supported, description)
             VALUES (?1, ?2, ?3, 'yes', ?2)",
            rusqlite::params![list, name, clone_of],
        )
    }

    #[test]
    fn identifies_software_by_list_and_name() {
        let mut connection = open();
        migrate(&mut connection).expect("migrate");
        connection
            .execute_batch(
                "INSERT INTO software_lists (name) VALUES ('nes');
                 INSERT INTO software_lists (name) VALUES ('c64_cart');",
            )
            .expect("lists");

        insert_software(&connection, "nes", "tetris", None).expect("nes tetris");
        insert_software(&connection, "c64_cart", "tetris", None).expect("same name, other list");

        assert!(insert_software(&connection, "nes", "tetris", None).is_err());
        assert!(insert_software(&connection, "missing_list", "x", None).is_err());
    }

    #[test]
    fn enforces_deferred_clone_references_within_the_same_list() {
        let mut connection = open();
        migrate(&mut connection).expect("migrate");
        connection
            .execute_batch(
                "INSERT INTO software_lists (name) VALUES ('nes');
                 INSERT INTO software_lists (name) VALUES ('snes');",
            )
            .expect("lists");

        // A clone listed before its parent is accepted at commit.
        let transaction = connection.transaction().expect("begin");
        insert_software(&transaction, "nes", "smb_alt", Some("smb")).expect("clone first");
        insert_software(&transaction, "nes", "smb", None).expect("parent later");
        transaction.commit().expect("valid clone reference");

        // A parent that only exists in another list is a dangling reference.
        let transaction = connection.transaction().expect("begin");
        insert_software(&transaction, "snes", "smb_alt", Some("smb")).expect("deferred");
        assert!(transaction.commit().is_err());
    }
}
