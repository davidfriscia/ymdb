//! The candidate files of a launch, from the catalog.
//!
//! Only relations stated by MAME are followed: `romof` and `cloneof` for
//! parents and BIOS sets, `device_ref` for devices (recursively), disks with
//! their `merge` attribute, `sampleof` and the samples of the machine, and
//! `cloneof` for software. Files are whole archives or disks; the ROMs inside
//! them are never examined. A set is a candidate when the catalog lists at
//! least one dumped ROM or disk for it.
//!
//! Paths follow MAME's conventions below a collection root:
//!
//! | Category            | Path                         |
//! | ------------------- | ---------------------------- |
//! | Machine ROMs        | `<set>.zip`                  |
//! | Machine CHDs        | `<set>/<disk>.chd`           |
//! | Software List ROMs  | `<list>/<software>.zip`      |
//! | Software List CHDs  | `<list>/<software>/<disk>.chd` |
//! | Samples (ZIP)       | `<sample set>.zip`           |
//! | Samples (directory) | `<sample set>/<sample>.wav`  |

use std::collections::HashSet;
use std::fmt;

use rusqlite::{Connection, OptionalExtension, params};

use super::Reason;
use crate::remote::sources::{CollectionOrganization, SourceCategory};

/// Relations followed at most this deep (MAME chains are 2 or 3 sets long).
const MAX_CHAIN: usize = 16;

/// A file that a launch may need.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub category: SourceCategory,
    /// The set the file belongs to: a machine or sample set, or `<list>/<software>`.
    pub set: String,
    pub reason: Reason,
    /// The path below the collection root, the same remotely and locally.
    /// Names coming from the catalog are checked when the plan is built.
    pub segments: Vec<String>,
    /// Collection organizations in which the file is legitimately absent.
    pub absent_note: Option<AbsentNote>,
}

/// Why a file may be absent from a collection of some organizations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbsentNote {
    pub organizations: Vec<CollectionOrganization>,
    pub message: String,
}

/// The organizations of the configured sources that change which files exist.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Organizations {
    pub machine_chds: Option<CollectionOrganization>,
    pub samples: Option<CollectionOrganization>,
}

/// What is launched.
#[derive(Debug, Clone, Copy)]
pub struct PlanTarget<'a> {
    pub machine: &'a str,
    pub software: Option<SoftwareTarget<'a>>,
}

#[derive(Debug, Clone, Copy)]
pub struct SoftwareTarget<'a> {
    pub list: &'a str,
    pub name: &'a str,
    /// The mounted parts; their disks are candidates. Empty means every part.
    pub parts: &'a [String],
}

#[derive(Debug)]
pub enum PlannerError {
    NotFound(String),
    Database(rusqlite::Error),
}

impl fmt::Display for PlannerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound(message) => f.write_str(message),
            Self::Database(error) => write!(f, "catalog error: {error}"),
        }
    }
}

impl std::error::Error for PlannerError {}

impl From<rusqlite::Error> for PlannerError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Database(error)
    }
}

/// The candidate files of a launch, without repetitions: the software first
/// (when one is launched), then the machine's ROM sets (machine, parents,
/// BIOS, devices), its disks and its samples.
pub fn plan(
    connection: &Connection,
    target: &PlanTarget<'_>,
    organizations: Organizations,
) -> Result<Vec<Candidate>, PlannerError> {
    let mut candidates = Candidates::default();
    let machine_reason = match &target.software {
        Some(software) => {
            software_candidates(connection, software, &mut candidates)?;
            Reason::System
        }
        None => Reason::Selected,
    };
    machine_candidates(
        connection,
        target.machine,
        machine_reason,
        organizations,
        &mut candidates,
    )?;
    Ok(candidates.list)
}

/// Candidates in order, deduplicated by category and path.
#[derive(Default)]
struct Candidates {
    list: Vec<Candidate>,
}

impl Candidates {
    fn push(&mut self, candidate: Candidate) {
        if !self
            .list
            .iter()
            .any(|c| c.category == candidate.category && c.segments == candidate.segments)
        {
            self.list.push(candidate);
        }
    }
}

struct MachineRow {
    name: String,
    clone_of: Option<String>,
    rom_of: Option<String>,
    sample_of: Option<String>,
    is_bios: bool,
}

