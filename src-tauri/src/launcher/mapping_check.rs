//! Which player mappings MAME applied, from the verbose output of a session.
//!
//! A launch that maps devices passes `-verbose`; MAME then prints (verified
//! with MAME 0.289) the joysticks in their initial order (`Input: Adding
//! joystick #n`), `Attempting to parse: <file>.cfg` when it reads the
//! generated controller file, one `Input: Remapped joystick #n` line per
//! `mapdevice` it applied (each swaps two joystick indexes, even when the
//! device is already there), then `Attempting to parse:` for its next
//! configuration file, or `Configuration file <name> not found` when that
//! file does not exist yet (a new configuration directory). The final order
//! is replayed from these lines.
//!
//! A mapping is only reported as applied when a `Remapped` line moved its
//! device to its player's index and it is still there once the controller
//! file is read; the absence of errors proves nothing. MAME prints nothing
//! for a device it does not find.

use serde::Serialize;

use super::controls::PlanMapping;
use crate::controllers::model::{ControllerBinding, device_ids_overlap};
use crate::mame::input_log::{InputLogLine, LoggedDevice, parse_line};

/// What the output says about one mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MappingStatus {
    /// MAME has not read the controller file yet.
    Pending,
    /// Applied to a device whose id identifies it (`device` binding) and
    /// that no other connected device could match.
    Verified,
    /// Applied, but the id follows the port or enumeration order (or
    /// another connected device matches it): the physical device behind it
    /// is not guaranteed.
    VerifiedPortOrder,
    /// MAME reported no device with this id.
    Missing,
    /// The device was found but MAME did not leave it at the player's index.
    NotApplied,
    /// The output does not say (no verbose output, MAME stopped early...).
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerVerification {
    pub player: u32,
    pub controller_id: String,
    pub controller_name: String,
    pub device_id: String,
    pub binding: ControllerBinding,
    pub status: MappingStatus,
    /// The name MAME reported for the device, when found.
    pub device_name: Option<String>,
}

/// The verification of a session's mappings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MappingVerification {
    /// `false` while MAME may still report the mappings.
    pub complete: bool,
    pub players: Vec<PlayerVerification>,
    /// Why the output does not tell more, when it does not.
    pub message: Option<String>,
}

impl MappingVerification {
    /// The initial state of a session.
    pub fn pending(mappings: &[PlanMapping]) -> Self {
        Self {
            complete: false,
            players: players(mappings, |_| (MappingStatus::Pending, None)),
            message: None,
        }
    }
}

fn players(
    mappings: &[PlanMapping],
    status: impl Fn(&PlanMapping) -> (MappingStatus, Option<String>),
) -> Vec<PlayerVerification> {
    mappings
        .iter()
        .map(|mapping| {
            let (status, device_name) = status(mapping);
            PlayerVerification {
                player: mapping.player,
                controller_id: mapping.controller_id.clone(),
                controller_name: mapping.controller_name.clone(),
                device_id: mapping.device_id.clone(),
                binding: mapping.binding,
                status,
                device_name,
            }
        })
        .collect()
}

