//! MAME options that YMDB can set, and their validation.
//!
//! Only options listed in [`DEFINITIONS`] are accepted. Every other MAME option
//! stays under MAME's own configuration (`mame.ini`). The list is extended by
//! adding definitions; input options are intentionally absent (see
//! `ARCHITECTURE.md`, Launcher).

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

/// How an option is passed to MAME.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum OptionKind {
    /// `-name` or `-noname`, without a value.
    Boolean,
    /// `-name <integer>` within an inclusive range.
    Integer { min: i64, max: i64 },
    /// `-name <number>` within an inclusive range.
    Float { min: f64, max: f64 },
    /// `-name <word>`: letters, digits, `_`, `.` and `-` (not first).
    Identifier,
    /// `-name <path>`: one file or directory.
    Path,
    /// `-name <path;path;...>`: several directories in one argument.
    PathList,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum OptionCategory {
    Display,
    Performance,
    Sound,
    Paths,
    Machine,
    Session,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OptionDefinition {
    /// MAME option name, without the leading `-`.
    pub name: &'static str,
    pub kind: OptionKind,
    pub category: OptionCategory,
    pub label: &'static str,
    pub description: &'static str,
}

const fn define(
    name: &'static str,
    kind: OptionKind,
    category: OptionCategory,
    label: &'static str,
    description: &'static str,
) -> OptionDefinition {
    OptionDefinition {
        name,
        kind,
        category,
        label,
        description,
    }
}

use OptionCategory::{Display, Machine, Paths, Performance, Session, Sound};
use OptionKind::{Boolean, Identifier, Path, PathList};

/// Supported options, in the order they are passed to MAME. Names and value
/// ranges were checked against `mame -showusage` and MAME 0.280.
pub const DEFINITIONS: &[OptionDefinition] = &[
    define(
        "window",
        Boolean,
        Display,
        "Window mode",
        "Run in a window instead of full screen.",
    ),
    define(
        "maximize",
        Boolean,
        Display,
        "Maximize window",
        "Start with a maximized window.",
    ),
    define(
        "keepaspect",
        Boolean,
        Display,
        "Keep aspect ratio",
        "Keep the aspect ratio when scaling.",
    ),
    define(
        "filter",
        Boolean,
        Display,
        "Bilinear filtering",
        "Smooth the image when scaling.",
    ),
    define(
        "waitvsync",
        Boolean,
        Display,
        "Wait for vertical sync",
        "Reduce tearing by waiting for VBLANK.",
    ),
    define(
        "syncrefresh",
        Boolean,
        Display,
        "Sync to refresh rate",
        "Throttle to the monitor refresh rate.",
    ),
    define(
        "video",
        Identifier,
        Display,
        "Video output",
        "Video backend, for example d3d, bgfx, opengl, gdi or none.",
    ),
    define(
        "throttle",
        Boolean,
        Performance,
        "Throttle",
        "Keep the emulation in sync with real time.",
    ),
    define(
        "speed",
        OptionKind::Float {
            min: 0.01,
            max: 100.0,
        },
        Performance,
        "Speed",
        "Emulation speed relative to real time (1 = normal).",
    ),
    define(
        "frameskip",
        OptionKind::Integer { min: 0, max: 10 },
        Performance,
        "Frame skip",
        "Fixed number of frames to skip (0-10).",
    ),
    define(
        "autoframeskip",
        Boolean,
        Performance,
        "Automatic frame skip",
        "Skip frames to keep full speed.",
    ),
    define(
        "sound",
        Identifier,
        Sound,
        "Sound output",
        "Sound backend, for example wasapi, xaudio2, portaudio or none.",
    ),
    define(
        "volume",
        OptionKind::Integer { min: -96, max: 12 },
        Sound,
        "Volume",
        "Sound volume in decibels (-96 to 12).",
    ),
    define(
        "rompath",
        PathList,
        Paths,
        "ROM paths",
        "Directories searched for ROM sets and CHDs.",
    ),
    define(
        "samplepath",
        PathList,
        Paths,
        "Sample paths",
        "Directories searched for audio samples.",
    ),
    define(
        "artpath",
        PathList,
        Paths,
        "Artwork paths",
        "Directories searched for artwork.",
    ),
    define(
        "hashpath",
        PathList,
        Paths,
        "Hash paths",
        "Directories searched for software list definitions.",
    ),
    define(
        "cfg_directory",
        Path,
        Paths,
        "Configuration directory",
        "Where MAME saves per-machine configuration.",
    ),
    define(
        "nvram_directory",
        Path,
        Paths,
        "NVRAM directory",
        "Where MAME saves NVRAM contents.",
    ),
    define(
        "state_directory",
        Path,
        Paths,
        "Save state directory",
        "Where MAME saves states.",
    ),
    define(
        "snapshot_directory",
        Path,
        Paths,
        "Snapshot directory",
        "Where MAME saves screenshots.",
    ),
    define(
        "bios",
        Identifier,
        Machine,
        "BIOS",
        "BIOS set of the machine, by short name.",
    ),
    define(
        "ramsize",
        Identifier,
        Machine,
        "RAM size",
        "RAM size, for machines with RAM options (for example 64K).",
    ),
    define(
        "skip_gameinfo",
        Boolean,
        Session,
        "Skip information screen",
        "Do not show the machine information screen.",
    ),
    define(
        "autosave",
        Boolean,
        Session,
        "Auto save",
        "Restore the state on start and save it on exit, where supported.",
    ),
    define(
        "rewind",
        Boolean,
        Session,
        "Rewind",
        "Keep rewind save states.",
    ),
    define(
        "cheat",
        Boolean,
        Session,
        "Cheats",
        "Enable the cheat subsystem.",
    ),
    define(
        "seconds_to_run",
        OptionKind::Integer {
            min: 1,
            max: 86_400,
        },
        Session,
        "Seconds to run",
        "Exit automatically after this many emulated seconds.",
    ),
];

pub fn definition(name: &str) -> Option<&'static OptionDefinition> {
    DEFINITIONS
        .iter()
        .find(|definition| definition.name == name)
}

