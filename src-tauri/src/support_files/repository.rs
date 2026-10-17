//! The global support files repository (`support-files.sqlite`).
//!
//! It is the authoritative store of the imported classifications, shared by
//! every MAME installation: one source per format (importing a format again
//! replaces its source), the active source of each dimension and one
//! revision per dimension. Catalogs keep a projection of the active data (see
//! `catalog::classification`) and compare these revisions to know when to
//! rebuild it.
//!
//! Rules:
//!
//! - Every change runs in one transaction; a failed import leaves the
//!   previous source untouched.
//! - A revision is incremented only when the effective content of its
//!   dimension changes: the active source is set, cleared or replaced by
//!   different data. Reimporting identical data changes nothing.
//! - The first source imported for a dimension becomes its active source.
//!   Removing the active source leaves the dimension without one; another
//!   source is never activated implicitly. Inactive sources are kept.
//! - `repository_id` is generated when the file is created, so that a
//!   recreated repository never matches the revisions of an older one.
//! - Machines are referenced by short name only and are not checked against
//!   any catalog: machines missing from a catalog stay here.

use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use serde::Serialize;

use super::model::{
    Dimension, DimensionKind, ParseWarning, ParsedSupportFile, SupportFileFormat, value_nodes,
};
use super::version;

pub const REPOSITORY_FILE_NAME: &str = "support-files.sqlite";

/// How long a connection waits for another one holding a lock.
pub const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

const MIGRATIONS: &[&str] = &[MIGRATION_1];
pub const SCHEMA_VERSION: i64 = MIGRATIONS.len() as i64;

/// Version 1 of the repository schema.
///
/// - `memberships` and `attributes` hold the normalized data of each source.
/// - `value_nodes` lists, for each membership value, the nodes it belongs to
///   (itself and its ancestors for hierarchical dimensions); `leaf` marks the
///   node of the value itself.
/// - `active_sources` references `source_dimensions`, so a source cannot be
///   removed while it is active without clearing the dimension explicitly.
const MIGRATION_1: &str = "
CREATE TABLE repository (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    repository_id TEXT NOT NULL
) STRICT;

CREATE TABLE sources (
    format TEXT PRIMARY KEY,
    file_name TEXT NOT NULL,
    declared_version TEXT,
    imported_at INTEGER NOT NULL,
    machine_count INTEGER NOT NULL CHECK (machine_count >= 0),
    warnings TEXT NOT NULL
) STRICT;

CREATE TABLE source_dimensions (
    format TEXT NOT NULL REFERENCES sources (format) ON DELETE CASCADE,
    dimension TEXT NOT NULL,
    machine_count INTEGER NOT NULL CHECK (machine_count >= 0),
    value_count INTEGER NOT NULL CHECK (value_count >= 0),
    PRIMARY KEY (format, dimension)
) STRICT;

CREATE TABLE memberships (
    format TEXT NOT NULL,
    dimension TEXT NOT NULL,
    value TEXT NOT NULL,
    machine_name TEXT NOT NULL,
    PRIMARY KEY (format, dimension, value, machine_name)
) STRICT, WITHOUT ROWID;

CREATE TABLE value_nodes (
    format TEXT NOT NULL,
    dimension TEXT NOT NULL,
    value TEXT NOT NULL,
    node TEXT NOT NULL,
    leaf INTEGER NOT NULL CHECK (leaf IN (0, 1)),
    PRIMARY KEY (format, dimension, value, node)
) STRICT, WITHOUT ROWID;

CREATE TABLE attributes (
    format TEXT NOT NULL,
    dimension TEXT NOT NULL,
    machine_name TEXT NOT NULL,
    value TEXT NOT NULL,
    sort_key INTEGER,
    PRIMARY KEY (format, dimension, machine_name)
) STRICT, WITHOUT ROWID;

