//! On-demand detection of the connected joysticks, with the MAME executable
//! of an installation.
//!
//! MAME lists its input devices with `-verbose` when a machine starts. The
//! detection runs `pong` (a machine without ROM files) without video or
//! sound for one emulated second, with `-noreadconfig` and its working
//! directory in a new temporary directory, so that no INI file is read and
//! everything MAME writes (`cfg`, `nvram`...) stays there; the directory is
//! removed afterwards. MAME runs directly (never through a shell) with a
//! timeout. Only the `Input: Adding joystick` lines (and the SDL lines of
//! `sdljoy`) are interpreted. Nothing is registered: the user chooses which
//! detected devices become controllers.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use serde::Serialize;

use super::devices::{self, DeviceIdentity, JoystickProvider};
use super::model::{ControllerBinding, ControllersConfig, DeviceClass, device_ids_overlap};
use crate::launcher::session::describe_exit;
use crate::mame::input_log::{InputLogLine, SdlJoystick, parse_line};
use crate::mame::process::{self, ProcessError};

/// A machine of every MAME build that needs no ROM file and stops cleanly
/// without video (validated with MAME 0.289).
pub const DETECTION_MACHINE: &str = "pong";
const TIMEOUT: Duration = Duration::from_secs(30);

/// A joystick reported by MAME.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedDevice {
    /// MAME's joystick number in this detection (its enumeration order).
    pub index: u32,
    pub name: String,
    pub provider: JoystickProvider,
    /// The complete MAME device id, as `mapdevice` matches it.
    pub device_id: String,
    pub device_class: DeviceClass,
    pub identity: DeviceIdentity,
    /// USB ids, when the provider's id or output encodes them.
    pub vendor_id: Option<String>,
    pub product_id: Option<String>,
    /// The serial number reported by SDL (`sdljoy`); not part of the id.
    pub serial: Option<String>,
    /// Whether no other detected device has an id MAME could confuse with it.
    pub distinguishable: bool,
    pub allowed_bindings: Vec<ControllerBinding>,
    /// The binding proposed for registration; `None` when the device cannot
    /// be told apart from another one.
    pub suggested_binding: Option<ControllerBinding>,
    /// Ids of the controllers registered with this provider and device id.
    pub registered_controllers: Vec<String>,
    /// What the user should know about this device id.
    pub notes: Vec<String>,
}

/// The result of a detection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceDetection {
    pub installation_id: String,
    pub provider: JoystickProvider,
    pub devices: Vec<DetectedDevice>,
    pub warnings: Vec<String>,
}

/// Why a detection failed. A failed detection is never reported as "no device".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DetectionError {
    /// MAME could not run or did not finish.
    Failed(String),
    /// MAME ran, but its output has no input device list.
    NotRecognized(String),
}

impl std::fmt::Display for DetectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Failed(message) | Self::NotRecognized(message) => f.write_str(message),
        }
    }
}

/// The detection command, run from `work_dir`.
pub fn detection_command(
    executable: &Path,
    provider: JoystickProvider,
    work_dir: &Path,
) -> Command {
    let mut command = Command::new(executable);
    command
        .args([
            DETECTION_MACHINE,
            "-noreadconfig",
            "-verbose",
            "-video",
            "none",
            "-sound",
            "none",
            "-nothrottle",
            "-seconds_to_run",
            "1",
            "-joystickprovider",
            provider.name(),
        ])
        .current_dir(work_dir);
    command
}

/// Runs the detection in a new directory under `temp_root` (removed
/// afterwards) and returns MAME's output.
pub fn run(
    executable: &Path,
    provider: JoystickProvider,
    temp_root: &Path,
) -> Result<String, DetectionError> {
    run_within(executable, provider, temp_root, TIMEOUT)
}

fn run_within(
    executable: &Path,
    provider: JoystickProvider,
    temp_root: &Path,
    timeout: Duration,
) -> Result<String, DetectionError> {
    let work_dir = temp_root.join(format!("ymdb-detect-{}", uuid::Uuid::new_v4().simple()));
    fs::create_dir_all(&work_dir).map_err(|error| {
        DetectionError::Failed(format!(
            "cannot create the temporary directory '{}': {error}",
            work_dir.display()
        ))
    })?;
    let result =
        process::run_with_timeout(detection_command(executable, provider, &work_dir), timeout);
    remove_work_dir(&work_dir);
    let output = result.map_err(|error| {
        DetectionError::Failed(match error {
            ProcessError::Spawn(error) => format!("MAME could not be started: {error}"),
            ProcessError::Wait(error) => format!("MAME could not be monitored: {error}"),
            ProcessError::TimedOut => format!(
                "MAME did not finish the detection within {} seconds and was stopped",
                timeout.as_secs()
            ),
        })
    })?;
    let text = format!("{}\n{}", output.stdout, output.stderr);
    if output.exit_code != Some(0) {
        let tail: Vec<&str> = text
            .lines()
            .filter(|line| !line.trim().is_empty())
            .collect();
        let tail = tail[tail.len().saturating_sub(3)..].join(" / ");
        return Err(DetectionError::Failed(format!(
            "the detection failed: {}{}",
            describe_exit(output.exit_code),
            if tail.is_empty() {
                String::new()
            } else {
                format!(" ({tail})")
            }
        )));
    }
    Ok(text)
}

