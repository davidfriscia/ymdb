//! Machine detail.

use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

use super::{RecordLink, to_u64};

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MachineDetail {
    pub name: String,
    pub description: String,
    pub year: Option<String>,
    pub manufacturer: Option<String>,
    pub source_file: Option<String>,
    pub is_bios: bool,
    pub is_device: bool,
    pub is_mechanical: bool,
    pub is_runnable: bool,
    /// Parent machine of a clone (`cloneof`).
    pub parent: Option<RecordLink>,
    /// Machine whose ROMs are used (`romof`): the parent or a BIOS.
    pub rom_parent: Option<RecordLink>,
    /// Sample set (`sampleof`); not necessarily a machine.
    pub sample_of: Option<String>,
    /// Number of clones; list them with the `cloneOf` search filter.
    pub clone_count: u64,
    pub driver: Option<DriverDetail>,
    pub features: Vec<FeatureDetail>,
    pub sound_channels: Option<i64>,
    pub input: Option<InputDetail>,
    pub controls: Vec<ControlDetail>,
    pub displays: Vec<DisplayDetail>,
    pub chips: Vec<ChipDetail>,
    pub bios_sets: Vec<BiosSetDetail>,
    pub roms: Vec<RomDetail>,
    pub disks: Vec<DiskDetail>,
    pub devices: Vec<DeviceDetail>,
    pub media_devices: Vec<MediaDeviceDetail>,
    pub software_lists: Vec<MachineSoftwareList>,
    pub ram_options: Vec<RamOptionDetail>,
    pub samples: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriverDetail {
    pub status: Option<String>,
    pub emulation: Option<String>,
    pub cocktail: Option<String>,
    pub save_state: Option<String>,
    pub requires_artwork: bool,
    pub unofficial: bool,
    pub no_sound_hardware: bool,
    pub incomplete: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureDetail {
    pub feature_type: String,
    pub status: Option<String>,
    pub overall: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InputDetail {
    pub players: Option<i64>,
    pub coins: Option<i64>,
    pub service: bool,
    pub tilt: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlDetail {
    pub control_type: String,
    pub player: Option<i64>,
    pub buttons: Option<i64>,
    pub required_buttons: Option<i64>,
    pub ways: Option<String>,
    pub ways2: Option<String>,
    pub ways3: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayDetail {
    pub tag: Option<String>,
    pub display_type: String,
    pub rotate: Option<i64>,
    pub flip_x: bool,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub refresh: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChipDetail {
    pub chip_type: String,
    pub name: String,
    pub tag: Option<String>,
    pub clock: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BiosSetDetail {
    pub name: String,
    pub description: String,
    pub is_default: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RomDetail {
    pub name: String,
    pub bios: Option<String>,
    pub size: i64,
    pub crc: Option<String>,
    pub sha1: Option<String>,
    pub merge_name: Option<String>,
    pub region: Option<String>,
    pub offset: Option<String>,
    pub status: String,
    pub is_optional: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskDetail {
    pub name: String,
    pub sha1: Option<String>,
    pub merge_name: Option<String>,
    pub region: Option<String>,
    pub index: Option<i64>,
    pub is_writable: bool,
    pub status: String,
    pub is_optional: bool,
}

/// A device instantiated by the machine, linked to its catalog entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceDetail {
    pub device: RecordLink,
    pub instance_count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaDeviceDetail {
    pub device_type: String,
    pub tag: Option<String>,
    pub interface: Option<String>,
    pub mandatory: Option<String>,
    pub fixed_image: Option<String>,
    pub instance_name: Option<String>,
    pub instance_brief_name: Option<String>,
    pub extensions: Vec<String>,
}

/// A software list referenced by the machine. The reference is what MAME
/// declares; it does not guarantee that every software of the list works with
/// every configuration of the machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MachineSoftwareList {
    pub name: String,
    pub tag: Option<String>,
    /// `original` or `compatible`.
    pub status: String,
    pub filter: Option<String>,
    /// Description of the list; `None` when the list is not in the catalog.
    pub description: Option<String>,
    /// Whether the list exists in the imported software lists.
    pub available: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RamOptionDetail {
    pub name: Option<String>,
    pub size: i64,
    pub is_default: bool,
}

pub fn machine_detail(
    connection: &Connection,
    name: &str,
) -> rusqlite::Result<Option<MachineDetail>> {
    let row = connection
        .prepare_cached(
            "SELECT m.name, m.description, m.year, m.manufacturer, m.source_file,
                    m.is_bios, m.is_device, m.is_mechanical, m.is_runnable,
                    m.clone_of, parent.description, m.rom_of, rom_parent.description,
                    m.sample_of, m.sound_channels,
                    m.input_players, m.input_coins, m.input_service, m.input_tilt,
                    m.driver_status, m.driver_emulation, m.driver_cocktail,
                    m.driver_save_state, m.driver_requires_artwork, m.driver_unofficial,
                    m.driver_no_sound_hardware, m.driver_incomplete,
                    (SELECT COUNT(*) FROM machines c WHERE c.clone_of = m.name)
             FROM machines m
             LEFT JOIN machines parent ON parent.name = m.clone_of
             LEFT JOIN machines rom_parent ON rom_parent.name = m.rom_of
             WHERE m.name = ?1",
        )?
        .query_row([name], |row| {
            let link = |name: Option<String>, description: Option<String>| {
                name.map(|name| RecordLink { name, description })
            };
            let has_input = row.get::<_, Option<i64>>(15)?.is_some();
            let has_driver = row.get::<_, Option<String>>(19)?.is_some();
            Ok(MachineDetail {
                name: row.get(0)?,
                description: row.get(1)?,
                year: row.get(2)?,
                manufacturer: row.get(3)?,
                source_file: row.get(4)?,
                is_bios: row.get(5)?,
                is_device: row.get(6)?,
                is_mechanical: row.get(7)?,
                is_runnable: row.get(8)?,
                parent: link(row.get(9)?, row.get(10)?),
                rom_parent: link(row.get(11)?, row.get(12)?),
                sample_of: row.get(13)?,
                sound_channels: row.get(14)?,
                input: has_input
                    .then(|| -> rusqlite::Result<InputDetail> {
                        Ok(InputDetail {
                            players: row.get(15)?,
                            coins: row.get(16)?,
                            service: row.get::<_, Option<bool>>(17)?.unwrap_or(false),
                            tilt: row.get::<_, Option<bool>>(18)?.unwrap_or(false),
                        })
                    })
                    .transpose()?,
                driver: has_driver
                    .then(|| -> rusqlite::Result<DriverDetail> {
                        Ok(DriverDetail {
                            status: row.get(19)?,
                            emulation: row.get(20)?,
                            cocktail: row.get(21)?,
                            save_state: row.get(22)?,
                            requires_artwork: row.get::<_, Option<bool>>(23)?.unwrap_or(false),
                            unofficial: row.get::<_, Option<bool>>(24)?.unwrap_or(false),
                            no_sound_hardware: row.get::<_, Option<bool>>(25)?.unwrap_or(false),
                            incomplete: row.get::<_, Option<bool>>(26)?.unwrap_or(false),
                        })
                    })
                    .transpose()?,
                clone_count: to_u64(row.get(27)?),
                features: Vec::new(),
                controls: Vec::new(),
                displays: Vec::new(),
                chips: Vec::new(),
                bios_sets: Vec::new(),
                roms: Vec::new(),
                disks: Vec::new(),
                devices: Vec::new(),
                media_devices: Vec::new(),
                software_lists: Vec::new(),
                ram_options: Vec::new(),
                samples: Vec::new(),
            })
        })
        .optional()?;
    let Some(mut detail) = row else {
        return Ok(None);
    };

    detail.features = rows(
        connection,
        "SELECT feature_type, status, overall FROM machine_features
         WHERE machine_name = ?1 ORDER BY id",
        name,
        |row| {
            Ok(FeatureDetail {
                feature_type: row.get(0)?,
                status: row.get(1)?,
                overall: row.get(2)?,
            })
        },
    )?;
    detail.controls = rows(
        connection,
        "SELECT control_type, player, buttons, required_buttons, ways, ways2, ways3
         FROM machine_controls WHERE machine_name = ?1 ORDER BY id",
        name,
        |row| {
            Ok(ControlDetail {
                control_type: row.get(0)?,
                player: row.get(1)?,
                buttons: row.get(2)?,
                required_buttons: row.get(3)?,
                ways: row.get(4)?,
                ways2: row.get(5)?,
                ways3: row.get(6)?,
            })
        },
    )?;
    detail.displays = rows(
        connection,
        "SELECT tag, display_type, rotate, flip_x, width, height, refresh
         FROM machine_displays WHERE machine_name = ?1 ORDER BY id",
        name,
        |row| {
            Ok(DisplayDetail {
                tag: row.get(0)?,
                display_type: row.get(1)?,
                rotate: row.get(2)?,
                flip_x: row.get(3)?,
                width: row.get(4)?,
                height: row.get(5)?,
                refresh: row.get(6)?,
            })
        },
    )?;
    detail.chips = rows(
        connection,
        "SELECT chip_type, name, tag, clock FROM machine_chips
         WHERE machine_name = ?1 ORDER BY id",
        name,
        |row| {
            Ok(ChipDetail {
                chip_type: row.get(0)?,
                name: row.get(1)?,
                tag: row.get(2)?,
                clock: row.get(3)?,
            })
        },
    )?;
    detail.bios_sets = rows(
        connection,
        "SELECT name, description, is_default FROM machine_bios_sets
         WHERE machine_name = ?1 ORDER BY id",
        name,
        |row| {
            Ok(BiosSetDetail {
                name: row.get(0)?,
                description: row.get(1)?,
                is_default: row.get(2)?,
            })
        },
    )?;
    detail.roms = rows(
        connection,
        "SELECT name, bios, size, crc, sha1, merge_name, region, offset, status, is_optional
         FROM machine_roms WHERE machine_name = ?1 ORDER BY id",
        name,
        |row| {
            Ok(RomDetail {
                name: row.get(0)?,
                bios: row.get(1)?,
                size: row.get(2)?,
                crc: row.get(3)?,
                sha1: row.get(4)?,
                merge_name: row.get(5)?,
                region: row.get(6)?,
                offset: row.get(7)?,
                status: row.get(8)?,
                is_optional: row.get(9)?,
            })
        },
    )?;
    detail.disks = rows(
        connection,
        "SELECT name, sha1, merge_name, region, disk_index, is_writable, status, is_optional
         FROM machine_disks WHERE machine_name = ?1 ORDER BY id",
        name,
        |row| {
            Ok(DiskDetail {
                name: row.get(0)?,
                sha1: row.get(1)?,
                merge_name: row.get(2)?,
                region: row.get(3)?,
                index: row.get(4)?,
                is_writable: row.get(5)?,
                status: row.get(6)?,
                is_optional: row.get(7)?,
            })
        },
    )?;
    detail.devices = rows(
        connection,
        "SELECT r.device_name, d.description, r.instance_count
         FROM machine_device_refs r LEFT JOIN machines d ON d.name = r.device_name
         WHERE r.machine_name = ?1 ORDER BY r.device_name",
        name,
        |row| {
            Ok(DeviceDetail {
                device: RecordLink {
                    name: row.get(0)?,
                    description: row.get(1)?,
                },
                instance_count: row.get(2)?,
            })
        },
    )?;
    detail.media_devices = media_devices(connection, name)?;
    detail.software_lists = rows(
        connection,
        "SELECT m.list_name, m.tag, m.status, m.filter, l.description, l.name IS NOT NULL
         FROM machine_software_lists m LEFT JOIN software_lists l ON l.name = m.list_name
         WHERE m.machine_name = ?1 ORDER BY m.id",
        name,
        |row| {
            Ok(MachineSoftwareList {
                name: row.get(0)?,
                tag: row.get(1)?,
                status: row.get(2)?,
                filter: row.get(3)?,
                description: row.get(4)?,
                available: row.get(5)?,
            })
        },
    )?;
    detail.ram_options = rows(
        connection,
        "SELECT name, size, is_default FROM machine_ram_options
         WHERE machine_name = ?1 ORDER BY id",
        name,
        |row| {
            Ok(RamOptionDetail {
                name: row.get(0)?,
                size: row.get(1)?,
                is_default: row.get(2)?,
            })
        },
    )?;
    detail.samples = rows(
        connection,
        "SELECT name FROM machine_samples WHERE machine_name = ?1 ORDER BY id",
        name,
        |row| row.get(0),
    )?;
    Ok(Some(detail))
}

fn media_devices(connection: &Connection, name: &str) -> rusqlite::Result<Vec<MediaDeviceDetail>> {
    let mut statement = connection.prepare_cached(
        "SELECT id, device_type, tag, interface, mandatory, fixed_image, instance_name,
                instance_brief_name
         FROM machine_media_devices WHERE machine_name = ?1 ORDER BY id",
    )?;
    let devices = statement
        .query_map([name], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                MediaDeviceDetail {
                    device_type: row.get(1)?,
                    tag: row.get(2)?,
                    interface: row.get(3)?,
                    mandatory: row.get(4)?,
                    fixed_image: row.get(5)?,
                    instance_name: row.get(6)?,
                    instance_brief_name: row.get(7)?,
                    extensions: Vec::new(),
                },
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut extensions = connection.prepare_cached(
        "SELECT extension FROM machine_media_device_extensions
         WHERE media_device_id = ?1 ORDER BY id",
    )?;
    devices
        .into_iter()
        .map(|(id, mut device)| {
            device.extensions = extensions
                .query_map(params![id], |row| row.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            Ok(device)
        })
        .collect()
}

fn rows<T>(
    connection: &Connection,
    sql: &str,
    name: &str,
    map: impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
) -> rusqlite::Result<Vec<T>> {
    connection
        .prepare_cached(sql)?
        .query_map([name], map)?
        .collect()
}