/// A value set by the user: a boolean, a number, a text (identifier or path)
/// or a list of paths.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OptionValue {
    Boolean(bool),
    Number(f64),
    Text(String),
    List(Vec<String>),
}

/// Options explicitly set at one configuration level, by MAME option name.
pub type OptionSet = BTreeMap<String, OptionValue>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptionError {
    pub option: String,
    pub message: String,
}

impl fmt::Display for OptionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "option '{}': {}", self.option, self.message)
    }
}

impl std::error::Error for OptionError {}

const MAX_TEXT_LENGTH: usize = 4096;
const MAX_IDENTIFIER_LENGTH: usize = 64;
const MAX_PATHS: usize = 32;

/// Checks that every option is supported and has a valid value.
pub fn validate_set(options: &OptionSet) -> Result<(), OptionError> {
    for (name, value) in options {
        validate(name, value)?;
    }
    Ok(())
}

pub fn validate(name: &str, value: &OptionValue) -> Result<(), OptionError> {
    let error = |message: String| OptionError {
        option: name.to_owned(),
        message,
    };
    let definition =
        definition(name).ok_or_else(|| error("is not a supported option".to_owned()))?;
    match (definition.kind, value) {
        (OptionKind::Boolean, OptionValue::Boolean(_)) => Ok(()),
        (OptionKind::Integer { min, max }, OptionValue::Number(number)) => {
            #[allow(clippy::cast_precision_loss)]
            let in_range = *number >= min as f64 && *number <= max as f64;
            if number.fract() != 0.0 || !in_range {
                return Err(error(format!("must be an integer from {min} to {max}")));
            }
            Ok(())
        }
        (OptionKind::Float { min, max }, OptionValue::Number(number)) => {
            if !number.is_finite() || *number < min || *number > max {
                return Err(error(format!("must be a number from {min} to {max}")));
            }
            Ok(())
        }
        (OptionKind::Identifier, OptionValue::Text(text)) => {
            let valid = !text.is_empty()
                && text.len() <= MAX_IDENTIFIER_LENGTH
                && !text.starts_with('-')
                && text
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'));
            if valid {
                Ok(())
            } else {
                Err(error(
                    "must be a word of letters, digits, '_', '.' or '-' (not first)".to_owned(),
                ))
            }
        }
        (OptionKind::Path, OptionValue::Text(path)) => validate_path(path).map_err(error),
        (OptionKind::PathList, OptionValue::List(paths)) => {
            if paths.is_empty() || paths.len() > MAX_PATHS {
                return Err(error(format!("must contain 1 to {MAX_PATHS} paths")));
            }
            paths
                .iter()
                .try_for_each(|path| validate_path(path))
                .map_err(error)
        }
        _ => Err(error(format!(
            "has a value of the wrong type for a {} option",
            kind_name(definition.kind)
        ))),
    }
}

fn kind_name(kind: OptionKind) -> &'static str {
    match kind {
        OptionKind::Boolean => "boolean",
        OptionKind::Integer { .. } => "integer",
        OptionKind::Float { .. } => "number",
        OptionKind::Identifier => "identifier",
        OptionKind::Path => "path",
        OptionKind::PathList => "path list",
    }
}

fn validate_path(path: &str) -> Result<(), String> {
    if path.trim().is_empty() || path.len() > MAX_TEXT_LENGTH {
        return Err("paths must not be empty".to_owned());
    }
    if path.starts_with('-') {
        return Err("paths must not start with '-'".to_owned());
    }
    if path.contains(';') {
        return Err("paths must not contain ';' (MAME uses it to separate paths)".to_owned());
    }
    if path.chars().any(char::is_control) {
        return Err("paths must not contain control characters".to_owned());
    }
    Ok(())
}