/// Best effort: a leftover temporary directory is harmless.
fn remove_work_dir(dir: &PathBuf) {
    let _ = fs::remove_dir_all(dir);
}

/// The joysticks listed in MAME's output, compared with the registered
/// controllers.
pub fn interpret(
    output: &str,
    provider: JoystickProvider,
    config: &ControllersConfig,
) -> Result<(Vec<DetectedDevice>, Vec<String>), DetectionError> {
    let mut any_device = false;
    let mut sdl: Vec<SdlJoystick> = Vec::new();
    let mut joysticks = Vec::new();
    for line in output.lines() {
        match parse_line(line) {
            Some(InputLogLine::Added(device)) => {
                any_device = true;
                if device.class == "joystick" {
                    joysticks.push(device);
                }
            }
            Some(InputLogLine::Sdl(info)) => sdl.push(info),
            _ => {}
        }
    }
    if !any_device {
        return Err(DetectionError::NotRecognized(
            "MAME's output lists no input device: this MAME version may not report its devices, or the detection was interrupted".to_owned(),
        ));
    }
    let mut warnings = Vec::new();
    if joysticks.is_empty() {
        warnings.push(format!(
            "No joystick was detected with the {} provider. Check that the controllers are connected, or try another provider.",
            provider.name()
        ));
    }
    let devices = joysticks
        .iter()
        .map(|device| {
            let identity = devices::identity(provider, &device.id);
            let distinguishable = !joysticks
                .iter()
                .any(|other| other.index != device.index && device_ids_overlap(&other.id, &device.id));
            let same_name = joysticks
                .iter()
                .filter(|other| other.name == device.name)
                .count();
            let info = sdl.iter().find(|info| info.guid == device.id);
            let (vendor_id, product_id) = match (info, devices::direct_input_usb_ids(&device.id)) {
                (Some(info), _) => (Some(info.vendor_id.clone()), Some(info.product_id.clone())),
                (None, Some((vendor, product))) => (Some(vendor), Some(product)),
                (None, None) => (None, None),
            };
            let mut notes = vec![identity_note(identity).to_owned()];
            if !distinguishable {
                notes.push(
                    "Another detected device has the same id: MAME cannot tell them apart with this provider, so it cannot be registered. Try another provider."
                        .to_owned(),
                );
            } else if same_name > 1 {
                notes.push(format!(
                    "{same_name} detected devices are named '{}': only their position tells them apart. Swapping their USB ports swaps them.",
                    device.name
                ));
            }
            DetectedDevice {
                index: device.index,
                name: device.name.clone(),
                provider,
                device_id: device.id.clone(),
                device_class: DeviceClass::Joystick,
                identity,
                vendor_id,
                product_id,
                serial: info.and_then(|info| info.serial.clone()),
                distinguishable,
                allowed_bindings: identity.allowed_bindings(),
                suggested_binding: distinguishable.then_some(ControllerBinding::PortOrder),
                registered_controllers: config
                    .controllers_with_device(provider.name(), &device.id)
                    .into_iter()
                    .map(|controller| controller.id.clone())
                    .collect(),
                notes,
            }
        })
        .collect();
    Ok((devices, warnings))
}