CREATE TABLE active_sources (
    dimension TEXT PRIMARY KEY,
    format TEXT NOT NULL,
    FOREIGN KEY (format, dimension) REFERENCES source_dimensions (format, dimension)
        DEFERRABLE INITIALLY DEFERRED
) STRICT;

CREATE TABLE dimension_revisions (
    dimension TEXT PRIMARY KEY,
    revision INTEGER NOT NULL CHECK (revision >= 0)
) STRICT;
";

#[derive(Debug)]
pub enum RepositoryError {
    Sqlite(rusqlite::Error),
    Io { path: PathBuf, source: io::Error },
    UnsupportedSchema { found: i64 },
    NotFound(String),
    Invalid(String),
}

impl fmt::Display for RepositoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sqlite(error) => write!(f, "support files database error: {error}"),
            Self::Io { path, source } => write!(f, "file error on '{}': {source}", path.display()),
            Self::UnsupportedSchema { found } => write!(
                f,
                "the support files database has schema version {found}, which this version \
                 of YMDB does not support (expected {SCHEMA_VERSION})"
            ),
            Self::NotFound(message) | Self::Invalid(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for RepositoryError {}

impl From<rusqlite::Error> for RepositoryError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Sqlite(error)
    }
}

/// The revision and active source of every dimension, as projections need them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositorySnapshot {
    pub repository_id: String,
    /// One entry per [`Dimension`], in [`Dimension::ALL`] order.
    pub dimensions: Vec<DimensionRevision>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DimensionRevision {
    pub dimension: Dimension,
    pub revision: i64,
    pub source: Option<SupportFileFormat>,
}