fn machine_row(connection: &Connection, name: &str) -> rusqlite::Result<Option<MachineRow>> {
    connection
        .prepare_cached(
            "SELECT name, clone_of, rom_of, sample_of, is_bios FROM machines WHERE name = ?1",
        )?
        .query_row([name], |row| {
            Ok(MachineRow {
                name: row.get(0)?,
                clone_of: row.get(1)?,
                rom_of: row.get(2)?,
                sample_of: row.get(3)?,
                is_bios: row.get(4)?,
            })
        })
        .optional()
}

/// Dumped ROMs of a machine, and those not merged from another set.
fn rom_counts(connection: &Connection, machine: &str) -> rusqlite::Result<(i64, i64)> {
    connection
        .prepare_cached(
            "SELECT COALESCE(SUM(status <> 'nodump'), 0),
                    COALESCE(SUM(status <> 'nodump' AND merge_name IS NULL), 0)
             FROM machine_roms WHERE machine_name = ?1",
        )?
        .query_row([machine], |row| Ok((row.get(0)?, row.get(1)?)))
}

/// Dumped disks of a machine: name and the disk it is merged from.
fn machine_disks(
    connection: &Connection,
    machine: &str,
) -> rusqlite::Result<Vec<(String, Option<String>)>> {
    connection
        .prepare_cached(
            "SELECT name, merge_name FROM machine_disks
             WHERE machine_name = ?1 AND status <> 'nodump' ORDER BY id",
        )?
        .query_map([machine], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect()
}

fn device_refs(connection: &Connection, machine: &str) -> rusqlite::Result<Vec<String>> {
    connection
        .prepare_cached(
            "SELECT device_name FROM machine_device_refs WHERE machine_name = ?1
             ORDER BY device_name",
        )?
        .query_map([machine], |row| row.get(0))?
        .collect()
}

fn machine_samples(connection: &Connection, machine: &str) -> rusqlite::Result<Vec<String>> {
    let names: Vec<String> = connection
        .prepare_cached("SELECT name FROM machine_samples WHERE machine_name = ?1 ORDER BY id")?
        .query_map([machine], |row| row.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    let mut seen = HashSet::new();
    Ok(names
        .into_iter()
        .filter(|name| seen.insert(name.clone()))
        .collect())
}

fn machine_candidates(
    connection: &Connection,
    machine: &str,
    reason: Reason,
    organizations: Organizations,
    candidates: &mut Candidates,
) -> Result<(), PlannerError> {
    let root = machine_row(connection, machine)?.ok_or_else(|| {
        PlannerError::NotFound(format!("machine '{machine}' is not in the catalog"))
    })?;

    // The machine, then its parents and BIOS sets, nearest first.
    let mut chain = vec![(root, reason)];
    let mut index = 0;
    while index < chain.len() && chain.len() < MAX_CHAIN {
        let related: Vec<String> = [&chain[index].0.rom_of, &chain[index].0.clone_of]
            .into_iter()
            .flatten()
            .cloned()
            .collect();
        for name in related {
            if chain.iter().any(|(row, _)| row.name == name) {
                continue;
            }
            if let Some(row) = machine_row(connection, &name)? {
                let reason = if row.is_bios {
                    Reason::Bios
                } else {
                    Reason::Parent
                };
                chain.push((row, reason));
            }
        }
        index += 1;
    }

    for (row, reason) in &chain {
        let (dumped, unmerged) = rom_counts(connection, &row.name)?;
        if dumped == 0 {
            continue;
        }
        candidates.push(Candidate {
            category: SourceCategory::MachineRoms,
            set: row.name.clone(),
            reason: *reason,
            segments: vec![format!("{}.zip", row.name)],
            absent_note: machine_set_note(row, *reason, unmerged == 0),
        });
    }

    // Devices of the machine and of its devices.
    let mut devices: Vec<String> = Vec::new();
    let mut pending = vec![machine.to_owned()];
    while let Some(name) = pending.pop() {
        if devices.len() >= 4096 {
            break;
        }
        for device in device_refs(connection, &name)? {
            if !devices.contains(&device) && device != machine {
                devices.push(device.clone());
                pending.push(device);
            }
        }
    }
    devices.sort();
    for device in devices {
        if chain.iter().any(|(row, _)| row.name == device) {
            continue;
        }
        if rom_counts(connection, &device)?.0 > 0 {
            candidates.push(Candidate {
                category: SourceCategory::MachineRoms,
                set: device.clone(),
                reason: Reason::Device,
                segments: vec![format!("{device}.zip")],
                absent_note: None,
            });
        }
    }

    // Disks: a disk merged from another set lives in that set's directory in
    // merged and split collections, where that set's own disks cover it.
    let keeps_merged_disks = matches!(
        organizations.machine_chds,
        Some(CollectionOrganization::NonMerged)
    );
    for (row, reason) in &chain {
        for (disk, merge) in machine_disks(connection, &row.name)? {
            if merge.is_some() && !keeps_merged_disks {
                continue;
            }
            candidates.push(Candidate {
                category: SourceCategory::MachineChds,
                set: row.name.clone(),
                reason: *reason,
                segments: vec![row.name.clone(), format!("{disk}.chd")],
                absent_note: (*reason == Reason::Parent).then(non_merged_parent_note),
            });
        }
    }

    // Samples of the machine only: parent/clone rules do not apply to them.
    let chain_root = &chain[0].0;
    let samples = machine_samples(connection, &chain_root.name)?;
    if !samples.is_empty() {
        let set = chain_root
            .sample_of
            .clone()
            .unwrap_or_else(|| chain_root.name.clone());
        if organizations.samples == Some(CollectionOrganization::Directories) {
            for sample in samples {
                candidates.push(Candidate {
                    category: SourceCategory::Samples,
                    set: set.clone(),
                    reason: Reason::Sample,
                    segments: vec![set.clone(), format!("{sample}.wav")],
                    absent_note: None,
                });
            }
        } else {
            candidates.push(Candidate {
                category: SourceCategory::Samples,
                set: set.clone(),
                reason: Reason::Sample,
                segments: vec![format!("{set}.zip")],
                absent_note: None,
            });
        }
    }
    Ok(())
}

/// When the archive of a machine set may be absent from a collection.
fn machine_set_note(row: &MachineRow, reason: Reason, all_merged: bool) -> Option<AbsentNote> {
    use CollectionOrganization::{Merged, Split};
    if let Some(parent) = &row.clone_of {
        return Some(if all_merged {
            AbsentNote {
                organizations: vec![Merged, Split],
                message: format!(
                    "Every ROM of {} is shared with {parent}: merged and split collections may have no archive for it.",
                    row.name
                ),
            }
        } else {
            AbsentNote {
                organizations: vec![Merged],
                message: format!(
                    "{} is a clone of {parent}: merged collections store it in the parent's archive.",
                    row.name
                ),
            }
        });
    }
    if let (true, Some(source)) = (all_merged, &row.rom_of) {
        return Some(AbsentNote {
            organizations: vec![Merged, Split],
            message: format!(
                "Every ROM of {} comes from {source}: merged and split collections may have no archive for it.",
                row.name
            ),
        });
    }
    (reason == Reason::Parent).then(non_merged_parent_note)
}

fn non_merged_parent_note() -> AbsentNote {
    AbsentNote {
        organizations: vec![CollectionOrganization::NonMerged],
        message: "A non-merged collection includes the parent's files in each clone.".to_owned(),
    }
}

struct SoftwareRow {
    id: i64,
    name: String,
    clone_of: Option<String>,
}

fn software_row(
    connection: &Connection,
    list: &str,
    name: &str,
) -> rusqlite::Result<Option<SoftwareRow>> {
    connection
        .prepare_cached(
            "SELECT id, name, clone_of FROM software WHERE list_name = ?1 AND name = ?2",
        )?
        .query_row(params![list, name], |row| {
            Ok(SoftwareRow {
                id: row.get(0)?,
                name: row.get(1)?,
                clone_of: row.get(2)?,
            })
        })
        .optional()
}

fn software_has_roms(connection: &Connection, software_id: i64) -> rusqlite::Result<bool> {
    connection
        .prepare_cached(
            "SELECT EXISTS (
                 SELECT 1 FROM software_parts p
                 JOIN software_data_areas a ON a.part_id = p.id
                 JOIN software_roms r ON r.data_area_id = a.id
                 WHERE p.software_id = ?1 AND r.name IS NOT NULL AND r.status <> 'nodump')",
        )?
        .query_row([software_id], |row| row.get(0))
}

struct SoftwareDisk {
    part: String,
    name: String,
    sha1: Option<String>,
}

fn software_disks(
    connection: &Connection,
    software_id: i64,
) -> rusqlite::Result<Vec<SoftwareDisk>> {
    connection
        .prepare_cached(
            "SELECT p.name, d.name, d.sha1 FROM software_parts p
             JOIN software_disk_areas a ON a.part_id = p.id
             JOIN software_disks d ON d.disk_area_id = a.id
             WHERE p.software_id = ?1 AND d.status <> 'nodump'
             ORDER BY p.id, a.id, d.id",
        )?
        .query_map([software_id], |row| {
            Ok(SoftwareDisk {
                part: row.get(0)?,
                name: row.get(1)?,
                sha1: row.get(2)?,
            })
        })?
        .collect()
}

fn software_candidates(
    connection: &Connection,
    target: &SoftwareTarget<'_>,
    candidates: &mut Candidates,
) -> Result<(), PlannerError> {
    let list = target.list;
    let root = software_row(connection, list, target.name)?.ok_or_else(|| {
        PlannerError::NotFound(format!(
            "software '{list}:{}' is not in the catalog",
            target.name
        ))
    })?;
    let mut chain = vec![(root, Reason::Software)];
    while chain.len() < MAX_CHAIN {
        let Some(parent) = chain.last().and_then(|(row, _)| row.clone_of.clone()) else {
            break;
        };
        if chain.iter().any(|(row, _)| row.name == parent) {
            break;
        }
        match software_row(connection, list, &parent)? {
            Some(row) => chain.push((row, Reason::SoftwareParent)),
            None => break,
        }
    }

    for (row, reason) in &chain {
        if !software_has_roms(connection, row.id)? {
            continue;
        }
        let absent_note = match (&row.clone_of, reason) {
            (Some(parent), _) => Some(AbsentNote {
                organizations: vec![CollectionOrganization::Merged],
                message: format!(
                    "{} is a clone of {parent}: merged collections store it in the parent's archive.",
                    row.name
                ),
            }),
            (None, Reason::SoftwareParent) => Some(non_merged_parent_note()),
            _ => None,
        };
        candidates.push(Candidate {
            category: SourceCategory::SoftwareListRoms,
            set: format!("{list}/{}", row.name),
            reason: *reason,
            segments: vec![list.to_owned(), format!("{}.zip", row.name)],
            absent_note,
        });
    }

    // Disks of the mounted parts, then of the parents' parts with the same names.
    let disks: Vec<Vec<SoftwareDisk>> = chain
        .iter()
        .map(|(row, _)| software_disks(connection, row.id))
        .collect::<rusqlite::Result<_>>()?;
    let mounted = |part: &str| target.parts.is_empty() || target.parts.iter().any(|p| p == part);
    for (index, (row, reason)) in chain.iter().enumerate() {
        for disk in disks[index].iter().filter(|disk| mounted(&disk.part)) {
            let same_as_parent = disk.sha1.is_some()
                && disks[index + 1..]
                    .iter()
                    .flatten()
                    .any(|other| other.name == disk.name && other.sha1 == disk.sha1);
            let absent_note = if same_as_parent {
                Some(AbsentNote {
                    organizations: vec![
                        CollectionOrganization::Merged,
                        CollectionOrganization::Split,
                    ],
                    message: format!(
                        "{} is identical to the parent's disk: merged and split collections may only keep the parent's.",
                        disk.name
                    ),
                })
            } else {
                (*reason == Reason::SoftwareParent).then(non_merged_parent_note)
            };
            candidates.push(Candidate {
                category: SourceCategory::SoftwareListChds,
                set: format!("{list}/{}", row.name),
                reason: *reason,
                segments: vec![
                    list.to_owned(),
                    row.name.clone(),
                    format!("{}.chd", disk.name),
                ],
                absent_note,
            });
        }
    }
    Ok(())
}