/// The MAME arguments for a validated option, for example `["-nowindow"]` or
/// `["-rompath", "roms;D:/MAME ROMs"]`.
pub fn to_arguments(name: &str, value: &OptionValue) -> Vec<String> {
    match value {
        OptionValue::Boolean(true) => vec![format!("-{name}")],
        OptionValue::Boolean(false) => vec![format!("-no{name}")],
        OptionValue::Number(number) => vec![format!("-{name}"), format_number(*number)],
        OptionValue::Text(text) => vec![format!("-{name}"), text.clone()],
        OptionValue::List(paths) => vec![format!("-{name}"), paths.join(";")],
    }
}

/// Integers without a fractional part, other numbers in plain decimal notation.
fn format_number(number: f64) -> String {
    if number.fract() == 0.0 && number.abs() < 1e15 {
        #[allow(clippy::cast_possible_truncation)]
        let integer = number as i64;
        integer.to_string()
    } else {
        number.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(value: &str) -> OptionValue {
        OptionValue::Text(value.to_owned())
    }

    #[test]
    fn definitions_have_unique_valid_names() {
        let mut names: Vec<_> = DEFINITIONS
            .iter()
            .map(|definition| definition.name)
            .collect();
        let count = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), count);
        assert!(
            names
                .iter()
                .all(|name| name.chars().all(|c| c.is_ascii_lowercase() || c == '_'))
        );
        // Input options belong to the future input devices feature.
        for input in ["joystick", "mouse", "ctrlr", "lightgun"] {
            assert!(definition(input).is_none(), "{input}");
        }
    }

    #[test]
    fn formats_each_kind_of_option() {
        assert_eq!(
            to_arguments("window", &OptionValue::Boolean(true)),
            ["-window"]
        );
        assert_eq!(
            to_arguments("window", &OptionValue::Boolean(false)),
            ["-nowindow"]
        );
        assert_eq!(
            to_arguments("volume", &OptionValue::Number(-12.0)),
            ["-volume", "-12"]
        );
        assert_eq!(
            to_arguments("speed", &OptionValue::Number(1.5)),
            ["-speed", "1.5"]
        );
        assert_eq!(to_arguments("video", &text("bgfx")), ["-video", "bgfx"]);
        assert_eq!(
            to_arguments(
                "rompath",
                &OptionValue::List(vec![
                    "roms".to_owned(),
                    "D:/Émulateurs MAME ✓/roms".to_owned()
                ])
            ),
            ["-rompath", "roms;D:/Émulateurs MAME ✓/roms"]
        );
    }

    #[test]
    fn accepts_valid_values() {
        for (name, value) in [
            ("window", OptionValue::Boolean(false)),
            ("volume", OptionValue::Number(-96.0)),
            ("volume", OptionValue::Number(12.0)),
            ("speed", OptionValue::Number(0.5)),
            ("bios", text("asia-s3")),
            ("ramsize", text("64K")),
            ("nvram_directory", text("C:\\Users\\Me\\MAME NVRAM ✓")),
            (
                "rompath",
                OptionValue::List(vec!["roms".to_owned(), "/srv/mame roms".to_owned()]),
            ),
        ] {
            assert_eq!(validate(name, &value), Ok(()), "{name}");
        }
    }

    #[test]
    fn rejects_unknown_options_and_invalid_values() {
        let invalid = [
            ("nosuchoption", OptionValue::Boolean(true)),
            ("joystick", OptionValue::Boolean(true)),
            ("window", text("yes")),
            ("volume", OptionValue::Number(-100.0)),
            ("volume", OptionValue::Number(1.5)),
            ("frameskip", OptionValue::Number(11.0)),
            ("speed", OptionValue::Number(f64::NAN)),
            ("speed", OptionValue::Number(0.0)),
            ("video", text("")),
            ("video", text("-window")),
            ("video", text("d3d; rm -rf /")),
            ("bios", text("x".repeat(65).as_str())),
            ("nvram_directory", text("-cfg_directory")),
            ("nvram_directory", text("a;b")),
            ("nvram_directory", text("line\nbreak")),
            ("nvram_directory", text("   ")),
            ("rompath", OptionValue::List(Vec::new())),
            ("rompath", text("roms")),
            (
                "rompath",
                OptionValue::List(vec!["roms".to_owned(), "x;y".to_owned()]),
            ),
        ];
        for (name, value) in invalid {
            assert!(validate(name, &value).is_err(), "{name} = {value:?}");
        }
    }

    #[test]
    fn values_round_trip_through_json() {
        let options: OptionSet = serde_json::from_str(
            r#"{ "window": true, "volume": -6, "speed": 1.25, "video": "bgfx", "rompath": ["roms", "D:/ROMs"] }"#,
        )
        .expect("parse");
        validate_set(&options).expect("valid");
        assert_eq!(options["volume"], OptionValue::Number(-6.0));
        let json = serde_json::to_value(&options).expect("serialize");
        assert_eq!(json["rompath"], serde_json::json!(["roms", "D:/ROMs"]));
        assert_eq!(json["window"], serde_json::json!(true));
    }
}
