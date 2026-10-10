//! Catalog lookups needed to launch a machine or a software.
//!
//! The catalog describes what MAME declares; it cannot prove that a software
//! works on a machine. Candidates therefore carry the declared relation, the
//! list filter evaluation and the media devices able to mount each part, and
//! the user chooses.

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

/// A machine as needed to launch it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchMachine {
    pub name: String,
    pub description: String,
    pub is_runnable: bool,
}

pub fn machine(connection: &Connection, name: &str) -> rusqlite::Result<Option<LaunchMachine>> {
    connection
        .prepare_cached("SELECT name, description, is_runnable FROM machines WHERE name = ?1")?
        .query_row([name], |row| {
            Ok(LaunchMachine {
                name: row.get(0)?,
                description: row.get(1)?,
                is_runnable: row.get(2)?,
            })
        })
        .optional()
}

/// A media device that can be addressed on the command line by its brief name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaDevice {
    /// Command line name, for example `cart` or `flop1`.
    pub brief_name: String,
    pub instance_name: Option<String>,
    pub device_type: String,
    /// Software list interfaces the device accepts.
    pub interfaces: Vec<String>,
}

impl MediaDevice {
    fn accepts(&self, interface: &str) -> bool {
        self.interfaces.iter().any(|accepted| accepted == interface)
    }
}

