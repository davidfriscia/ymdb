//! Local projection of the global support file classifications.
//!
//! Each catalog has a projection file, `classifications.sqlite`, next to
//! `catalog.sqlite`. It holds the classifications of the active support file
//! sources, restricted to the machines of that catalog (matched by short
//! name), in a shape the search can join directly. It is derived data:
//!
//! - It never goes into `catalog.sqlite`, which only holds data derived from
//!   MAME and is replaced by imports.
//! - `projection_state` records, per dimension, the repository identity and
//!   revision, the active source and the catalog import it was built from.
//!   A dimension is rebuilt when any of them differs: after an import,
//!   reimport, removal or change of the active source in the repository, and
//!   after a catalog reimport even when the repository did not change.
//! - Rebuilding runs in one transaction with set-based `INSERT ... SELECT`
//!   statements over the attached repository and catalog; the state is
//!   updated in the same transaction, so a failure keeps the previous
//!   projection and its state, and the next access tries again.
//!
//! Connections are opened by [`super::store::CatalogStore`] only, under its
//! file lock. Searches read the projection through the catalog's read
//! connection, where it is attached read-only as `cls`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use serde::Serialize;

use super::store::StoreError;
use crate::support_files::model::{Dimension, DimensionKind, SupportFileFormat};
use crate::support_files::repository::{BUSY_TIMEOUT, RepositorySnapshot};

pub const CLASSIFICATIONS_FILE_NAME: &str = "classifications.sqlite";
/// Schema name of the projection on catalog read connections.
pub const SCHEMA: &str = "cls";

const MIGRATIONS: &[&str] = &[MIGRATION_1];
const SCHEMA_VERSION: i64 = MIGRATIONS.len() as i64;

/// - `memberships` has one row per machine and node: for hierarchical
///   dimensions a machine is listed under its category and every ancestor
///   (`direct = 0`), so that selecting a parent selects its descendants with
///   an equality lookup.
/// - `attributes` has at most one value per machine and dimension, with the
///   semantic sort key of versions.
const MIGRATION_1: &str = "
CREATE TABLE projection_state (
    dimension TEXT PRIMARY KEY,
    repository_id TEXT NOT NULL,
    revision INTEGER NOT NULL,
    source TEXT,
    catalog_token TEXT NOT NULL,
    machine_count INTEGER NOT NULL CHECK (machine_count >= 0),
    value_count INTEGER NOT NULL CHECK (value_count >= 0)
) STRICT;

CREATE TABLE memberships (
    dimension TEXT NOT NULL,
    value TEXT NOT NULL,
    machine_name TEXT NOT NULL,
    direct INTEGER NOT NULL CHECK (direct IN (0, 1)),
    PRIMARY KEY (dimension, value, machine_name)
) STRICT, WITHOUT ROWID;

CREATE INDEX memberships_machine ON memberships (dimension, machine_name);

CREATE TABLE attributes (
    dimension TEXT NOT NULL,
    machine_name TEXT NOT NULL,
    value TEXT NOT NULL,
    sort_key INTEGER,
    PRIMARY KEY (dimension, machine_name)
) STRICT, WITHOUT ROWID;

CREATE INDEX attributes_value ON attributes (dimension, value);
CREATE INDEX attributes_sort_key ON attributes (dimension, sort_key);
";

/// What the projection of every dimension should reflect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionTarget {
    /// The global repository file, attached while rebuilding.
    pub repository_path: PathBuf,
    pub repository_id: String,
    pub dimensions: Vec<DimensionTarget>,
}