/// Verifies `mappings` against the output of a session started with the
/// controller file `file_name` (`<name>.cfg`). `exited` tells whether MAME
/// has stopped, so the output is all there is.
pub fn verify(
    output: &str,
    file_name: &str,
    mappings: &[PlanMapping],
    exited: bool,
) -> MappingVerification {
    let mut added: Vec<LoggedDevice> = Vec::new();
    let mut remapped: Vec<LoggedDevice> = Vec::new();
    let (mut started, mut done, mut failed) = (false, false, false);
    for line in output.lines() {
        match parse_line(line) {
            Some(InputLogLine::Added(device)) if device.class == "joystick" && !started => {
                added.push(device);
            }
            Some(InputLogLine::Parsing(file)) if file == file_name => started = true,
            Some(InputLogLine::Parsing(_) | InputLogLine::ConfigurationMissing(_)) if started => {
                done = true;
                break;
            }
            Some(InputLogLine::Remapped(device)) if started && device.class == "joystick" => {
                remapped.push(device);
            }
            Some(InputLogLine::ControllerFileFailed(file)) if file == file_name => {
                failed = true;
                break;
            }
            _ => {}
        }
    }
    let ended = |status: MappingStatus, message: &str| MappingVerification {
        complete: true,
        players: players(mappings, |_| (status, None)),
        message: Some(message.to_owned()),
    };
    if failed {
        return ended(
            MappingStatus::NotApplied,
            "MAME could not load the generated controller file.",
        );
    }
    if !done {
        if !exited {
            return MappingVerification::pending(mappings);
        }
        return ended(
            MappingStatus::Unknown,
            if started {
                "MAME stopped while reading the controller file."
            } else {
                "MAME's output does not show the controller file being read (MAME may have stopped before, or its output was not recorded)."
            },
        );
    }

    // Replay the swaps: `order[i]` is the device at joystick index i + 1.
    let mut order: Vec<Option<&LoggedDevice>> = Vec::new();
    for device in &added {
        let index = usize::try_from(device.index).unwrap_or(usize::MAX) - 1;
        if index >= order.len() {
            order.resize(index + 1, None);
        }
        order[index] = Some(device);
    }
    let mut moved: Vec<(&str, u32)> = Vec::new();
    for remap in &remapped {
        let target = usize::try_from(remap.index).unwrap_or(usize::MAX) - 1;
        let Some(current) = order
            .iter()
            .position(|slot| slot.is_some_and(|device| device.id == remap.id))
        else {
            continue;
        };
        if target >= order.len() {
            order.resize(target + 1, None);
        }
        order.swap(current, target);
        moved.push((remap.id.as_str(), remap.index));
    }
    MappingVerification {
        complete: true,
        players: players(mappings, |mapping| {
            let matches: Vec<&LoggedDevice> = added
                .iter()
                .filter(|device| contains_id(&device.id, &mapping.device_id))
                .collect();
            if matches.is_empty() {
                return (MappingStatus::Missing, None);
            }
            let index = usize::try_from(mapping.player).unwrap_or(usize::MAX) - 1;
            let at = order.get(index).copied().flatten();
            let applied = at.is_some_and(|device| {
                contains_id(&device.id, &mapping.device_id)
                    && moved
                        .iter()
                        .any(|(id, slot)| *id == device.id && *slot == mapping.player)
            });
            let name = at.map(|device| device.name.clone());
            if !applied {
                return (
                    MappingStatus::NotApplied,
                    matches.first().map(|d| d.name.clone()),
                );
            }
            let unique = matches.len() == 1
                && !added.iter().any(|device| {
                    device.id != matches[0].id && device_ids_overlap(&device.id, &mapping.device_id)
                });
            if mapping.binding == ControllerBinding::Device && unique {
                (MappingStatus::Verified, name)
            } else {
                (MappingStatus::VerifiedPortOrder, name)
            }
        }),
        message: None,
    }
}

