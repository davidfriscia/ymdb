//! Validation of MAME controller configuration files imported as the
//! `base.cfg` of a controller profile.
//!
//! The checks mirror what MAME requires to load the file (`config.cpp`,
//! `load_xml`): a well-formed XML document whose root is `<mameconfig>` with
//! the supported `version`, containing `<system name="...">` elements. MAME
//! stops with a fatal error (exit code 3) when no section of a controller
//! file applies to the running machine, so a baseline used for every machine
//! needs a `<system name="default">` section (in any position). Device
//! mappings (`<mapdevice>`) must name a valid controller (`JOYCODE_1`...):
//! MAME silently drops the other mappings of a section after an invalid one.
//! The inputs themselves are not interpreted and the file is copied verbatim.

use std::fmt;
use std::fs;
use std::io;
use std::path::Path;

use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

use crate::catalog::xml::for_each_attribute;

/// The configuration file version of MAME 0.162 to 0.289 (`CONFIG_VERSION`).
pub const SUPPORTED_VERSION: &str = "10";
/// Controller files distributed with MAME are a few kilobytes.
pub const MAX_FILE_SIZE: u64 = 1024 * 1024;
/// The section MAME applies to every machine.
pub const DEFAULT_SYSTEM: &str = "default";
/// MAME's `DEVICE_INDEX_MAXIMUM`: controller names use indexes 1 to 255.
const MAX_DEVICE_INDEX: u32 = 255;

/// The device classes of MAME controller names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SlotClass {
    Keyboard,
    Mouse,
    Lightgun,
    Joystick,
}

impl SlotClass {
    const ALL: [Self; 4] = [Self::Keyboard, Self::Mouse, Self::Lightgun, Self::Joystick];

    fn prefix(self) -> &'static str {
        match self {
            Self::Keyboard => "KEYCODE",
            Self::Mouse => "MOUSECODE",
            Self::Lightgun => "GUNCODE",
            Self::Joystick => "JOYCODE",
        }
    }
}

/// A MAME controller name such as `JOYCODE_2`: a device class and a 1-based
/// device index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ControllerSlot {
    pub class: SlotClass,
    pub index: u32,
}

impl ControllerSlot {
    pub fn joystick(index: u32) -> Self {
        Self {
            class: SlotClass::Joystick,
            index,
        }
    }

    /// Parses `<CLASS>_<index>` (`input.cpp`, `map_device_to_controller`).
    pub fn parse(text: &str) -> Option<Self> {
        let (prefix, index) = text.split_once('_')?;
        let class = SlotClass::ALL
            .into_iter()
            .find(|class| class.prefix().eq_ignore_ascii_case(prefix))?;
        if index.is_empty() || !index.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        let index: u32 = index.parse().ok()?;
        (1..=MAX_DEVICE_INDEX)
            .contains(&index)
            .then_some(Self { class, index })
    }
}

impl fmt::Display for ControllerSlot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}_{}", self.class.prefix(), self.index)
    }
}

/// A `<mapdevice>` element of a controller file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceMapping {
    /// The `name` of the `<system>` section it belongs to.
    pub system: String,
    /// The device id, which MAME matches as a case-insensitive substring.
    pub device: String,
    pub controller: ControllerSlot,
}

/// What composing a controller file needs to know about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControllerFileInfo {
    /// Byte offset right after the start tag of `<mameconfig>`, where a
    /// section can be inserted without touching the others.
    pub root_content_start: usize,
    /// Whether a `<system name="default">` section exists.
    pub has_default: bool,
    /// The names of the `<system>` sections, in order.
    pub systems: Vec<String>,
    /// Every `<mapdevice>` of the `<input>` of a section, in order.
    pub mappings: Vec<DeviceMapping>,
}

/// Reads and checks a controller configuration file; returns its exact content.
pub fn read(path: &Path) -> Result<Vec<u8>, String> {
    let metadata = fs::metadata(path).map_err(|error| match error.kind() {
        io::ErrorKind::NotFound => format!("the file '{}' does not exist", path.display()),
        _ => format!("cannot read '{}': {error}", path.display()),
    })?;
    if !metadata.is_file() {
        return Err(format!("'{}' is not a regular file", path.display()));
    }
    if metadata.len() > MAX_FILE_SIZE {
        return Err(format!(
            "'{}' is larger than {} KiB: it is not a controller configuration file",
            path.display(),
            MAX_FILE_SIZE / 1024
        ));
    }
    let content =
        fs::read(path).map_err(|error| format!("cannot read '{}': {error}", path.display()))?;
    validate(&content)?;
    Ok(content)
}