/// Media devices of a machine that accept software list parts.
pub fn media_devices(connection: &Connection, machine: &str) -> rusqlite::Result<Vec<MediaDevice>> {
    connection
        .prepare_cached(
            "SELECT instance_brief_name, instance_name, device_type, interface
             FROM machine_media_devices
             WHERE machine_name = ?1 AND instance_brief_name IS NOT NULL AND interface IS NOT NULL
             ORDER BY id",
        )?
        .query_map([machine], |row| {
            let interfaces: String = row.get(3)?;
            Ok(MediaDevice {
                brief_name: row.get(0)?,
                instance_name: row.get(1)?,
                device_type: row.get(2)?,
                interfaces: interfaces
                    .split(',')
                    .map(str::trim)
                    .filter(|interface| !interface.is_empty())
                    .map(str::to_owned)
                    .collect(),
            })
        })?
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SoftwarePart {
    pub name: String,
    pub interface: String,
    /// The `part_id` feature, a human-readable label such as "Side A".
    pub label: Option<String>,
    /// The `compatibility` feature of the part, else of the software.
    pub compatibility: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchSoftware {
    pub list_name: String,
    pub name: String,
    pub description: String,
    pub parts: Vec<SoftwarePart>,
}

pub fn software(
    connection: &Connection,
    list: &str,
    name: &str,
) -> rusqlite::Result<Option<LaunchSoftware>> {
    let row = connection
        .prepare_cached(
            "SELECT s.id, s.description,
                    (SELECT value FROM software_shared_features f
                     WHERE f.software_id = s.id AND f.name = 'compatibility' ORDER BY f.id LIMIT 1)
             FROM software s WHERE s.list_name = ?1 AND s.name = ?2",
        )?
        .query_row(params![list, name], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })
        .optional()?;
    let Some((id, description, shared_compatibility)) = row else {
        return Ok(None);
    };
    let parts = connection
        .prepare_cached(
            "SELECT p.name, p.interface,
                    (SELECT value FROM software_part_features f
                     WHERE f.part_id = p.id AND f.name = 'part_id' ORDER BY f.id LIMIT 1),
                    (SELECT value FROM software_part_features f
                     WHERE f.part_id = p.id AND f.name = 'compatibility' ORDER BY f.id LIMIT 1)
             FROM software_parts p WHERE p.software_id = ?1 ORDER BY p.id",
        )?
        .query_map([id], |row| {
            Ok(SoftwarePart {
                name: row.get(0)?,
                interface: row.get(1)?,
                label: row.get(2)?,
                compatibility: row
                    .get::<_, Option<String>>(3)?
                    .or_else(|| shared_compatibility.clone()),
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(Some(LaunchSoftware {
        list_name: list.to_owned(),
        name: name.to_owned(),
        description,
        parts,
    }))
}

/// Whether MAME considers a part compatible with a machine's list filter,
/// as in `software_list_device::is_compatible`: no filter or no
/// compatibility value means compatible; otherwise one filter value must
/// appear in the compatibility values.
pub fn filter_accepts(filter: Option<&str>, compatibility: Option<&str>) -> bool {
    let (Some(filter), Some(compatibility)) = (filter, compatibility) else {
        return true;
    };
    let compatibility = format!("{compatibility},");
    filter
        .split(',')
        .any(|token| compatibility.contains(&format!("{token},")))
}

/// Declared relation between a machine and a software list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ListRelation {
    Original,
    Compatible,
}

/// One mounted part: the part name and the media device it goes into.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MediaAssignment {
    /// Media device brief name.
    pub device: String,
    pub part: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MachineCandidate {
    pub name: String,
    pub description: String,
    pub year: Option<String>,
    pub manufacturer: Option<String>,
    pub clone_of: Option<String>,
    pub relation: ListRelation,
    /// Filters of the machine's references to the list.
    pub filters: Vec<String>,
    /// Whether the list filter accepts every part of the software.
    pub filter_match: bool,
    /// Media devices accepting at least one part of the software.
    pub devices: Vec<MediaDevice>,
    /// Suggested assignment: parts in order, one per device.
    pub default_media: Vec<MediaAssignment>,
    /// Whether at least one part can be mounted.
    pub launchable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SoftwareLaunchInfo {
    pub list_name: String,
    pub name: String,
    pub description: String,
    pub parts: Vec<SoftwarePart>,
    /// Runnable machines declaring the list: original relations first, then
    /// matching filters, parents before clones, by name.
    pub candidates: Vec<MachineCandidate>,
    /// The only candidate that is original, matches the filter and can mount
    /// the software, if there is exactly one.
    pub recommended: Option<String>,
}

pub fn software_launch_info(
    connection: &Connection,
    list: &str,
    name: &str,
) -> rusqlite::Result<Option<SoftwareLaunchInfo>> {
    let Some(software) = software(connection, list, name)? else {
        return Ok(None);
    };
    // One row per reference to the list; devices are filled in below.
    let references: Vec<MachineCandidate> = connection
        .prepare_cached(
            "SELECT m.name, m.description, m.year, m.manufacturer, m.clone_of, l.status, l.filter
                 FROM machine_software_lists l JOIN machines m ON m.name = l.machine_name
                 WHERE l.list_name = ?1 AND m.is_runnable = 1 AND m.is_device = 0
                 ORDER BY m.name, l.id",
        )?
        .query_map([list], |row| {
            let status: String = row.get(5)?;
            let filter: Option<String> = row.get(6)?;
            let filter_match = software
                .parts
                .iter()
                .all(|part| filter_accepts(filter.as_deref(), part.compatibility.as_deref()));
            Ok(MachineCandidate {
                name: row.get(0)?,
                description: row.get(1)?,
                year: row.get(2)?,
                manufacturer: row.get(3)?,
                clone_of: row.get(4)?,
                relation: if status == "original" {
                    ListRelation::Original
                } else {
                    ListRelation::Compatible
                },
                filters: filter.into_iter().collect(),
                filter_match,
                devices: Vec::new(),
                default_media: Vec::new(),
                launchable: false,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;

    let mut candidates: Vec<MachineCandidate> = Vec::new();
    for reference in references {
        // A machine can reference a list several times (different tags).
        if let Some(existing) = candidates
            .iter_mut()
            .find(|candidate| candidate.name == reference.name)
        {
            if reference.relation == ListRelation::Original {
                existing.relation = ListRelation::Original;
            }
            existing.filter_match |= reference.filter_match;
            for filter in reference.filters {
                if !existing.filters.contains(&filter) {
                    existing.filters.push(filter);
                }
            }
            continue;
        }
        let mut candidate = reference;
        candidate.devices = media_devices(connection, &candidate.name)?
            .into_iter()
            .filter(|device| {
                software
                    .parts
                    .iter()
                    .any(|part| device.accepts(&part.interface))
            })
            .collect();
        candidate.default_media = default_media(&candidate.devices, &software.parts);
        candidate.launchable = !candidate.default_media.is_empty();
        candidates.push(candidate);
    }
    candidates.sort_by(|a, b| {
        (
            a.relation != ListRelation::Original,
            !a.filter_match,
            a.clone_of.is_some(),
            &a.name,
        )
            .cmp(&(
                b.relation != ListRelation::Original,
                !b.filter_match,
                b.clone_of.is_some(),
                &b.name,
            ))
    });
    let mut valid = candidates.iter().filter(|candidate| {
        candidate.relation == ListRelation::Original
            && candidate.filter_match
            && candidate.launchable
    });
    let recommended = match (valid.next(), valid.next()) {
        (Some(only), None) => Some(only.name.clone()),
        _ => None,
    };
    Ok(Some(SoftwareLaunchInfo {
        list_name: software.list_name,
        name: software.name,
        description: software.description,
        parts: software.parts,
        candidates,
        recommended,
    }))
}

/// Parts in order, each to the first free device accepting its interface.
pub fn default_media(devices: &[MediaDevice], parts: &[SoftwarePart]) -> Vec<MediaAssignment> {
    let mut assignments: Vec<MediaAssignment> = Vec::new();
    for part in parts {
        let free = devices.iter().find(|device| {
            device.accepts(&part.interface)
                && !assignments
                    .iter()
                    .any(|assigned| assigned.device == device.brief_name)
        });
        if let Some(device) = free {
            assignments.push(MediaAssignment {
                device: device.brief_name.clone(),
                part: part.name.clone(),
            });
        }
    }
    assignments
}

/// Checks a user's part assignment against the machine's devices.
pub fn validate_media(
    devices: &[MediaDevice],
    parts: &[SoftwarePart],
    media: &[MediaAssignment],
) -> Result<(), String> {
    if media.is_empty() {
        return Err("mount at least one part of the software".to_owned());
    }
    for (index, assignment) in media.iter().enumerate() {
        let device = devices
            .iter()
            .find(|device| device.brief_name == assignment.device)
            .ok_or_else(|| format!("the machine has no media device '{}'", assignment.device))?;
        let part = parts
            .iter()
            .find(|part| part.name == assignment.part)
            .ok_or_else(|| format!("the software has no part '{}'", assignment.part))?;
        if !device.accepts(&part.interface) {
            return Err(format!(
                "media device '{}' does not accept part '{}' ({})",
                device.brief_name, part.name, part.interface
            ));
        }
        let earlier = &media[..index];
        if earlier
            .iter()
            .any(|other| other.device == assignment.device)
        {
            return Err(format!(
                "media device '{}' is used twice",
                assignment.device
            ));
        }
        if earlier.iter().any(|other| other.part == assignment.part) {
            return Err(format!("part '{}' is mounted twice", assignment.part));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(brief: &str, interfaces: &[&str]) -> MediaDevice {
        MediaDevice {
            brief_name: brief.to_owned(),
            instance_name: None,
            device_type: "floppydisk".to_owned(),
            interfaces: interfaces.iter().map(|&i| i.to_owned()).collect(),
        }
    }

    fn part(name: &str, interface: &str) -> SoftwarePart {
        SoftwarePart {
            name: name.to_owned(),
            interface: interface.to_owned(),
            label: None,
            compatibility: None,
        }
    }

    fn assign(device: &str, part: &str) -> MediaAssignment {
        MediaAssignment {
            device: device.to_owned(),
            part: part.to_owned(),
        }
    }

    #[test]
    fn evaluates_list_filters_like_mame() {
        assert!(filter_accepts(None, Some("PAL")));
        assert!(filter_accepts(Some("NTSC"), None));
        assert!(filter_accepts(Some("NTSC"), Some("NTSC,PAL")));
        assert!(filter_accepts(Some("A2E"), Some("A2P,A2E,A2EE,A2C,A2GS")));
        assert!(!filter_accepts(Some("A2"), Some("A2P,A2E,A2EE,A2C,A2GS")));
        assert!(filter_accepts(
            Some("MC68030,MC68030_32"),
            Some("MC68030_32")
        ));
        assert!(!filter_accepts(Some("PAL"), Some("NTSC")));
        assert!(!filter_accepts(Some("NTSC"), Some("NTSC-J")));
    }

    #[test]
    fn assigns_parts_in_order_to_free_devices() {
        let devices = [
            device("flop1", &["floppy_5_25"]),
            device("flop2", &["floppy_5_25"]),
        ];
        let parts = [
            part("flop1", "floppy_5_25"),
            part("flop2", "floppy_5_25"),
            part("flop3", "floppy_5_25"),
        ];
        assert_eq!(
            default_media(&devices, &parts),
            [assign("flop1", "flop1"), assign("flop2", "flop2")]
        );
        let cart = [device("cart", &["c64_cart", "vic10_cart"])];
        assert_eq!(
            default_media(&cart, &[part("cart", "vic10_cart")]),
            [assign("cart", "cart")]
        );
        assert!(default_media(&cart, &[part("flop1", "floppy_5_25")]).is_empty());
    }

    #[test]
    fn validates_part_assignments() {
        let devices = [
            device("flop1", &["floppy_5_25"]),
            device("flop2", &["floppy_5_25"]),
            device("cass", &["apple2_cass"]),
        ];
        let parts = [part("flop1", "floppy_5_25"), part("flop2", "floppy_5_25")];
        assert_eq!(
            validate_media(&devices, &parts, &[assign("flop2", "flop1")]),
            Ok(())
        );
        assert_eq!(
            validate_media(
                &devices,
                &parts,
                &[assign("flop1", "flop2"), assign("flop2", "flop1")]
            ),
            Ok(())
        );
        for invalid in [
            vec![],
            vec![assign("flop9", "flop1")],
            vec![assign("flop1", "side3")],
            vec![assign("cass", "flop1")],
            vec![assign("flop1", "flop1"), assign("flop1", "flop2")],
            vec![assign("flop1", "flop1"), assign("flop2", "flop1")],
        ] {
            assert!(
                validate_media(&devices, &parts, &invalid).is_err(),
                "{invalid:?}"
            );
        }
    }
}