/// MAME's match: the device id contains the mapped id, without case.
fn contains_id(device_id: &str, mapped: &str) -> bool {
    device_id.to_lowercase().contains(&mapped.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controllers::devices::JoystickProvider;

    const GP100_A: &str = "USB gamepad            product_e401081f-0000-0000-0000-504944564944 instance_1a2b3c4d-1111-11f0-8001-444553540000";
    const GP100_B: &str = "USB gamepad            product_e401081f-0000-0000-0000-504944564944 instance_5e6f7a8b-2222-11f1-8001-444553540000";
    const F310: &str = "Controller (Gamepad F310) product_c21d046d-0000-0000-0000-504944564944 instance_9c0d1e2f-3333-11f0-8001-444553540000";
    const FILE: &str = "ymdb-0123456789abcdef.cfg";

    fn mapping(player: u32, id: &str, binding: ControllerBinding) -> PlanMapping {
        PlanMapping {
            player,
            controller_id: format!("pad-{player}"),
            controller_name: format!("Pad {player}"),
            provider: JoystickProvider::Winhybrid,
            device_id: id.to_owned(),
            binding,
        }
    }

    fn added(lines: &[(u32, &str, &str)]) -> String {
        lines
            .iter()
            .map(|(index, name, id)| {
                format!("Input: Adding joystick #{index}: {name} (device id: {id})\n")
            })
            .collect()
    }

    fn remapped(index: u32, name: &str, id: &str) -> String {
        format!("Input: Remapped joystick #{index}: {name} (device id: {id})\n")
    }

    fn statuses(verification: &MappingVerification) -> Vec<MappingStatus> {
        verification
            .players
            .iter()
            .map(|player| player.status)
            .collect()
    }

    /// The session 3A scenario: F310 P1, GP100-A P2, GP100-B P3.
    fn three_controllers() -> (String, Vec<PlanMapping>) {
        let mut log = added(&[
            (1, "USB gamepad", GP100_A),
            (2, "USB gamepad", GP100_B),
            (3, "Controller (Gamepad F310)", F310),
        ]);
        log.push_str("Starting Pong (Rev E) ':'\n");
        log.push_str(&format!("Attempting to parse: {FILE}\r\n"));
        log.push_str("Applying default configuration from controller configuration file x\n");
        log.push_str(&remapped(1, "Controller (Gamepad F310)", F310));
        log.push_str(&remapped(2, "USB gamepad", GP100_A));
        log.push_str(&remapped(3, "USB gamepad", GP100_B));
        log.push_str("Attempting to parse: default.cfg\n");
        (
            log,
            vec![
                mapping(1, F310, ControllerBinding::Device),
                mapping(2, GP100_A, ControllerBinding::PortOrder),
                mapping(3, GP100_B, ControllerBinding::PortOrder),
            ],
        )
    }

    #[test]
    fn verifies_applied_mappings_by_binding() {
        let (log, mappings) = three_controllers();
        let verification = verify(&log, FILE, &mappings, false);
        assert!(verification.complete);
        assert_eq!(
            statuses(&verification),
            [
                MappingStatus::Verified,
                MappingStatus::VerifiedPortOrder,
                MappingStatus::VerifiedPortOrder
            ]
        );
        assert_eq!(
            verification.players[0].device_name.as_deref(),
            Some("Controller (Gamepad F310)")
        );
    }

    #[test]
    fn a_new_configuration_directory_also_ends_the_controller_file() {
        // MAME 0.289 with a configuration directory created for the launch.
        let (log, mappings) = three_controllers();
        let log = log.replace(
            "Attempting to parse: default.cfg",
            "Applying default configuration from controller configuration file x\nConfiguration file default.cfg not found\nConfiguration file pong.cfg not found",
        );
        let verification = verify(&log, FILE, &mappings, false);
        assert!(verification.complete);
        assert_eq!(statuses(&verification)[0], MappingStatus::Verified);
    }

    #[test]
    fn is_pending_until_the_controller_file_is_read() {
        let (log, mappings) = three_controllers();
        let partial: String = log
            .lines()
            .take(5)
            .map(|line| format!("{line}\n"))
            .collect();
        let verification = verify(&partial, FILE, &mappings, false);
        assert!(!verification.complete);
        assert!(
            statuses(&verification)
                .iter()
                .all(|s| *s == MappingStatus::Pending)
        );
        // Once MAME has exited, a partial output stays unknown.
        let verification = verify(&partial, FILE, &mappings, true);
        assert!(verification.complete);
        assert!(
            statuses(&verification)
                .iter()
                .all(|s| *s == MappingStatus::Unknown)
        );
        assert!(
            verification
                .message
                .expect("message")
                .contains("stopped while reading")
        );
        // No verbose output at all.
        let verification = verify("", FILE, &mappings, true);
        assert!(
            statuses(&verification)
                .iter()
                .all(|s| *s == MappingStatus::Unknown)
        );
    }

    #[test]
    fn missing_devices_and_unapplied_mappings() {
        // Only GP100-B is connected: GP100-A is missing.
        let mut log = added(&[(1, "USB gamepad", GP100_B)]);
        log.push_str(&format!("Attempting to parse: {FILE}\n"));
        log.push_str(&remapped(2, "USB gamepad", GP100_B));
        log.push_str("Attempting to parse: default.cfg\n");
        let mappings = [
            mapping(1, GP100_A, ControllerBinding::PortOrder),
            mapping(2, GP100_B, ControllerBinding::PortOrder),
        ];
        assert_eq!(
            statuses(&verify(&log, FILE, &mappings, false)),
            [MappingStatus::Missing, MappingStatus::VerifiedPortOrder]
        );
        // A device found without a Remapped line was not mapped.
        let mut log = added(&[(1, "USB gamepad", GP100_A)]);
        log.push_str(&format!(
            "Attempting to parse: {FILE}\nAttempting to parse: default.cfg\n"
        ));
        assert_eq!(
            statuses(&verify(&log, FILE, &mappings[..1], false)),
            [MappingStatus::NotApplied]
        );
    }

    #[test]
    fn later_swaps_that_move_a_device_away_are_detected() {
        // A later mapping (for example of the baseline) moves F310 away.
        let mut log = added(&[(1, "Pad", GP100_A), (2, "F310", F310)]);
        log.push_str(&format!("Attempting to parse: {FILE}\n"));
        log.push_str(&remapped(1, "F310", F310));
        log.push_str(&remapped(1, "Pad", GP100_A));
        log.push_str("Attempting to parse: default.cfg\n");
        let mappings = [mapping(1, F310, ControllerBinding::Device)];
        assert_eq!(
            statuses(&verify(&log, FILE, &mappings, true)),
            [MappingStatus::NotApplied]
        );
    }

    #[test]
    fn ambiguous_ids_are_never_reported_as_identified() {
        // Two devices with the same SDL GUID: MAME takes the first one.
        let guid = "0300007c1f08000001e4000000000000";
        let mut log = added(&[(1, "USB gamepad", guid), (2, "USB gamepad", guid)]);
        log.push_str(&format!("Attempting to parse: {FILE}\n"));
        log.push_str(&remapped(1, "USB gamepad", guid));
        log.push_str("Attempting to parse: default.cfg\n");
        let mut sdl = mapping(1, guid, ControllerBinding::Device);
        sdl.provider = JoystickProvider::Sdljoy;
        assert_eq!(
            statuses(&verify(&log, FILE, &[sdl], true)),
            [MappingStatus::VerifiedPortOrder]
        );
    }

    #[test]
    fn a_controller_file_that_fails_to_load_applies_nothing() {
        let mut log = added(&[(1, "Pad", GP100_A)]);
        log.push_str(&format!(
            "Attempting to parse: {FILE}\nFatal error: Could not load controller configuration file {FILE}\n"
        ));
        let verification = verify(
            &log,
            FILE,
            &[mapping(1, GP100_A, ControllerBinding::PortOrder)],
            true,
        );
        assert_eq!(statuses(&verification), [MappingStatus::NotApplied]);
        assert!(
            verification
                .message
                .expect("message")
                .contains("could not load")
        );
    }

    #[test]
    fn serializes_for_the_session() {
        let verification = MappingVerification::pending(&[mapping(
            2,
            "XInput Player 1",
            ControllerBinding::PortOrder,
        )]);
        assert_eq!(
            serde_json::to_value(&verification).expect("json"),
            serde_json::json!({
                "complete": false,
                "players": [{
                    "player": 2,
                    "controllerId": "pad-2",
                    "controllerName": "Pad 2",
                    "deviceId": "XInput Player 1",
                    "binding": "portOrder",
                    "status": "pending",
                    "deviceName": null
                }],
                "message": null
            })
        );
    }
}
