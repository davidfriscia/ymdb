//! Validation of MAME controller configuration files imported as the
//! `base.cfg` of a controller profile.
//!
//! The checks mirror what MAME requires to load the file (`config.cpp`,
//! `load_xml`): a well-formed XML document whose root is `<mameconfig>` with
//! the supported `version`, containing `<system name="...">` elements. The
//! inputs themselves are not interpreted and the file is copied verbatim.

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

/// Checks the structure of a controller configuration file.
pub fn validate(content: &[u8]) -> Result<(), String> {
    if content.len() as u64 > MAX_FILE_SIZE {
        return Err("the file is too large for a controller configuration file".to_owned());
    }
    let mut reader = Reader::from_reader(content);
    let mut buf = Vec::new();
    // Names of the open elements, from the root.
    let mut path: Vec<String> = Vec::new();
    let mut root_seen = false;
    let mut systems = 0usize;
    let mut sections = 0usize;
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
                    }
                    1 if name == "system" => {
                        check_system(element)?;
                        systems += 1;
                    }
                    2 if path[1] == "system" && matches!(name.as_str(), "input" | "ui") => {
                        sections += 1;
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
    if systems == 0 || sections == 0 {
        return Err(
            "the file has no <system> element with inputs: it is not a controller configuration file"
                .to_owned(),
        );
    }
    Ok(())
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

fn check_system(element: &BytesStart<'_>) -> Result<(), String> {
    let mut name = None;
    for_each_attribute(element, |key, value| {
        if key == "name" {
            name = Some(value);
        }
        Ok(())
    })?;
    match name {
        Some(name) if !name.trim().is_empty() => Ok(()),
        _ => Err("a <system> element has no name".to_owned()),
    }
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
        validate(VALID.as_bytes()).expect("valid");
        // Only a device mapping.
        validate(br#"<mameconfig version="10"><system name="default"><input><mapdevice device="XInput Player 1" controller="JOYCODE_1"/></input></system></mameconfig>"#)
            .expect("valid");
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
    }
}
