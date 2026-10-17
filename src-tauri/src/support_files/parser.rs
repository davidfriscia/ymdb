//! Parsers of the MAME support files.
//!
//! The files share an INI-like syntax but not a structure:
//!
//! - `catver.ini` has a `[Category]` section of `machine=Category / Sub`
//!   lines and a `[VerAdded]` section of `machine=0.123` lines. Only these
//!   two sections are read: `VerAdded` is an attribute, not a category.
//! - `genre.ini`, `catlist.ini` and `series.ini` have one section per value,
//!   listing machine short names, one per line. A machine may be listed in
//!   several sections (several series, for example).
//! - `version.ini` has one section per MAME version listing the machines
//!   added in it; a machine has one version.
//!
//! `[FOLDER_SETTINGS]` and `[ROOT_FOLDER]` hold settings of the frontends the
//! files were written for and are ignored. Lines starting with `;` or `#` are
//! comments; the first comment usually names the file and its version, for
//! example `;; catver.ini 0.288 / 09-Jun-26 / MAME 0.288 ;;`.
//!
//! The format is identified from that header, then from the structure (only
//! `catver.ini` has `key=value` sections; only `version.ini` has version
//! sections), and only as a last resort from the file name. Files the parser
//! cannot identify are rejected rather than guessed.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

use super::model::{
    Attribute, Dimension, Membership, ParsedSupportFile, SupportFileFormat, WarningKind, Warnings,
};
use super::version;
use crate::catalog::is_short_name;

/// Largest accepted file (`catver.ini` of MAME 0.288 is 2.5 MB).
pub const MAX_FILE_SIZE: u64 = 64 * 1024 * 1024;

/// Sections written for frontends, not classifications.
const ROOT_FOLDER: &str = "ROOT_FOLDER";
const FRONTEND_SECTIONS: [&str; 2] = ["FOLDER_SETTINGS", ROOT_FOLDER];
const CATVER_CATEGORY: &str = "Category";
const CATVER_VERSION: &str = "VerAdded";

#[derive(Debug)]
pub enum ParseError {
    Io(io::Error),
    TooLarge {
        size: u64,
    },
    /// The header names a file that YMDB does not import (such as `mature.ini`).
    Unsupported {
        name: String,
    },
    /// The format cannot be determined reliably.
    Unidentified,
    /// The content does not match the identified format.
    Invalid {
        format: SupportFileFormat,
        message: String,
    },
    /// The file contains no usable entry.
    Empty {
        format: SupportFileFormat,
    },
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "the file could not be read: {error}"),
            Self::TooLarge { size } => write!(
                f,
                "the file is too large ({size} bytes, at most {MAX_FILE_SIZE} bytes)"
            ),
            Self::Unsupported { name } => write!(
                f,
                "'{name}' is not a supported support file (supported: catver.ini, genre.ini, \
                 catlist.ini, version.ini, series.ini)"
            ),
            Self::Unidentified => write!(
                f,
                "the file is not a recognized support file: its header does not name it and \
                 its structure does not identify it"
            ),
            Self::Invalid { format, message } => write!(f, "invalid {format}: {message}"),
            Self::Empty { format } => write!(f, "the {format} file contains no machine entries"),
        }
    }
}

impl std::error::Error for ParseError {}

/// Reads and parses a support file. The file name is only a hint, used when
/// neither the header nor the structure identifies the format.
pub fn read_file(path: &Path) -> Result<ParsedSupportFile, ParseError> {
    let file = File::open(path).map_err(ParseError::Io)?;
    let size = file.metadata().map_err(ParseError::Io)?.len();
    if size > MAX_FILE_SIZE {
        return Err(ParseError::TooLarge { size });
    }
    let mut content = Vec::new();
    file.take(MAX_FILE_SIZE + 1)
        .read_to_end(&mut content)
        .map_err(ParseError::Io)?;
    let name = path.file_name().and_then(|name| name.to_str());
    parse(&content, name)
}