impl ProjectionTarget {
    /// The target of the repository at `repository_path`, as of `snapshot`.
    pub fn new(repository_path: &Path, snapshot: &RepositorySnapshot) -> Self {
        Self {
            repository_path: repository_path.to_path_buf(),
            repository_id: snapshot.repository_id.clone(),
            dimensions: snapshot
                .dimensions
                .iter()
                .map(|entry| DimensionTarget {
                    dimension: entry.dimension,
                    revision: entry.revision,
                    source: entry.source,
                })
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DimensionTarget {
    pub dimension: Dimension,
    pub revision: i64,
    /// `None` when the dimension has no active source: it is emptied.
    pub source: Option<SupportFileFormat>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncOutcome {
    /// The installation has no catalog.
    NoCatalog,
    UpToDate,
    /// These dimensions were rebuilt.
    Synced(Vec<Dimension>),
}

/// A dimension available in a catalog's projection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassifiedDimension {
    pub dimension: Dimension,
    pub source: SupportFileFormat,
    /// Machines of this catalog with a value.
    pub machine_count: u64,
    /// Values used by machines of this catalog.
    pub value_count: u64,
}

/// The classifications of one machine, from the active sources.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MachineClassifications {
    pub genres: Vec<String>,
    /// Category paths, levels joined with ` / `.
    pub categories: Vec<String>,
    pub series: Vec<String>,
    pub version_added: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StoredState {
    repository_id: String,
    revision: i64,
    source: Option<String>,
    catalog_token: String,
}

/// Creates or upgrades the projection file and returns a writable connection.
pub(super) fn open_writer(path: &Path) -> Result<Connection, StoreError> {
    let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
        | OpenFlags::SQLITE_OPEN_CREATE
        | OpenFlags::SQLITE_OPEN_URI
        | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let mut connection = Connection::open_with_flags(path, flags)?;
    connection.busy_timeout(BUSY_TIMEOUT)?;
    let found: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if found > SCHEMA_VERSION {
        return Err(StoreError::Integrity(format!(
            "the classifications file has schema version {found}, which this version of YMDB \
             does not support"
        )));
    }
    for (index, sql) in MIGRATIONS
        .iter()
        .enumerate()
        .skip(usize::try_from(found).unwrap_or(0))
    {
        let transaction = connection.transaction()?;
        transaction.execute_batch(sql)?;
        transaction.pragma_update(None, "user_version", i64::try_from(index + 1).unwrap_or(0))?;
        transaction.commit()?;
    }
    Ok(connection)
}

/// Attaches the projection read-only to a catalog read connection.
pub(super) fn attach(reader: &Connection, path: &Path) -> Result<(), StoreError> {
    reader.execute(
        &format!("ATTACH DATABASE ?1 AS {SCHEMA}"),
        [sqlite_uri(path, "ro")?],
    )?;
    Ok(())
}

pub(super) fn is_attached(connection: &Connection) -> rusqlite::Result<bool> {
    let mut statement = connection.prepare("PRAGMA database_list")?;
    let names = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(names.iter().any(|name| name == SCHEMA))
}

/// Identifies the machine import of a catalog: a reimport changes it.
pub(super) fn catalog_token(reader: &Connection) -> rusqlite::Result<String> {
    reader.query_row(
        "SELECT imported_at || ':' || machine_count FROM main.catalog_metadata WHERE id = 1",
        [],
        |row| row.get(0),
    )
}

/// Dimensions whose projection does not match `target` and `token`.
pub(super) fn pending(
    reader: &Connection,
    target: &ProjectionTarget,
    token: &str,
) -> Vec<Dimension> {
    // A projection that cannot be read is rebuilt.
    let stored = stored_states(reader).unwrap_or_default();
    target
        .dimensions
        .iter()
        .filter(|wanted| {
            stored.get(&wanted.dimension)
                != Some(&StoredState {
                    repository_id: target.repository_id.clone(),
                    revision: wanted.revision,
                    source: wanted.source.map(|format| format.as_str().to_owned()),
                    catalog_token: token.to_owned(),
                })
        })
        .map(|wanted| wanted.dimension)
        .collect()
}

fn stored_states(reader: &Connection) -> rusqlite::Result<HashMap<Dimension, StoredState>> {
    let mut statement = reader.prepare_cached(&format!(
        "SELECT dimension, repository_id, revision, source, catalog_token
         FROM {SCHEMA}.projection_state"
    ))?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                StoredState {
                    repository_id: row.get(1)?,
                    revision: row.get(2)?,
                    source: row.get(3)?,
                    catalog_token: row.get(4)?,
                },
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows
        .into_iter()
        .filter_map(|(dimension, state)| Some((Dimension::from_id(&dimension)?, state)))
        .collect())
}

/// Rebuilds the projection of `dimensions` in one transaction.
pub(super) fn rebuild(
    projection: &Path,
    catalog: &Path,
    target: &ProjectionTarget,
    token: &str,
    dimensions: &[Dimension],
) -> Result<(), StoreError> {
    let mut connection = open_writer(projection)?;
    connection.execute(
        "ATTACH DATABASE ?1 AS catalog",
        [sqlite_uri(catalog, "ro")?],
    )?;
    let wanted: Vec<&DimensionTarget> = target
        .dimensions
        .iter()
        .filter(|entry| dimensions.contains(&entry.dimension))
        .collect();
    if wanted.iter().any(|entry| entry.source.is_some()) {
        connection.execute(
            "ATTACH DATABASE ?1 AS repo",
            [sqlite_uri(&target.repository_path, "ro")?],
        )?;
    }
    // Deferred: the attached catalog and repository are only read.
    let transaction = connection.transaction()?;
    for entry in wanted {
        rebuild_dimension(&transaction, entry)?;
        let (machine_count, value_count): (i64, i64) = transaction.query_row(
            match entry.dimension.kind() {
                DimensionKind::Membership => {
                    "SELECT COUNT(DISTINCT machine_name), COUNT(DISTINCT value)
                     FROM memberships WHERE dimension = ?1 AND direct = 1"
                }
                DimensionKind::Attribute => {
                    "SELECT COUNT(*), COUNT(DISTINCT value) FROM attributes WHERE dimension = ?1"
                }
            },
            [entry.dimension.as_str()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        transaction.execute(
            "INSERT OR REPLACE INTO projection_state (
                dimension, repository_id, revision, source, catalog_token, machine_count,
                value_count
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                entry.dimension.as_str(),
                target.repository_id,
                entry.revision,
                entry.source.map(SupportFileFormat::as_str),
                token,
                machine_count,
                value_count
            ],
        )?;
    }
    transaction.commit()?;
    Ok(())
}

fn rebuild_dimension(
    transaction: &rusqlite::Transaction<'_>,
    entry: &DimensionTarget,
) -> rusqlite::Result<()> {
    let dimension = entry.dimension.as_str();
    let (delete, insert) = match entry.dimension.kind() {
        DimensionKind::Membership => (
            "DELETE FROM memberships WHERE dimension = ?1",
            // A machine listed under several values sharing an ancestor gets
            // one row per node.
            "INSERT INTO memberships (dimension, value, machine_name, direct)
             SELECT ?1, n.node, m.machine_name, MAX(n.leaf)
             FROM repo.memberships m
             JOIN repo.value_nodes n
               ON n.format = m.format AND n.dimension = m.dimension AND n.value = m.value
             WHERE m.format = ?2 AND m.dimension = ?1
               -- `+`: check each row against the catalog instead of driving one
               -- lookup per catalog machine and node.
               AND +m.machine_name IN (SELECT name FROM catalog.machines)
             GROUP BY n.node, m.machine_name",
        ),
        DimensionKind::Attribute => (
            "DELETE FROM attributes WHERE dimension = ?1",
            "INSERT INTO attributes (dimension, machine_name, value, sort_key)
             SELECT ?1, a.machine_name, a.value, a.sort_key
             FROM repo.attributes a
             WHERE a.format = ?2 AND a.dimension = ?1
               AND a.machine_name IN (SELECT name FROM catalog.machines)",
        ),
    };
    transaction.execute(delete, [dimension])?;
    if let Some(source) = entry.source {
        transaction.execute(insert, params![dimension, source.as_str()])?;
    }
    Ok(())
}

/// The dimensions available in the projection attached to `reader`.
pub fn classified_dimensions(reader: &Connection) -> rusqlite::Result<Vec<ClassifiedDimension>> {
    if !is_attached(reader)? {
        return Ok(Vec::new());
    }
    let mut statement = reader.prepare_cached(&format!(
        "SELECT dimension, source, machine_count, value_count
         FROM {SCHEMA}.projection_state WHERE source IS NOT NULL"
    ))?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut dimensions: Vec<ClassifiedDimension> = rows
        .into_iter()
        .filter_map(|(dimension, source, machine_count, value_count)| {
            Some(ClassifiedDimension {
                dimension: Dimension::from_id(&dimension)?,
                source: SupportFileFormat::from_id(&source)?,
                machine_count: u64::try_from(machine_count).unwrap_or(0),
                value_count: u64::try_from(value_count).unwrap_or(0),
            })
        })
        .collect();
    dimensions.sort_by_key(|entry| entry.dimension);
    Ok(dimensions)
}

/// The `values` of `dimension` that no machine of this catalog has (every
/// value without a projection). Filters on them match nothing.
pub fn missing_values(
    reader: &Connection,
    dimension: Dimension,
    values: &[String],
) -> rusqlite::Result<Vec<String>> {
    if values.is_empty() {
        return Ok(Vec::new());
    }
    if !is_attached(reader)? {
        return Ok(values.to_vec());
    }
    let table = match dimension.kind() {
        DimensionKind::Membership => "memberships",
        DimensionKind::Attribute => "attributes",
    };
    let json = serde_json::to_string(values).unwrap_or_else(|_| "[]".to_owned());
    let mut statement = reader.prepare_cached(&format!(
        "SELECT j.value FROM json_each(?1) j
         WHERE j.value NOT IN (SELECT value FROM {SCHEMA}.{table} WHERE dimension = ?2)
         ORDER BY j.key"
    ))?;
    statement
        .query_map(params![json, dimension.as_str()], |row| row.get(0))?
        .collect()
}

/// The classifications of machine `name` (empty without a projection).
pub fn machine_classifications(
    reader: &Connection,
    name: &str,
) -> rusqlite::Result<MachineClassifications> {
    let mut classifications = MachineClassifications::default();
    if !is_attached(reader)? {
        return Ok(classifications);
    }
    let mut statement = reader.prepare_cached(&format!(
        "SELECT dimension, value FROM {SCHEMA}.memberships
         WHERE machine_name = ?1 AND direct = 1 AND dimension IN ('genre', 'category', 'series')
         ORDER BY dimension, value"
    ))?;
    let rows = statement
        .query_map([name], |row| Ok((row.get::<_, String>(0)?, row.get(1)?)))?
        .collect::<rusqlite::Result<Vec<(String, String)>>>()?;
    for (dimension, value) in rows {
        match Dimension::from_id(&dimension) {
            Some(Dimension::Genre) => classifications.genres.push(value),
            Some(Dimension::Category) => classifications.categories.push(value),
            Some(Dimension::Series) => classifications.series.push(value),
            Some(Dimension::VersionAdded) | None => {}
        }
    }
    classifications.version_added = reader
        .query_row(
            &format!(
                "SELECT value FROM {SCHEMA}.attributes
                 WHERE dimension = 'versionAdded' AND machine_name = ?1"
            ),
            [name],
            |row| row.get(0),
        )
        .optional()?;
    Ok(classifications)
}

/// A read-only `file:` URI for `path`, for `ATTACH`. Every character outside
/// the unreserved set is percent-encoded, so spaces, `?`, `#` and non-ASCII
/// characters in directory names are safe.
fn sqlite_uri(path: &Path, mode: &str) -> Result<String, StoreError> {
    let text = path.to_str().ok_or_else(|| {
        StoreError::Integrity(format!(
            "the path '{}' is not valid Unicode",
            path.display()
        ))
    })?;
    // URIs use `/`; on Unix `\` is an ordinary file name character.
    let text = if std::path::MAIN_SEPARATOR == '\\' {
        text.replace('\\', "/")
    } else {
        text.to_owned()
    };
    let mut encoded = String::with_capacity(text.len() + 16);
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~' | b'/' | b':') {
            encoded.push(char::from(byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    let prefix = if encoded.starts_with('/') {
        "file://"
    } else {
        // `C:/...` on Windows.
        "file:///"
    };
    Ok(format!("{prefix}{encoded}?mode={mode}"))
}

#[cfg(test)]
mod tests;