fn identity_note(identity: DeviceIdentity) -> &'static str {
    match identity {
        DeviceIdentity::XinputSlot => {
            "Identified by its XInput slot, which follows the order the controllers were connected in: use the port order binding."
        }
        DeviceIdentity::DirectInputInstance => {
            "The id follows the USB port unless the device has its own serial number. To use the device binding, connect it to another USB port and detect again: only if the id stays the same does it identify the device."
        }
        DeviceIdentity::SdlModel => "The id identifies the model: identical controllers share it.",
        DeviceIdentity::Other => "Nothing is known about how this id follows the device.",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controllers::model::{ControllerInput, ControllerKind};

    /// MAME 0.289 output with two GP100 and an F310 in X mode (`winhybrid`).
    const WINHYBRID: &str = "Video: Monitor 65537 = \"\\\\.\\DISPLAY1\" (primary)
Input: Adding keyboard #1: Standard PS/2 Keyboard (device id: \\\\?\\ACPI#KBD0001#1&0000000&0#{884b96c3})
Input: Adding mouse #1: HID-compliant mouse (device id: \\\\?\\HID#MOUSE0001&Col01#1&0000000&0&0000#{378de44c})
Input: Adding joystick #1: USB gamepad            (device id: USB gamepad            product_e401081f-0000-0000-0000-504944564944 instance_1a2b3c4d-1111-11f0-8001-444553540000)
DirectInput: Device type=0x14 subtype=0x01 HID=yes
Input: Adding joystick #2: USB gamepad            (device id: USB gamepad            product_e401081f-0000-0000-0000-504944564944 instance_5e6f7a8b-2222-11f1-8001-444553540000)
DirectInput: Device type=0x14 subtype=0x01 HID=yes
Skipping DirectInput for XInput compatible joystick 0x7e73379738.
XInput: Configuring player 1 type 0x01 (game controller) sub type 0x01 (gamepad).
Input: Adding joystick #3: XInput Player 1 (device id: XInput Player 1)
Starting Pong (Rev E) ':'
";

    /// The same devices with `sdljoy`: the two GP100 share their GUID.
    const SDLJOY: &str = "Input: Adding keyboard #1: Keyboard (device id: kbd)
Joystick: XInput Controller #1 [GUID 0300fa676d0400001dc2000014407801] Vendor ID 046D, Product ID C21D, Revision 4014, Serial <nullptr>
Input: Adding joystick #1: XInput Controller #1 (device id: 0300fa676d0400001dc2000014407801)
Joystick: (Periferiche standard di sistema) USB gamepad [GUID 0300007c1f08000001e4000000000000] Vendor ID 081F, Product ID E401, Revision 0000, Serial <nullptr>
Input: Adding joystick #2: (Periferiche standard di sistema) USB gamepad (device id: 0300007c1f08000001e4000000000000)
Joystick: (Periferiche standard di sistema) USB gamepad [GUID 0300007c1f08000001e4000000000000] Vendor ID 081F, Product ID E401, Revision 0000, Serial <nullptr>
Input: Adding joystick #3: (Periferiche standard di sistema) USB gamepad (device id: 0300007c1f08000001e4000000000000)
";

    #[test]
    fn detects_three_controllers_with_winhybrid() {
        let mut config = ControllersConfig::new();
        config
            .create_controller(ControllerInput {
                name: "GP100 port A".to_owned(),
                kind: ControllerKind::Gamepad,
                device_class: DeviceClass::Joystick,
                device_id: Some(
                    "usb gamepad            product_e401081f-0000-0000-0000-504944564944 instance_1a2b3c4d-1111-11f0-8001-444553540000"
                        .to_owned(),
                ),
                provider: Some("winhybrid".to_owned()),
                binding: Some(ControllerBinding::PortOrder),
            })
            .expect("controller");
        let (devices, warnings) =
            interpret(WINHYBRID, JoystickProvider::Winhybrid, &config).expect("interpreted");
        assert!(warnings.is_empty());
        assert_eq!(devices.len(), 3);
        let [first, second, xinput] = [&devices[0], &devices[1], &devices[2]];
        assert_eq!(first.name, "USB gamepad");
        assert_eq!(first.identity, DeviceIdentity::DirectInputInstance);
        assert_eq!(first.vendor_id.as_deref(), Some("081F"));
        assert_eq!(first.product_id.as_deref(), Some("E401"));
        assert!(first.distinguishable && second.distinguishable);
        assert_eq!(first.suggested_binding, Some(ControllerBinding::PortOrder));
        // Identical names: only their position tells them apart.
        assert!(
            first
                .notes
                .iter()
                .any(|note| note.contains("Swapping their USB ports"))
        );
        // Registered devices are recognized by provider and exact id only.
        assert_eq!(first.registered_controllers, ["gp100-port-a"]);
        assert!(second.registered_controllers.is_empty());
        assert_eq!(xinput.index, 3);
        assert_eq!(xinput.device_id, "XInput Player 1");
        assert_eq!(xinput.identity, DeviceIdentity::XinputSlot);
        assert_eq!(xinput.allowed_bindings, [ControllerBinding::PortOrder]);
        assert_eq!(xinput.serial, None);
        // The same id seen through another provider is another device id.
        let (devices, _) =
            interpret(WINHYBRID, JoystickProvider::Dinput, &config).expect("interpreted");
        assert!(devices[0].registered_controllers.is_empty());
    }

    #[test]
    fn identical_sdl_devices_cannot_be_told_apart() {
        let (devices, _) = interpret(SDLJOY, JoystickProvider::Sdljoy, &ControllersConfig::new())
            .expect("interpreted");
        assert_eq!(devices.len(), 3);
        assert!(devices[0].distinguishable);
        assert_eq!(devices[0].identity, DeviceIdentity::SdlModel);
        assert_eq!(devices[0].vendor_id.as_deref(), Some("046D"));
        for gp100 in &devices[1..] {
            assert!(!gp100.distinguishable);
            assert_eq!(gp100.suggested_binding, None);
            assert!(
                gp100
                    .notes
                    .iter()
                    .any(|note| note.contains("cannot tell them apart"))
            );
        }
    }

    #[test]
    fn single_device_and_no_device() {
        let single = "Input: Adding keyboard #1: Keyboard (device id: kbd)\nInput: Adding joystick #1: XInput Player 1 (device id: XInput Player 1)\n";
        let (devices, warnings) =
            interpret(single, JoystickProvider::Xinput, &ControllersConfig::new()).expect("ok");
        assert_eq!(devices.len(), 1);
        assert!(devices[0].distinguishable);
        assert!(warnings.is_empty());

        let none = "Input: Adding keyboard #1: Keyboard (device id: kbd)\n";
        let (devices, warnings) =
            interpret(none, JoystickProvider::Winhybrid, &ControllersConfig::new()).expect("ok");
        assert!(devices.is_empty());
        assert!(warnings[0].contains("No joystick"));
    }

    #[test]
    fn unrecognized_output_is_an_error_not_an_empty_list() {
        for output in [
            "",
            "Starting Pong (Rev E) ':'\n",
            "Input: Adding joystick without index\n",
        ] {
            assert!(matches!(
                interpret(
                    output,
                    JoystickProvider::Winhybrid,
                    &ControllersConfig::new()
                ),
                Err(DetectionError::NotRecognized(_))
            ));
        }
    }

    #[test]
    fn the_command_runs_pong_without_reading_ini_files() {
        let dir = Path::new("work");
        let command = detection_command(Path::new("mame"), JoystickProvider::Dinput, dir);
        let args: Vec<_> = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert_eq!(args[0], "pong");
        for expected in ["-noreadconfig", "-verbose", "-nothrottle"] {
            assert!(args.iter().any(|arg| arg == expected), "{expected}");
        }
        let provider = args
            .iter()
            .position(|arg| arg == "-joystickprovider")
            .expect("provider");
        assert_eq!(args[provider + 1], "dinput");
        let video = args.iter().position(|arg| arg == "-video").expect("video");
        assert_eq!(args[video + 1], "none");
        assert_eq!(command.get_current_dir(), Some(dir));
    }

    /// Runs a test program instead of MAME.
    fn fake_mame(dir: &Path, script: &str) -> PathBuf {
        if cfg!(windows) {
            let path = dir.join("fake-mame.cmd");
            fs::write(&path, format!("@echo off\r\n{script}\r\n")).expect("write");
            path
        } else {
            let path = dir.join("fake-mame.sh");
            fs::write(&path, format!("#!/bin/sh\n{script}\n")).expect("write");
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("chmod");
            }
            path
        }
    }

    #[test]
    fn failed_runs_are_errors_and_leave_no_temporary_directory() {
        let dir = tempfile::tempdir().expect("temp dir");
        let temp_root = dir.path().join("tmp");
        fs::create_dir(&temp_root).expect("dir");
        // Missing executable.
        let missing = dir.path().join("missing-mame");
        assert!(matches!(
            run(&missing, JoystickProvider::Winhybrid, &temp_root),
            Err(DetectionError::Failed(message)) if message.contains("could not be started")
        ));
        // Unexpected exit code, with the end of the output.
        let failing = fake_mame(dir.path(), "echo Unknown system 'pong'\nexit 5");
        let error = run(&failing, JoystickProvider::Winhybrid, &temp_root).unwrap_err();
        assert!(
            error.to_string().contains("does not know this machine"),
            "{error}"
        );
        assert!(error.to_string().contains("Unknown system"), "{error}");
        // Success: the output is returned.
        let ok = fake_mame(
            dir.path(),
            "echo Input: Adding joystick #1: Pad (device id: pad-1)",
        );
        let output = run(&ok, JoystickProvider::Winhybrid, &temp_root).expect("ran");
        assert!(output.contains("device id: pad-1"));
        assert_eq!(fs::read_dir(&temp_root).expect("read").count(), 0);
        // A process that does not finish is stopped. (The script's own child
        // process may keep its directory busy for a while on Windows.)
        let hanging = fake_mame(
            dir.path(),
            if cfg!(windows) {
                "ping -n 30 127.0.0.1 > nul"
            } else {
                "sleep 30"
            },
        );
        let error = run_within(
            &hanging,
            JoystickProvider::Winhybrid,
            &temp_root,
            Duration::from_secs(1),
        )
        .unwrap_err();
        assert!(error.to_string().contains("did not finish"), "{error}");
    }
}