/// The repository as shown in Settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SupportFilesView {
    /// In [`SupportFileFormat::ALL`] order.
    pub sources: Vec<SourceView>,
    /// In [`Dimension::ALL`] order.
    pub dimensions: Vec<DimensionView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceView {
    pub format: SupportFileFormat,
    /// Name of the imported file; the file itself is not kept or modified.
    pub file_name: String,
    pub declared_version: Option<String>,
    /// Milliseconds since the Unix epoch.
    pub imported_at: u64,
    /// Distinct machines over every dimension of the source.
    pub machine_count: u64,
    pub dimensions: Vec<SourceDimensionView>,
    pub warnings: Vec<ParseWarning>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceDimensionView {
    pub dimension: Dimension,
    pub machine_count: u64,
    /// Categories, genres, series or versions.
    pub value_count: u64,
    /// Whether this source is the active source of the dimension.
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DimensionView {
    pub dimension: Dimension,
    pub active_source: Option<SupportFileFormat>,
    /// Imported sources providing the dimension.
    pub available_sources: Vec<SupportFileFormat>,
    pub revision: i64,
}

/// What an import changed.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ImportOutcome {
    /// Dimensions whose effective content changed (their revision was incremented).
    pub changed: Vec<Dimension>,
    /// Dimensions for which the imported source became the active source.
    pub activated: Vec<Dimension>,
}

#[derive(Debug, Clone)]
pub struct SupportRepository {
    path: PathBuf,
}

impl SupportRepository {
    pub fn in_dir(dir: &Path) -> Self {
        Self {
            path: dir.join(REPOSITORY_FILE_NAME),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Opens the repository, creating and upgrading it as needed.
    fn open(&self) -> Result<Connection, RepositoryError> {
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir).map_err(|source| RepositoryError::Io {
                path: dir.to_path_buf(),
                source,
            })?;
        }
        let mut connection = Connection::open(&self.path)?;
        connection.busy_timeout(BUSY_TIMEOUT)?;
        connection.execute_batch("PRAGMA foreign_keys = ON")?;
        migrate(&mut connection)?;
        connection.execute(
            "INSERT OR IGNORE INTO repository (id, repository_id) VALUES (1, ?1)",
            [uuid::Uuid::new_v4().simple().to_string()],
        )?;
        Ok(connection)
    }

    pub fn snapshot(&self) -> Result<RepositorySnapshot, RepositoryError> {
        let connection = self.open()?;
        let repository_id: String = connection.query_row(
            "SELECT repository_id FROM repository WHERE id = 1",
            [],
            |row| row.get(0),
        )?;
        let dimensions = Dimension::ALL
            .into_iter()
            .map(|dimension| {
                Ok(DimensionRevision {
                    dimension,
                    revision: revision(&connection, dimension)?,
                    source: active_source(&connection, dimension)?,
                })
            })
            .collect::<Result<_, RepositoryError>>()?;
        Ok(RepositorySnapshot {
            repository_id,
            dimensions,
        })
    }

    pub fn view(&self) -> Result<SupportFilesView, RepositoryError> {
        let connection = self.open()?;
        let mut sources = Vec::new();
        for format in SupportFileFormat::ALL {
            if let Some(source) = source_view(&connection, format)? {
                sources.push(source);
            }
        }
        let dimensions = Dimension::ALL
            .into_iter()
            .map(|dimension| {
                Ok(DimensionView {
                    dimension,
                    active_source: active_source(&connection, dimension)?,
                    available_sources: sources
                        .iter()
                        .filter(|source| {
                            source
                                .dimensions
                                .iter()
                                .any(|entry| entry.dimension == dimension)
                        })
                        .map(|source| source.format)
                        .collect(),
                    revision: revision(&connection, dimension)?,
                })
            })
            .collect::<Result<_, RepositoryError>>()?;
        Ok(SupportFilesView {
            sources,
            dimensions,
        })
    }

    /// Stores `parsed` as the source of its format, replacing the previous
    /// source of that format atomically.
    pub fn import(
        &self,
        file_name: &str,
        parsed: &ParsedSupportFile,
        imported_at: u64,
    ) -> Result<ImportOutcome, RepositoryError> {
        let mut connection = self.open()?;
        let format = parsed.format;
        let statistics = parsed.statistics();
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        stage(&transaction, parsed)?;

        let mut outcome = ImportOutcome::default();
        for &dimension in format.dimensions() {
            let provided = statistics.iter().any(|entry| entry.dimension == dimension);
            let active = active_source(&transaction, dimension)?;
            let changed = if provided && active.is_none() {
                set_active_row(&transaction, dimension, Some(format))?;
                outcome.activated.push(dimension);
                true
            } else if active == Some(format) && !provided {
                // The new file no longer feeds a dimension it was active for.
                set_active_row(&transaction, dimension, None)?;
                true
            } else {
                active == Some(format) && staged_differs(&transaction, format, dimension)?
            };
            if changed {
                bump_revision(&transaction, dimension)?;
                outcome.changed.push(dimension);
            }
        }

        delete_rows(&transaction, format)?;
        transaction.execute(
            "INSERT INTO memberships (format, dimension, value, machine_name)
             SELECT ?1, dimension, value, machine_name FROM temp.staged_memberships",
            [format.as_str()],
        )?;
        transaction.execute(
            "INSERT INTO value_nodes (format, dimension, value, node, leaf)
             SELECT ?1, dimension, value, node, leaf FROM temp.staged_nodes",
            [format.as_str()],
        )?;
        transaction.execute(
            "INSERT INTO attributes (format, dimension, machine_name, value, sort_key)
             SELECT ?1, dimension, machine_name, value, sort_key FROM temp.staged_attributes",
            [format.as_str()],
        )?;
        let warnings = serde_json::to_string(&parsed.warnings)
            .map_err(|error| RepositoryError::Invalid(error.to_string()))?;
        transaction.execute(
            "INSERT OR REPLACE INTO sources (
                format, file_name, declared_version, imported_at, machine_count, warnings
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                format.as_str(),
                file_name,
                parsed.declared_version,
                to_i64(imported_at),
                to_i64(parsed.machine_count()),
                warnings
            ],
        )?;
        transaction.execute(
            "DELETE FROM source_dimensions WHERE format = ?1",
            [format.as_str()],
        )?;
        for entry in &statistics {
            transaction.execute(
                "INSERT INTO source_dimensions (format, dimension, machine_count, value_count)
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    format.as_str(),
                    entry.dimension.as_str(),
                    to_i64(entry.machine_count),
                    to_i64(entry.value_count)
                ],
            )?;
        }
        transaction.commit()?;
        Ok(outcome)
    }

    /// Removes the source of `format` from the repository (never the file it
    /// was imported from). Dimensions it was active for are left without an
    /// active source. Returns the dimensions whose revision changed.
    pub fn remove(&self, format: SupportFileFormat) -> Result<Vec<Dimension>, RepositoryError> {
        let mut connection = self.open()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if source_view(&transaction, format)?.is_none() {
            return Err(RepositoryError::NotFound(format!(
                "no {format} has been imported"
            )));
        }
        let mut changed = Vec::new();
        for &dimension in format.dimensions() {
            if active_source(&transaction, dimension)? == Some(format) {
                set_active_row(&transaction, dimension, None)?;
                bump_revision(&transaction, dimension)?;
                changed.push(dimension);
            }
        }
        delete_rows(&transaction, format)?;
        transaction.execute("DELETE FROM sources WHERE format = ?1", [format.as_str()])?;
        transaction.commit()?;
        Ok(changed)
    }

    /// Sets (or clears, with `None`) the active source of `dimension`.
    /// Returns whether it changed.
    pub fn set_active(
        &self,
        dimension: Dimension,
        format: Option<SupportFileFormat>,
    ) -> Result<bool, RepositoryError> {
        let mut connection = self.open()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(format) = format {
            let provides: bool = transaction.query_row(
                "SELECT EXISTS (
                    SELECT 1 FROM source_dimensions WHERE format = ?1 AND dimension = ?2
                 )",
                params![format.as_str(), dimension.as_str()],
                |row| row.get(0),
            )?;
            if !provides {
                return Err(RepositoryError::Invalid(format!(
                    "{format} has not been imported or does not provide {}",
                    dimension.as_str()
                )));
            }
        }
        if active_source(&transaction, dimension)? == format {
            return Ok(false);
        }
        set_active_row(&transaction, dimension, format)?;
        bump_revision(&transaction, dimension)?;
        transaction.commit()?;
        Ok(true)
    }
}