/// Parses the content of a support file.
pub fn parse(content: &[u8], file_name: Option<&str>) -> Result<ParsedSupportFile, ParseError> {
    let mut warnings = Warnings::default();
    let text = decode(content, &mut warnings);
    let document = Document::read(&text, &mut warnings);
    let format = identify(&document, file_name)?;
    let mut parsed = match format {
        SupportFileFormat::Catver => parse_catver(&document, &mut warnings)?,
        SupportFileFormat::Version => parse_version(&document, &mut warnings),
        SupportFileFormat::Genre | SupportFileFormat::Catlist | SupportFileFormat::Series => {
            parse_sections(&document, format, &mut warnings)
        }
    };
    if parsed.memberships.is_empty() && parsed.attributes.is_empty() {
        return Err(ParseError::Empty { format });
    }
    // `[FOLDER_SETTINGS]` always holds frontend settings; machines listed in
    // `[ROOT_FOLDER]` are not classified, so they are reported.
    for section in &document.sections {
        if section.name.eq_ignore_ascii_case(ROOT_FOLDER) && !section.entries.is_empty() {
            warnings.add(WarningKind::IgnoredSection, || {
                format!("line {}: [{}]", section.line, section.name)
            });
        }
    }
    parsed.memberships.sort();
    parsed.attributes.sort();
    parsed.declared_version = document.header.and_then(|header| header.version);
    parsed.warnings = warnings.into_vec();
    Ok(parsed)
}

/// UTF-8 (with or without BOM), else Windows-1252, the encoding of older
/// files written on Windows.
fn decode(content: &[u8], warnings: &mut Warnings) -> String {
    let content = content.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(content);
    match std::str::from_utf8(content) {
        Ok(text) => text.to_owned(),
        Err(error) => {
            warnings.add(WarningKind::NotUtf8, || {
                format!("invalid UTF-8 at byte {}", error.valid_up_to())
            });
            content.iter().map(|&byte| windows_1252(byte)).collect()
        }
    }
}

fn windows_1252(byte: u8) -> char {
    const HIGH: [char; 32] = [
        '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8D}', 'Ž',
        '\u{8F}', '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9D}',
        'ž', 'Ÿ',
    ];
    match byte {
        0x80..=0x9F => HIGH[usize::from(byte - 0x80)],
        _ => char::from(byte),
    }
}

/// The header comment: the file it names and the version it declares.
struct Header {
    /// Lowercase name without `.ini`, such as `catver`.
    name: String,
    /// The name as written, for messages.
    display: String,
    version: Option<String>,
}

struct Section {
    name: String,
    line: usize,
    /// `(line number, trimmed text)`.
    entries: Vec<(usize, String)>,
}

struct Document {
    header: Option<Header>,
    sections: Vec<Section>,
}

impl Document {
    fn read(text: &str, warnings: &mut Warnings) -> Self {
        let mut header = None;
        let mut sections: Vec<Section> = Vec::new();
        // `\n` and `\r\n` line endings, or `\r` alone in old Mac files.
        let lines: Vec<&str> = if text.contains('\n') {
            text.lines().collect()
        } else {
            text.split('\r').collect()
        };
        for (index, line) in lines.into_iter().enumerate() {
            let number = index + 1;
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if line.starts_with(';') || line.starts_with('#') {
                if header.is_none() {
                    header = parse_header(line);
                }
                continue;
            }
            if let Some(name) = line
                .strip_prefix('[')
                .and_then(|rest| rest.strip_suffix(']'))
            {
                sections.push(Section {
                    name: name.trim().to_owned(),
                    line: number,
                    entries: Vec::new(),
                });
                continue;
            }
            match sections.last_mut() {
                Some(section) => section.entries.push((number, line.to_owned())),
                None => warnings.add(WarningKind::EntriesOutsideSection, || {
                    format!("line {number}: {line}")
                }),
            }
        }
        Self { header, sections }
    }

    /// Sections holding classification data, in file order.
    fn data_sections(&self) -> impl Iterator<Item = &Section> {
        self.sections
            .iter()
            .filter(|section| !is_frontend_section(&section.name))
    }

    fn section(&self, name: &str) -> Option<&Section> {
        self.sections
            .iter()
            .find(|section| section.name.eq_ignore_ascii_case(name))
    }
}

fn is_frontend_section(name: &str) -> bool {
    FRONTEND_SECTIONS
        .iter()
        .any(|section| section.eq_ignore_ascii_case(name))
}

/// `;; GENRE.ini 0.288 / 09-Jun-26 / MAME 0.288 ;;` names `genre`, version `0.288`.
fn parse_header(line: &str) -> Option<Header> {
    let text = line.trim_matches(|c: char| c == ';' || c == '#' || c.is_whitespace());
    let mut tokens = text.split_whitespace();
    let display = tokens.next()?;
    let lower = display.to_ascii_lowercase();
    let name = lower.strip_suffix(".ini")?;
    if name.is_empty() {
        return None;
    }
    let rest: Vec<&str> = tokens.collect();
    let version = rest
        .first()
        .filter(|token| version::is_version(token))
        .or_else(|| {
            rest.windows(2)
                .find(|pair| pair[0].eq_ignore_ascii_case("MAME") && version::is_version(pair[1]))
                .map(|pair| &pair[1])
        })
        .map(|token| (*token).to_owned());
    Some(Header {
        name: name.to_owned(),
        display: display.to_owned(),
        version,
    })
}

