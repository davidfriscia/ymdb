//! The lines of MAME's verbose output (`-verbose`) about input devices and
//! controller files, as printed by MAME 0.289:
//!
//! ```text
//! Input: Adding joystick #3: XInput Player 1 (device id: XInput Player 1)
//! Attempting to parse: ymdb-0123456789abcdef.cfg
//! Input: Remapped joystick #1: USB gamepad (device id: USB gamepad product_... instance_...)
//! Configuration file default.cfg not found
//! Fatal error: Could not load controller configuration file missing.cfg
//! Joystick: <name> [GUID <guid>] Vendor ID 046D, Product ID C21D, Revision 4014, Serial <nullptr>
//! ```
//!
//! Only these lines are interpreted; every other line is ignored.

/// A device named by an `Adding` or `Remapped` line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoggedDevice {
    /// MAME's device class name: `keyboard`, `mouse`, `lightgun`, `joystick`.
    pub class: String,
    /// The 1-based device index (`#n`).
    pub index: u32,
    pub name: String,
    pub id: String,
}

/// What an SDL joystick line tells about the next joystick added.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SdlJoystick {
    pub guid: String,
    pub vendor_id: String,
    pub product_id: String,
    /// `None` when SDL reports no serial number.
    pub serial: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputLogLine {
    Added(LoggedDevice),
    Remapped(LoggedDevice),
    /// MAME starts reading a configuration file (`<name>.cfg`).
    Parsing(String),
    /// MAME looked for a configuration file (`default.cfg`, `<machine>.cfg`)
    /// that does not exist; printed instead of `Parsing` for it.
    ConfigurationMissing(String),
    /// MAME stopped because it could not load a controller file.
    ControllerFileFailed(String),
    Sdl(SdlJoystick),
}

pub fn parse_line(line: &str) -> Option<InputLogLine> {
    let line = line.trim_end();
    if let Some(rest) = line.strip_prefix("Input: Adding ") {
        return parse_device(rest).map(InputLogLine::Added);
    }
    if let Some(rest) = line.strip_prefix("Input: Remapped ") {
        return parse_device(rest).map(InputLogLine::Remapped);
    }
    if let Some(file) = line.strip_prefix("Attempting to parse: ") {
        return Some(InputLogLine::Parsing(file.to_owned()));
    }
    if let Some(file) = line
        .strip_prefix("Configuration file ")
        .and_then(|rest| rest.strip_suffix(" not found"))
    {
        return Some(InputLogLine::ConfigurationMissing(file.to_owned()));
    }
    if let Some((_, file)) = line.split_once("Could not load controller configuration file ") {
        return Some(InputLogLine::ControllerFileFailed(file.to_owned()));
    }
    if let Some(rest) = line.strip_prefix("Joystick: ") {
        return parse_sdl(rest).map(InputLogLine::Sdl);
    }
    None
}

/// `<class> #<n>: <name> (device id: <id>)`.
fn parse_device(text: &str) -> Option<LoggedDevice> {
    let (class, rest) = text.split_once(" #")?;
    let (index, rest) = rest.split_once(": ")?;
    let (name, id) = rest.split_once(" (device id: ")?;
    let id = id.strip_suffix(')')?;
    if class.is_empty() || !class.bytes().all(|b| b.is_ascii_lowercase()) || id.is_empty() {
        return None;
    }
    Some(LoggedDevice {
        class: class.to_owned(),
        index: index.parse().ok().filter(|index| *index > 0)?,
        name: name.trim().to_owned(),
        id: id.to_owned(),
    })
}