fn migrate(connection: &mut Connection) -> Result<(), RepositoryError> {
    let current: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if current > SCHEMA_VERSION {
        return Err(RepositoryError::UnsupportedSchema { found: current });
    }
    for (index, sql) in MIGRATIONS
        .iter()
        .enumerate()
        .skip(usize::try_from(current).unwrap_or(0))
    {
        let transaction = connection.transaction()?;
        transaction.execute_batch(sql)?;
        transaction.pragma_update(None, "user_version", to_i64(index + 1))?;
        transaction.commit()?;
    }
    Ok(())
}

/// Copies the parsed rows into temporary tables, in a few set-based
/// statements: each kind of row is bound as one JSON array.
fn stage(transaction: &Transaction<'_>, parsed: &ParsedSupportFile) -> Result<(), RepositoryError> {
    transaction.execute_batch(
        "CREATE TEMP TABLE IF NOT EXISTS staged_memberships (
            dimension TEXT NOT NULL, value TEXT NOT NULL, machine_name TEXT NOT NULL,
            PRIMARY KEY (dimension, value, machine_name)
         ) WITHOUT ROWID;
         CREATE TEMP TABLE IF NOT EXISTS staged_nodes (
            dimension TEXT NOT NULL, value TEXT NOT NULL, node TEXT NOT NULL, leaf INTEGER NOT NULL,
            PRIMARY KEY (dimension, value, node)
         ) WITHOUT ROWID;
         CREATE TEMP TABLE IF NOT EXISTS staged_attributes (
            dimension TEXT NOT NULL, machine_name TEXT NOT NULL, value TEXT NOT NULL,
            sort_key INTEGER, PRIMARY KEY (dimension, machine_name)
         ) WITHOUT ROWID;
         DELETE FROM temp.staged_memberships;
         DELETE FROM temp.staged_nodes;
         DELETE FROM temp.staged_attributes;",
    )?;
    let memberships: Vec<[&str; 3]> = parsed
        .memberships
        .iter()
        .map(|entry| {
            [
                entry.dimension.as_str(),
                entry.value.as_str(),
                entry.machine.as_str(),
            ]
        })
        .collect();
    transaction.execute(
        "INSERT OR IGNORE INTO temp.staged_memberships (dimension, value, machine_name)
         SELECT value ->> 0, value ->> 1, value ->> 2 FROM json_each(?1)",
        [json(&memberships)?],
    )?;
    let values: BTreeSet<(Dimension, &str)> = parsed
        .memberships
        .iter()
        .map(|entry| (entry.dimension, entry.value.as_str()))
        .collect();
    let mut nodes = Vec::new();
    for (dimension, value) in values {
        let paths = value_nodes(dimension, value);
        let last = paths.len().saturating_sub(1);
        for (index, node) in paths.into_iter().enumerate() {
            nodes.push(serde_json::json!([
                dimension.as_str(),
                value,
                node,
                i64::from(index == last)
            ]));
        }
    }
    transaction.execute(
        "INSERT OR IGNORE INTO temp.staged_nodes (dimension, value, node, leaf)
         SELECT value ->> 0, value ->> 1, value ->> 2, value ->> 3 FROM json_each(?1)",
        [json(&nodes)?],
    )?;
    let attributes: Vec<serde_json::Value> = parsed
        .attributes
        .iter()
        .map(|entry| {
            serde_json::json!([
                entry.dimension.as_str(),
                entry.machine,
                entry.value,
                version::sort_key(&entry.value)
            ])
        })
        .collect();
    transaction.execute(
        "INSERT OR IGNORE INTO temp.staged_attributes (dimension, machine_name, value, sort_key)
         SELECT value ->> 0, value ->> 1, value ->> 2, value ->> 3 FROM json_each(?1)",
        [json(&attributes)?],
    )?;
    Ok(())
}