fn identify(document: &Document, file_name: Option<&str>) -> Result<SupportFileFormat, ParseError> {
    let format = match &document.header {
        Some(header) => {
            Some(
                format_named(&header.name).ok_or_else(|| ParseError::Unsupported {
                    name: header.display.clone(),
                })?,
            )
        }
        None => None,
    };
    let structural = structural_format(document);
    let format = match (format, structural) {
        (Some(format), _) => format,
        (None, Some(format)) => format,
        (None, None) => file_name
            .and_then(|name| {
                let lower = name.to_ascii_lowercase();
                let stem = lower.strip_suffix(".ini").unwrap_or(&lower).to_owned();
                format_named(&stem)
            })
            .ok_or(ParseError::Unidentified)?,
    };
    // The header or the file name must agree with the structure.
    let has_catver_sections =
        document.section(CATVER_CATEGORY).is_some() || document.section(CATVER_VERSION).is_some();
    if format == SupportFileFormat::Catver && !has_catver_sections {
        return Err(ParseError::Invalid {
            format,
            message: "it has neither a [Category] nor a [VerAdded] section".to_owned(),
        });
    }
    if format != SupportFileFormat::Catver && structural == Some(SupportFileFormat::Catver) {
        return Err(ParseError::Invalid {
            format,
            message: "it has the structure of catver.ini".to_owned(),
        });
    }
    Ok(format)
}

fn format_named(name: &str) -> Option<SupportFileFormat> {
    SupportFileFormat::ALL
        .into_iter()
        .find(|format| format.as_str() == name)
}

/// The format implied by the structure alone, when it is unambiguous.
fn structural_format(document: &Document) -> Option<SupportFileFormat> {
    let key_values = |name| {
        document
            .section(name)
            .is_some_and(|section| section.entries.iter().any(|(_, text)| text.contains('=')))
    };
    if key_values(CATVER_CATEGORY) || key_values(CATVER_VERSION) {
        return Some(SupportFileFormat::Catver);
    }
    let mut sections = document.data_sections().peekable();
    if sections.peek().is_some() && sections.all(|section| version::is_version(&section.name)) {
        return Some(SupportFileFormat::Version);
    }
    None
}

fn parse_catver(
    document: &Document,
    warnings: &mut Warnings,
) -> Result<ParsedSupportFile, ParseError> {
    let format = SupportFileFormat::Catver;
    let category = document.section(CATVER_CATEGORY);
    let added = document.section(CATVER_VERSION);
    for (section, name) in [(category, CATVER_CATEGORY), (added, CATVER_VERSION)] {
        if section.is_none() {
            warnings.add(WarningKind::MissingSection, || format!("[{name}]"));
        }
    }
    for section in document.data_sections() {
        let known = [CATVER_CATEGORY, CATVER_VERSION]
            .iter()
            .any(|name| section.name.eq_ignore_ascii_case(name));
        if !known && !section.entries.is_empty() {
            warnings.add(WarningKind::IgnoredSection, || {
                format!("line {}: [{}]", section.line, section.name)
            });
        }
    }
    let memberships = category
        .map(|section| key_values(section, warnings))
        .unwrap_or_default()
        .into_iter()
        .map(|(machine, value)| Membership {
            dimension: Dimension::Category,
            value,
            machine,
        })
        .collect();
    let attributes: Vec<Attribute> = added
        .map(|section| key_values(section, warnings))
        .unwrap_or_default()
        .into_iter()
        .map(|(machine, value)| Attribute {
            dimension: Dimension::VersionAdded,
            machine,
            value,
        })
        .collect();
    check_versions(&attributes, warnings);
    if category.is_none() && added.is_none() {
        return Err(ParseError::Invalid {
            format,
            message: "it has neither a [Category] nor a [VerAdded] section".to_owned(),
        });
    }
    Ok(ParsedSupportFile {
        format,
        declared_version: None,
        memberships,
        attributes,
        warnings: Vec::new(),
    })
}

