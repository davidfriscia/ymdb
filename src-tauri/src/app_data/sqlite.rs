//! Consistent copies and checks of SQLite databases.

use std::path::Path;

use rusqlite::{Connection, MAIN_DB, OpenFlags};

/// Copies the main database of `source` to `destination` with the SQLite
/// backup API, which reads one consistent state (including pages still in a
/// WAL file) even while other connections use the database. The copy uses a
/// rollback journal, so it is a single self-contained file, and is checked.
pub fn backup(source: &Connection, destination: &Path) -> Result<(), String> {
    source
        .backup(MAIN_DB, destination, None)
        .map_err(|error| format!("database copy failed: {error}"))?;
    let copy = Connection::open(destination).map_err(|error| error.to_string())?;
    copy.pragma_update_and_check(None, "journal_mode", "DELETE", |row| {
        row.get::<_, String>(0)
    })
    .map_err(|error| error.to_string())?;
    quick_check(&copy)?;
    copy.close().map_err(|(_, error)| error.to_string())
}

/// Opens a database file read-only, checks its structure and returns its
/// `user_version` (the schema version of YMDB databases).
pub fn check_file(path: &Path) -> Result<i64, String> {
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|error| error.to_string())?;
    quick_check(&connection)?;
    connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|error| error.to_string())
}

fn quick_check(connection: &Connection) -> Result<(), String> {
    let result: String = connection
        .query_row("PRAGMA quick_check(1)", [], |row| row.get(0))
        .map_err(|error| error.to_string())?;
    if result == "ok" {
        Ok(())
    } else {
        Err(format!("the database is damaged: {result}"))
    }
}