/// Whether the staged rows of `dimension` differ from those stored for `format`.
fn staged_differs(
    transaction: &Transaction<'_>,
    format: SupportFileFormat,
    dimension: Dimension,
) -> Result<bool, RepositoryError> {
    let sql = match dimension.kind() {
        DimensionKind::Membership => {
            "SELECT EXISTS (
                SELECT value, machine_name FROM temp.staged_memberships WHERE dimension = ?2
                EXCEPT
                SELECT value, machine_name FROM memberships WHERE format = ?1 AND dimension = ?2
             ) OR EXISTS (
                SELECT value, machine_name FROM memberships WHERE format = ?1 AND dimension = ?2
                EXCEPT
                SELECT value, machine_name FROM temp.staged_memberships WHERE dimension = ?2
             )"
        }
        DimensionKind::Attribute => {
            "SELECT EXISTS (
                SELECT machine_name, value FROM temp.staged_attributes WHERE dimension = ?2
                EXCEPT
                SELECT machine_name, value FROM attributes WHERE format = ?1 AND dimension = ?2
             ) OR EXISTS (
                SELECT machine_name, value FROM attributes WHERE format = ?1 AND dimension = ?2
                EXCEPT
                SELECT machine_name, value FROM temp.staged_attributes WHERE dimension = ?2
             )"
        }
    };
    Ok(
        transaction.query_row(sql, params![format.as_str(), dimension.as_str()], |row| {
            row.get(0)
        })?,
    )
}

fn delete_rows(transaction: &Transaction<'_>, format: SupportFileFormat) -> rusqlite::Result<()> {
    for sql in [
        "DELETE FROM memberships WHERE format = ?1",
        "DELETE FROM value_nodes WHERE format = ?1",
        "DELETE FROM attributes WHERE format = ?1",
    ] {
        transaction.execute(sql, [format.as_str()])?;
    }
    Ok(())
}