/// `machine=value` lines; the first value of a machine wins.
fn key_values(section: &Section, warnings: &mut Warnings) -> BTreeMap<String, String> {
    let mut values: BTreeMap<String, String> = BTreeMap::new();
    for (line, text) in &section.entries {
        let Some((machine, value)) = text.split_once('=') else {
            warnings.add(WarningKind::InvalidLine, || format!("line {line}: {text}"));
            continue;
        };
        let (machine, value) = (machine.trim(), value.trim());
        if value.is_empty() {
            warnings.add(WarningKind::InvalidLine, || format!("line {line}: {text}"));
            continue;
        }
        if !is_short_name(machine) {
            warnings.add(WarningKind::InvalidMachineName, || {
                format!("line {line}: {machine}")
            });
            continue;
        }
        match values.get(machine) {
            Some(existing) if existing == value => {
                warnings.add(WarningKind::DuplicateEntry, || {
                    format!("line {line}: {text}")
                });
            }
            Some(_) => warnings.add(WarningKind::ConflictingValue, || {
                format!("line {line}: {text}")
            }),
            None => {
                values.insert(machine.to_owned(), value.to_owned());
            }
        }
    }
    values
}

/// Sections listing machine names, one value per section.
fn parse_sections(
    document: &Document,
    format: SupportFileFormat,
    warnings: &mut Warnings,
) -> ParsedSupportFile {
    // Genre, Category or Series: the single dimension of these formats.
    let dimension = format.dimensions()[0];
    let mut memberships = BTreeSet::new();
    for section in document.data_sections() {
        if section.name.is_empty() {
            warnings.add(WarningKind::InvalidLine, || {
                format!("line {}: []", section.line)
            });
            continue;
        }
        for machine in section_machines(section, warnings) {
            let membership = Membership {
                dimension,
                value: section.name.clone(),
                machine: machine.to_owned(),
            };
            if !memberships.insert(membership) {
                warnings.add(WarningKind::DuplicateEntry, || {
                    format!("[{}] {machine}", section.name)
                });
            }
        }
    }
    ParsedSupportFile {
        format,
        declared_version: None,
        memberships: memberships.into_iter().collect(),
        attributes: Vec::new(),
        warnings: Vec::new(),
    }
}

/// `version.ini`: the section names the version a machine was added in.
fn parse_version(document: &Document, warnings: &mut Warnings) -> ParsedSupportFile {
    let mut versions: BTreeMap<String, String> = BTreeMap::new();
    for section in document.data_sections() {
        for machine in section_machines(section, warnings) {
            match versions.get(machine) {
                Some(existing) if *existing == section.name => {
                    warnings.add(WarningKind::DuplicateEntry, || {
                        format!("[{}] {machine}", section.name)
                    });
                }
                Some(_) => warnings.add(WarningKind::ConflictingValue, || {
                    format!("[{}] {machine}", section.name)
                }),
                None => {
                    versions.insert(machine.to_owned(), section.name.clone());
                }
            }
        }
    }
    let attributes: Vec<Attribute> = versions
        .into_iter()
        .map(|(machine, value)| Attribute {
            dimension: Dimension::VersionAdded,
            machine,
            value,
        })
        .collect();
    check_versions(&attributes, warnings);
    ParsedSupportFile {
        format: SupportFileFormat::Version,
        declared_version: None,
        memberships: Vec::new(),
        attributes,
        warnings: Vec::new(),
    }
}

/// The valid machine names of a section listing names.
fn section_machines<'a>(section: &'a Section, warnings: &mut Warnings) -> Vec<&'a str> {
    let mut machines = Vec::with_capacity(section.entries.len());
    for (line, text) in &section.entries {
        if is_short_name(text) {
            machines.push(text.as_str());
        } else if text.contains('=') || text.contains(char::is_whitespace) {
            warnings.add(WarningKind::InvalidLine, || format!("line {line}: {text}"));
        } else {
            warnings.add(WarningKind::InvalidMachineName, || {
                format!("line {line}: {text}")
            });
        }
    }
    machines
}

/// Versions that cannot be ordered are kept, but reported once each.
fn check_versions(attributes: &[Attribute], warnings: &mut Warnings) {
    let unrecognized: BTreeSet<&str> = attributes
        .iter()
        .map(|attribute| attribute.value.as_str())
        .filter(|value| !version::is_version(value))
        .collect();
    for value in unrecognized {
        warnings.add(WarningKind::UnrecognizedVersion, || value.to_owned());
    }
}
