//! Software and software list details.

use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

use super::{NameValue, RecordLink, to_u64};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SoftwareDetail {
    pub list_name: String,
    pub list_description: Option<String>,
    pub name: String,
    pub description: String,
    pub year: Option<String>,
    pub publisher: Option<String>,
    pub notes: Option<String>,
    /// Support status declared by MAME: `yes`, `partial` or `no`.
    pub supported: String,
    /// Parent software in the same list.
    pub parent: Option<RecordLink>,
    /// Number of clones; list them with the `cloneOf` and `lists` search filters.
    pub clone_count: u64,
    /// Number of machines referencing the list of this software.
    pub machine_count: u64,
    pub info: Vec<NameValue>,
    pub shared_features: Vec<NameValue>,
    pub parts: Vec<PartDetail>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PartDetail {
    pub name: String,
    pub interface: String,
    pub features: Vec<NameValue>,
    pub data_areas: Vec<DataAreaDetail>,
    pub disk_areas: Vec<DiskAreaDetail>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DataAreaDetail {
    pub name: String,
    pub size: i64,
    pub databits: i64,
    pub endian: String,
    pub roms: Vec<SoftwareRomDetail>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SoftwareRomDetail {
    /// `None` for loading instructions such as `reload` or `fill`.
    pub name: Option<String>,
    pub size: Option<i64>,
    pub length: Option<i64>,
    pub crc: Option<String>,
    pub sha1: Option<String>,
    pub offset: Option<String>,
    /// Fill value of `fill` instructions.
    pub value: Option<String>,
    pub status: String,
    pub load_flag: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskAreaDetail {
    pub name: String,
    pub disks: Vec<SoftwareDiskDetail>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SoftwareDiskDetail {
    pub name: String,
    pub sha1: Option<String>,
    pub status: String,
    pub is_writeable: bool,
}

/// A software list, or a list name referenced by machines but not provided
/// by MAME (`available` is then false).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SoftwareListDetail {
    pub name: String,
    pub description: Option<String>,
    pub notes: Option<String>,
    pub available: bool,
    pub software_count: u64,
    /// Machines referencing the list, by relation status.
    pub original_machine_count: u64,
    pub compatible_machine_count: u64,
}

pub fn software_detail(
    connection: &Connection,
    list: &str,
    name: &str,
) -> rusqlite::Result<Option<SoftwareDetail>> {
    let row = connection
        .prepare_cached(
            "SELECT s.id, s.list_name, l.description, s.name, s.description, s.year,
                    s.publisher, s.notes, s.supported, s.clone_of, parent.description,
                    (SELECT COUNT(*) FROM software c
                     WHERE c.list_name = s.list_name AND c.clone_of = s.name),
                    (SELECT COUNT(DISTINCT m.machine_name) FROM machine_software_lists m
                     WHERE m.list_name = s.list_name)
             FROM software s
             LEFT JOIN software_lists l ON l.name = s.list_name
             LEFT JOIN software parent
                    ON parent.list_name = s.list_name AND parent.name = s.clone_of
             WHERE s.list_name = ?1 AND s.name = ?2",
        )?
        .query_row(params![list, name], |row| {
            let parent_name: Option<String> = row.get(9)?;
            let parent_description: Option<String> = row.get(10)?;
            Ok((
                row.get::<_, i64>(0)?,
                SoftwareDetail {
                    list_name: row.get(1)?,
                    list_description: row.get(2)?,
                    name: row.get(3)?,
                    description: row.get(4)?,
                    year: row.get(5)?,
                    publisher: row.get(6)?,
                    notes: row.get(7)?,
                    supported: row.get(8)?,
                    parent: parent_name.map(|name| RecordLink {
                        name,
                        description: parent_description,
                    }),
                    clone_count: to_u64(row.get(11)?),
                    machine_count: to_u64(row.get(12)?),
                    info: Vec::new(),
                    shared_features: Vec::new(),
                    parts: Vec::new(),
                },
            ))
        })
        .optional()?;
    let Some((id, mut detail)) = row else {
        return Ok(None);
    };

    detail.info = name_values(
        connection,
        "SELECT name, value FROM software_info WHERE software_id = ?1 ORDER BY id",
        id,
    )?;
    detail.shared_features = name_values(
        connection,
        "SELECT name, value FROM software_shared_features WHERE software_id = ?1 ORDER BY id",
        id,
    )?;
    detail.parts = parts(connection, id)?;
    Ok(Some(detail))
}

pub fn software_list_detail(
    connection: &Connection,
    name: &str,
) -> rusqlite::Result<Option<SoftwareListDetail>> {
    let (original, compatible): (i64, i64) = connection
        .prepare_cached(
            "SELECT COUNT(DISTINCT CASE WHEN status = 'original' THEN machine_name END),
                    COUNT(DISTINCT CASE WHEN status <> 'original' THEN machine_name END)
             FROM machine_software_lists WHERE list_name = ?1",
        )?
        .query_row([name], |row| Ok((row.get(0)?, row.get(1)?)))?;
    let list = connection
        .prepare_cached(
            "SELECT description, notes, (SELECT COUNT(*) FROM software WHERE list_name = ?1)
             FROM software_lists WHERE name = ?1",
        )?
        .query_row([name], |row| {
            Ok((
                row.get::<_, Option<String>>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })
        .optional()?;
    if list.is_none() && original + compatible == 0 {
        return Ok(None);
    }
    let available = list.is_some();
    let (description, notes, software_count) = list.unwrap_or((None, None, 0));
    Ok(Some(SoftwareListDetail {
        name: name.to_owned(),
        description,
        notes,
        available,
        software_count: to_u64(software_count),
        original_machine_count: to_u64(original),
        compatible_machine_count: to_u64(compatible),
    }))
}

fn name_values(connection: &Connection, sql: &str, id: i64) -> rusqlite::Result<Vec<NameValue>> {
    connection
        .prepare_cached(sql)?
        .query_map([id], |row| {
            Ok(NameValue {
                name: row.get(0)?,
                value: row.get(1)?,
            })
        })?
        .collect()
}

fn parts(connection: &Connection, software_id: i64) -> rusqlite::Result<Vec<PartDetail>> {
    let parts: Vec<(i64, String, String)> = connection
        .prepare_cached(
            "SELECT id, name, interface FROM software_parts WHERE software_id = ?1 ORDER BY id",
        )?
        .query_map([software_id], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?
        .collect::<rusqlite::Result<_>>()?;
    parts
        .into_iter()
        .map(|(part_id, name, interface)| {
            Ok(PartDetail {
                name,
                interface,
                features: name_values(
                    connection,
                    "SELECT name, value FROM software_part_features WHERE part_id = ?1 ORDER BY id",
                    part_id,
                )?,
                data_areas: data_areas(connection, part_id)?,
                disk_areas: disk_areas(connection, part_id)?,
            })
        })
        .collect()
}

fn data_areas(connection: &Connection, part_id: i64) -> rusqlite::Result<Vec<DataAreaDetail>> {
    let areas: Vec<(i64, DataAreaDetail)> = connection
        .prepare_cached(
            "SELECT id, name, size, databits, endian FROM software_data_areas
             WHERE part_id = ?1 ORDER BY id",
        )?
        .query_map([part_id], |row| {
            Ok((
                row.get(0)?,
                DataAreaDetail {
                    name: row.get(1)?,
                    size: row.get(2)?,
                    databits: row.get(3)?,
                    endian: row.get(4)?,
                    roms: Vec::new(),
                },
            ))
        })?
        .collect::<rusqlite::Result<_>>()?;
    let mut roms = connection.prepare_cached(
        "SELECT name, size, length, crc, sha1, offset, value, status, load_flag FROM software_roms
         WHERE data_area_id = ?1 ORDER BY id",
    )?;
    areas
        .into_iter()
        .map(|(id, mut area)| {
            area.roms = roms
                .query_map([id], |row| {
                    Ok(SoftwareRomDetail {
                        name: row.get(0)?,
                        size: row.get(1)?,
                        length: row.get(2)?,
                        crc: row.get(3)?,
                        sha1: row.get(4)?,
                        offset: row.get(5)?,
                        value: row.get(6)?,
                        status: row.get(7)?,
                        load_flag: row.get(8)?,
                    })
                })?
                .collect::<rusqlite::Result<_>>()?;
            Ok(area)
        })
        .collect()
}

fn disk_areas(connection: &Connection, part_id: i64) -> rusqlite::Result<Vec<DiskAreaDetail>> {
    let areas: Vec<(i64, String)> = connection
        .prepare_cached("SELECT id, name FROM software_disk_areas WHERE part_id = ?1 ORDER BY id")?
        .query_map([part_id], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let mut disks = connection.prepare_cached(
        "SELECT name, sha1, status, is_writeable FROM software_disks
         WHERE disk_area_id = ?1 ORDER BY id",
    )?;
    areas
        .into_iter()
        .map(|(id, name)| {
            let disks = disks
                .query_map([id], |row| {
                    Ok(SoftwareDiskDetail {
                        name: row.get(0)?,
                        sha1: row.get(1)?,
                        status: row.get(2)?,
                        is_writeable: row.get(3)?,
                    })
                })?
                .collect::<rusqlite::Result<_>>()?;
            Ok(DiskAreaDetail { name, disks })
        })
        .collect()
}