fn source_view(
    connection: &Connection,
    format: SupportFileFormat,
) -> Result<Option<SourceView>, RepositoryError> {
    let row = connection
        .query_row(
            "SELECT file_name, declared_version, imported_at, machine_count, warnings
             FROM sources WHERE format = ?1",
            [format.as_str()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, String>(4)?,
                ))
            },
        )
        .optional()?;
    let Some((file_name, declared_version, imported_at, machine_count, warnings)) = row else {
        return Ok(None);
    };
    let mut statement = connection.prepare_cached(
        "SELECT d.dimension, d.machine_count, d.value_count, a.format IS NOT NULL
         FROM source_dimensions d
         LEFT JOIN active_sources a ON a.dimension = d.dimension AND a.format = d.format
         WHERE d.format = ?1",
    )?;
    let mut dimensions = statement
        .query_map([format.as_str()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, bool>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .filter_map(|(dimension, machine_count, value_count, active)| {
            Some(SourceDimensionView {
                dimension: Dimension::from_id(&dimension)?,
                machine_count: to_u64(machine_count),
                value_count: to_u64(value_count),
                active,
            })
        })
        .collect::<Vec<_>>();
    dimensions.sort_by_key(|entry| entry.dimension);
    Ok(Some(SourceView {
        format,
        file_name,
        declared_version,
        imported_at: to_u64(imported_at),
        machine_count: to_u64(machine_count),
        dimensions,
        // Warnings are informative: unreadable ones are not an error.
        warnings: serde_json::from_str(&warnings).unwrap_or_default(),
    }))
}

fn active_source(
    connection: &Connection,
    dimension: Dimension,
) -> Result<Option<SupportFileFormat>, RepositoryError> {
    let format: Option<String> = connection
        .query_row(
            "SELECT format FROM active_sources WHERE dimension = ?1",
            [dimension.as_str()],
            |row| row.get(0),
        )
        .optional()?;
    format
        .map(|format| {
            SupportFileFormat::from_id(&format).ok_or_else(|| {
                RepositoryError::Invalid(format!("unknown support file format '{format}'"))
            })
        })
        .transpose()
}

fn set_active_row(
    transaction: &Transaction<'_>,
    dimension: Dimension,
    format: Option<SupportFileFormat>,
) -> rusqlite::Result<()> {
    match format {
        Some(format) => transaction.execute(
            "INSERT OR REPLACE INTO active_sources (dimension, format) VALUES (?1, ?2)",
            params![dimension.as_str(), format.as_str()],
        )?,
        None => transaction.execute(
            "DELETE FROM active_sources WHERE dimension = ?1",
            [dimension.as_str()],
        )?,
    };
    Ok(())
}

fn revision(connection: &Connection, dimension: Dimension) -> rusqlite::Result<i64> {
    Ok(connection
        .query_row(
            "SELECT revision FROM dimension_revisions WHERE dimension = ?1",
            [dimension.as_str()],
            |row| row.get(0),
        )
        .optional()?
        .unwrap_or(0))
}

fn bump_revision(transaction: &Transaction<'_>, dimension: Dimension) -> rusqlite::Result<()> {
    transaction.execute(
        "INSERT INTO dimension_revisions (dimension, revision) VALUES (?1, 1)
         ON CONFLICT (dimension) DO UPDATE SET revision = revision + 1",
        [dimension.as_str()],
    )?;
    Ok(())
}

fn json(value: &impl Serialize) -> Result<String, RepositoryError> {
    serde_json::to_string(value).map_err(|error| RepositoryError::Invalid(error.to_string()))
}

fn to_i64<T: TryInto<i64>>(value: T) -> i64 {
    value.try_into().unwrap_or(i64::MAX)
}

fn to_u64(value: i64) -> u64 {
    u64::try_from(value).unwrap_or(0)
}