/// `<name> [GUID <guid>] Vendor ID <v>, Product ID <p>, Revision <r>, Serial <s>`.
fn parse_sdl(text: &str) -> Option<SdlJoystick> {
    let (_, rest) = text.split_once(" [GUID ")?;
    let (guid, rest) = rest.split_once("] ")?;
    let vendor_id = rest.strip_prefix("Vendor ID ")?.split(',').next()?;
    let product_id = rest.split_once("Product ID ")?.1.split(',').next()?;
    let serial = rest.split_once(", Serial ")?.1.trim();
    Some(SdlJoystick {
        guid: guid.to_owned(),
        vendor_id: vendor_id.to_owned(),
        product_id: product_id.to_owned(),
        serial: (!serial.is_empty() && serial != "<nullptr>").then(|| serial.to_owned()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_device_lines() {
        assert_eq!(
            parse_line("Input: Adding joystick #3: XInput Player 1 (device id: XInput Player 1)\r"),
            Some(InputLogLine::Added(LoggedDevice {
                class: "joystick".to_owned(),
                index: 3,
                name: "XInput Player 1".to_owned(),
                id: "XInput Player 1".to_owned(),
            }))
        );
        // Names may contain parentheses; ids keep their inner spaces.
        let line = "Input: Remapped joystick #1: Controller (Gamepad F310) (device id: Controller (Gamepad F310) product_c21d046d-0000-0000-0000-504944564944 instance_9c0dd280)";
        let Some(InputLogLine::Remapped(device)) = parse_line(line) else {
            panic!("not parsed");
        };
        assert_eq!(device.index, 1);
        assert_eq!(device.name, "Controller (Gamepad F310)");
        assert_eq!(
            device.id,
            "Controller (Gamepad F310) product_c21d046d-0000-0000-0000-504944564944 instance_9c0dd280"
        );
        let Some(InputLogLine::Added(device)) = parse_line(
            "Input: Adding joystick #1: USB gamepad            (device id: USB gamepad            product_e401 instance_1a2b)",
        ) else {
            panic!("not parsed");
        };
        assert_eq!(device.name, "USB gamepad");
        assert_eq!(
            device.id,
            "USB gamepad            product_e401 instance_1a2b"
        );
    }

    #[test]
    fn parses_controller_file_lines() {
        assert_eq!(
            parse_line("Attempting to parse: ymdb-0011.cfg"),
            Some(InputLogLine::Parsing("ymdb-0011.cfg".to_owned()))
        );
        assert_eq!(
            parse_line("Configuration file default.cfg not found\r"),
            Some(InputLogLine::ConfigurationMissing("default.cfg".to_owned()))
        );
        assert_eq!(
            parse_line("Fatal error: Could not load controller configuration file other.cfg"),
            Some(InputLogLine::ControllerFileFailed("other.cfg".to_owned()))
        );
    }

    #[test]
    fn parses_sdl_joystick_lines() {
        assert_eq!(
            parse_line(
                "Joystick: XInput Controller #1 [GUID 0300fa676d0400001dc2000014407801] Vendor ID 046D, Product ID C21D, Revision 4014, Serial <nullptr>"
            ),
            Some(InputLogLine::Sdl(SdlJoystick {
                guid: "0300fa676d0400001dc2000014407801".to_owned(),
                vendor_id: "046D".to_owned(),
                product_id: "C21D".to_owned(),
                serial: None,
            }))
        );
        let Some(InputLogLine::Sdl(joystick)) = parse_line(
            "Joystick: Pad [GUID 0300] Vendor ID 046D, Product ID C216, Revision 1, Serial 0123ABCD",
        ) else {
            panic!("not parsed");
        };
        assert_eq!(joystick.serial.as_deref(), Some("0123ABCD"));
        assert_eq!(parse_line("Joystick:   ...  6 axes, 11 buttons"), None);
    }

    #[test]
    fn ignores_other_lines() {
        for line in [
            "",
            "Starting Pong (Rev E) ':'",
            "Input: Adding joystick: no index (device id: x)",
            "Input: Adding joystick #0: zero (device id: x)",
            "Input: Adding joystick #1: no id",
            "Input: Adding Joystick #1: upper (device id: x)",
            "DirectInput: Device type=0x14 subtype=0x01 HID=yes",
        ] {
            assert_eq!(parse_line(line), None, "{line}");
        }
    }
}