/// Checks a controller configuration file used as the baseline of a
/// profile: its structure, and a `default` section so that MAME can load it
/// for every machine.
pub fn validate(content: &[u8]) -> Result<ControllerFileInfo, String> {
    let info = analyze(content)?;
    if !info.has_default {
        return Err(no_default_message(&info.systems));
    }
    Ok(info)
}

fn no_default_message(systems: &[String]) -> String {
    let mut named = systems
        .iter()
        .take(5)
        .map(|name| format!("'{name}'"))
        .collect::<Vec<_>>()
        .join(", ");
    if systems.len() > 5 {
        named.push_str(", ...");
    }
    format!(
        "the file has no <system name=\"{DEFAULT_SYSTEM}\"> section, only sections for specific machines ({named}): MAME stops with a fatal error when it runs any other machine with it"
    )
}

/// Checks the structure of a controller configuration file and describes it.
pub fn analyze(content: &[u8]) -> Result<ControllerFileInfo, String> {
    if content.len() as u64 > MAX_FILE_SIZE {
        return Err("the file is too large for a controller configuration file".to_owned());
    }
    let mut reader = Reader::from_reader(content);
    let mut buf = Vec::new();
    // Names of the open elements, from the root.
    let mut path: Vec<String> = Vec::new();
    let mut root_seen = false;
    let mut sections = 0usize;
    let mut info = ControllerFileInfo {
        root_content_start: 0,
        has_default: false,
        systems: Vec::new(),
        mappings: Vec::new(),
    };
    loop {
        buf.clear();
        let event = reader.read_event_into(&mut buf).map_err(|error| {
            format!("malformed XML at byte {}: {error}", reader.error_position())
        })?;
        match event {
            Event::Start(_) | Event::Empty(_) if path.is_empty() && root_seen => {
                return Err("the document has more than one root element".to_owned());
            }
            Event::Start(ref element) | Event::Empty(ref element) => {
                let empty = matches!(event, Event::Empty(_));
                let name = element.name().as_ref().to_owned();
                match path.len() {
                    0 => {
                        check_root(element)?;
                        root_seen = true;
                        info.root_content_start =
                            usize::try_from(reader.buffer_position()).unwrap_or(usize::MAX);
                    }
                    1 if name == "system" => {
                        let system = check_system(element)?;
                        info.has_default |= system == DEFAULT_SYSTEM;
                        info.systems.push(system);
                    }
                    2 if path[1] == "system" && matches!(name.as_str(), "input" | "ui") => {
                        sections += 1;
                    }
                    3 if path[1] == "system" && path[2] == "input" && name == "mapdevice" => {
                        let system = info.systems.last().cloned().unwrap_or_default();
                        info.mappings.push(check_mapping(element, system)?);
                    }
                    _ => {}
                }
                if !empty {
                    path.push(name);
                }
            }
            Event::End(_) => {
                path.pop();
            }
            Event::Text(text) if path.is_empty() && !text.xml10_content().trim().is_empty() => {
                return Err("text outside the root element".to_owned());
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if !root_seen {
        return Err("the file is not an XML document".to_owned());
    }
    if !path.is_empty() {
        return Err("the document ends before its elements are closed".to_owned());
    }
    if info.systems.is_empty() || sections == 0 {
        return Err(
            "the file has no <system> element with inputs: it is not a controller configuration file"
                .to_owned(),
        );
    }
    Ok(info)
}

fn check_root(element: &BytesStart<'_>) -> Result<(), String> {
    if element.name().as_ref() != "mameconfig" {
        return Err(format!(
            "the root element is <{}>, not <mameconfig>: it is not a MAME configuration file",
            element.name().as_ref()
        ));
    }
    let mut version = None;
    for_each_attribute(element, |key, value| {
        if key == "version" {
            version = Some(value);
        }
        Ok(())
    })?;
    match version.as_deref() {
        Some(SUPPORTED_VERSION) => Ok(()),
        Some(other) => Err(format!(
            "unsupported configuration file version {other}: MAME only loads version {SUPPORTED_VERSION}"
        )),
        None => Err("<mameconfig> has no version".to_owned()),
    }
}

fn check_system(element: &BytesStart<'_>) -> Result<String, String> {
    let mut name = None;
    for_each_attribute(element, |key, value| {
        if key == "name" {
            name = Some(value);
        }
        Ok(())
    })?;
    match name {
        Some(name) if !name.trim().is_empty() => Ok(name),
        _ => Err("a <system> element has no name".to_owned()),
    }
}

fn check_mapping(element: &BytesStart<'_>, system: String) -> Result<DeviceMapping, String> {
    let (mut device, mut controller) = (None, None);
    for_each_attribute(element, |key, value| {
        match key {
            "device" => device = Some(value),
            "controller" => controller = Some(value),
            _ => {}
        }
        Ok(())
    })?;
    let device = device
        .filter(|device| !device.trim().is_empty())
        .ok_or_else(|| format!("a <mapdevice> of the section '{system}' has no device"))?;
    let controller = controller.unwrap_or_default();
    let slot = ControllerSlot::parse(&controller).ok_or_else(|| {
        format!(
            "the <mapdevice> of '{device}' in the section '{system}' names the invalid controller '{controller}' (expected JOYCODE_1, GUNCODE_2...): MAME would ignore the other device mappings of the section"
        )
    })?;
    Ok(DeviceMapping {
        system,
        device,
        controller: slot,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &str = r#"<?xml version="1.0"?>
<!-- Example controller file -->
<mameconfig version="10">
    <system name="default">
        <input>
            <port type="P1_BUTTON1">
                <newseq type="standard">KEYCODE_X</newseq>
            </port>
        </input>
    </system>
    <system name="neogeo">
        <input>
            <remap origcode="KEYCODE_Z" newcode="KEYCODE_A" />
        </input>
    </system>
</mameconfig>
"#;

    #[test]
    fn accepts_controller_configuration_files() {
        let info = validate(VALID.as_bytes()).expect("valid");
        assert!(info.has_default);
        assert_eq!(info.systems, ["default", "neogeo"]);
        assert!(info.mappings.is_empty());
        // The insertion point is right after the root start tag.
        assert!(VALID[..info.root_content_start].ends_with("<mameconfig version=\"10\">"));
        // Only a device mapping.
        let info = validate(br#"<mameconfig version="10"><system name="default"><input><mapdevice device="XInput Player 1" controller="JOYCODE_1"/></input></system></mameconfig>"#)
            .expect("valid");
        assert_eq!(
            info.mappings,
            [DeviceMapping {
                system: "default".to_owned(),
                device: "XInput Player 1".to_owned(),
                controller: ControllerSlot::joystick(1),
            }]
        );
    }

    #[test]
    fn the_default_section_may_come_after_machine_sections() {
        let content = r#"<mameconfig version="10">
            <system name="sf2"><input><port type="P1_BUTTON1"><newseq type="standard">JOYCODE_1_BUTTON3</newseq></port></input></system>
            <system name="neogeo"><input/></system>
            <system name="default"><input><mapdevice device="Pad &amp; Co" controller="GUNCODE_2"/></input></system>
        </mameconfig>"#;
        let info = validate(content.as_bytes()).expect("valid");
        assert_eq!(info.systems, ["sf2", "neogeo", "default"]);
        assert_eq!(info.mappings[0].device, "Pad & Co");
        assert_eq!(info.mappings[0].controller.to_string(), "GUNCODE_2");
    }

    #[test]
    fn files_for_specific_machines_only_are_not_baselines() {
        let content = r#"<mameconfig version="10">
            <system name="sf2"><input><port type="P1_BUTTON1"><newseq type="standard">KEYCODE_A</newseq></port></input></system>
            <system name="neogeo"><input/></system>
        </mameconfig>"#;
        // Structurally valid...
        let info = analyze(content.as_bytes()).expect("structure");
        assert!(!info.has_default);
        // ...but MAME would stop with exit code 3 for any other machine.
        let error = validate(content.as_bytes()).expect_err("no default");
        assert!(error.contains("no <system name=\"default\">"), "{error}");
        assert!(error.contains("'sf2', 'neogeo'"), "{error}");
    }

    #[test]
    fn rejects_invalid_device_mappings() {
        for (mapping, expected) in [
            (r#"<mapdevice controller="JOYCODE_1"/>"#, "has no device"),
            (
                r#"<mapdevice device=" " controller="JOYCODE_1"/>"#,
                "has no device",
            ),
            (r#"<mapdevice device="pad"/>"#, "invalid controller ''"),
            (
                r#"<mapdevice device="pad" controller="JOYCODE_0"/>"#,
                "JOYCODE_0",
            ),
            (
                r#"<mapdevice device="pad" controller="JOYCODE_256"/>"#,
                "JOYCODE_256",
            ),
            (
                r#"<mapdevice device="pad" controller="JOYCODE_X"/>"#,
                "JOYCODE_X",
            ),
            (
                r#"<mapdevice device="pad" controller="PADCODE_1"/>"#,
                "PADCODE_1",
            ),
            (
                r#"<mapdevice device="pad" controller="JOYCODE_+1"/>"#,
                "JOYCODE_+1",
            ),
        ] {
            let content = format!(
                r#"<mameconfig version="10"><system name="default"><input>{mapping}</input></system></mameconfig>"#
            );
            let error = validate(content.as_bytes()).expect_err(mapping);
            assert!(error.contains(expected), "{mapping}: {error}");
        }
        assert_eq!(
            ControllerSlot::parse("joycode_10"),
            Some(ControllerSlot::joystick(10))
        );
        assert_eq!(
            ControllerSlot::parse("MOUSECODE_255").map(|slot| slot.to_string()),
            Some("MOUSECODE_255".to_owned())
        );
    }

    #[test]
    fn rejects_invalid_files() {
        let cases = [
            ("", "not an XML document"),
            ("not xml at all", "outside the root"),
            (
                "<mameconfig version=\"10\"><system name=\"default\"><input>",
                "ends before",
            ),
            (
                "<mameconfig version=\"10\"><system name=\"default\"><input></system></mameconfig>",
                "malformed",
            ),
            ("<config version=\"10\"/>", "not <mameconfig>"),
            (
                "<mameconfig version=\"9\"><system name=\"default\"><input/></system></mameconfig>",
                "version 9",
            ),
            (
                "<mameconfig><system name=\"default\"><input/></system></mameconfig>",
                "no version",
            ),
            ("<mameconfig version=\"10\"></mameconfig>", "no <system>"),
            (
                "<mameconfig version=\"10\"><system><input/></system></mameconfig>",
                "no name",
            ),
            (
                "<mameconfig version=\"10\"><system name=\"default\"/></mameconfig>",
                "no <system>",
            ),
            (
                "<mameconfig version=\"10\"><system name=\"a\"><input/></system></mameconfig><x/>",
                "more than one root",
            ),
        ];
        for (content, expected) in cases {
            let error = validate(content.as_bytes()).expect_err(content);
            assert!(error.contains(expected), "{content}: {error}");
        }
    }

    #[test]
    fn reads_regular_files_of_reasonable_size_only() {
        let dir = tempfile::tempdir().expect("temp dir");
        let file = dir.path().join("Contrôleur ✓.cfg");
        fs::write(&file, VALID).expect("write");
        assert_eq!(read(&file).expect("read"), VALID.as_bytes());

        assert!(
            read(&dir.path().join("missing.cfg"))
                .unwrap_err()
                .contains("does not exist")
        );
        assert!(read(dir.path()).unwrap_err().contains("not a regular file"));
        let large = dir.path().join("large.cfg");
        let mut content = VALID.as_bytes().to_vec();
        content.resize(usize::try_from(MAX_FILE_SIZE).expect("size") + 1, b' ');
        fs::write(&large, content).expect("write");
        assert!(read(&large).unwrap_err().contains("larger"));
        // A machine-only file is refused on import, and never modified.
        let machine_only = dir.path().join("sf2.cfg");
        let content =
            r#"<mameconfig version="10"><system name="sf2"><input/></system></mameconfig>"#;
        fs::write(&machine_only, content).expect("write");
        assert!(read(&machine_only).unwrap_err().contains("default"));
        assert_eq!(fs::read_to_string(&machine_only).expect("read"), content);
    }
}
