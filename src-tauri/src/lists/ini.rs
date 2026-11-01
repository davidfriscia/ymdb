//! Folder files of MAME frontends (`.ini`), imported as user lists.
//!
//! These files (`bootlegs.ini`, `category.ini`, `mame.ini` of the pS
//! support files, folders of MAMEUI and other frontends) list MAME machines
//! by short name, one per line, in sections:
//!
//! ```text
//! [FOLDER_SETTINGS]          settings of the frontend: ignored
//! RootFolderIcon mame
//!
//! ;; BOOTLEGS.ini 0.288 / 09-Jun-26 / MAME 0.288 ;;   comments: ignored
//!
//! [ROOT_FOLDER]              machines of the folder itself
//! 1942abl
//!
//! [Arcade / Pinball]         a subfolder, named as written
//! aliencr
//! ```
//!
//! Only `[ROOT_FOLDER]` and the subfolder sections are read: every other
//! section name is a subfolder, whatever its name (`S1`, `Arcade / Pinball`).
//! Lines that are not MAME short names (`key=value` settings, text) are never
//! taken as machines: they are skipped and reported. Machines missing from a
//! catalog are kept, since the file may come from another MAME version.
//!
//! The parser only reads the file: naming the lists and merging them is the
//! job of [`super::folder_import`].

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

use serde::Serialize;

use crate::catalog::is_short_name;
use crate::support_files::parser::windows_1252;

/// Largest accepted file (`category.ini` of MAME 0.288 is 0.5 MB).
pub const MAX_FILE_SIZE: u64 = 64 * 1024 * 1024;
/// Examples kept per warning kind.
const MAX_EXAMPLES: usize = 5;

const ROOT_FOLDER: &str = "ROOT_FOLDER";
const FOLDER_SETTINGS: &str = "FOLDER_SETTINGS";

#[derive(Debug)]
pub enum IniError {
    Io(io::Error),
    TooLarge {
        size: u64,
    },
    /// The file is empty or only has comments and settings.
    Empty,
    /// The file is not text in a supported encoding (UTF-16, binary data).
    UnsupportedEncoding(&'static str),
    /// The file is not a folder file: it has no folder section.
    NotAFolderFile,
}

impl fmt::Display for IniError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "the file could not be read: {error}"),
            Self::TooLarge { size } => write!(
                f,
                "the file is too large ({size} bytes, at most {MAX_FILE_SIZE} bytes)"
            ),
            Self::Empty => f.write_str("the file contains no machines"),
            Self::UnsupportedEncoding(encoding) => write!(
                f,
                "the file is encoded as {encoding}; save it as UTF-8 or Windows-1252"
            ),
            Self::NotAFolderFile => f.write_str(
                "the file is not a MAME folder file: it has no [ROOT_FOLDER] or folder section",
            ),
        }
    }
}

impl std::error::Error for IniError {}

/// Problems found in a file that do not prevent the import.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum IniWarningKind {
    /// The file is not valid UTF-8 and was read as Windows-1252.
    NotUtf8,
    /// The file has no `[ROOT_FOLDER]` section.
    MissingRootFolder,
    /// Text before the first section (outside any folder).
    OutsideSection,
    /// A line that is not a MAME short name (settings, text, invalid names).
    InvalidLine,
    /// A machine listed twice in the same folder (imported once).
    DuplicateEntry,
    /// A folder that appears twice in the file (its machines are combined).
    DuplicateFolder,
    /// A section without a name, `[]` (its lines are skipped).
    UnnamedSection,
}

/// One kind of warning: how often it happened, with the first examples.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IniWarning {
    pub kind: IniWarningKind,
    pub count: usize,
    pub examples: Vec<String>,
}

#[derive(Debug, Default)]
struct Warnings(Vec<IniWarning>);

impl Warnings {
    fn add(&mut self, kind: IniWarningKind, example: impl FnOnce() -> String) {
        match self.0.iter_mut().find(|warning| warning.kind == kind) {
            Some(warning) => {
                warning.count += 1;
                if warning.examples.len() < MAX_EXAMPLES {
                    warning.examples.push(example());
                }
            }
            None => self.0.push(IniWarning {
                kind,
                count: 1,
                examples: vec![example()],
            }),
        }
    }
}

/// A folder of the file with its machines, in file order and without duplicates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Folder {
    /// The section name as written (trimmed).
    pub name: String,
    pub machines: Vec<String>,
}

/// The folders read from a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderFile {
    /// Machines of `[ROOT_FOLDER]` (empty when it is empty or missing).
    pub root: Vec<String>,
    /// Subfolders with at least one machine, in file order.
    pub folders: Vec<Folder>,
    /// Subfolders without any machine (not imported).
    pub empty_folders: usize,
    /// Machines listed more than once within a folder.
    pub duplicates: usize,
    pub warnings: Vec<IniWarning>,
}

impl FolderFile {
    pub fn has_subfolders(&self) -> bool {
        !self.folders.is_empty()
    }
}

/// Reads and parses a folder file.
pub fn read_file(path: &Path) -> Result<FolderFile, IniError> {
    let file = File::open(path).map_err(IniError::Io)?;
    let size = file.metadata().map_err(IniError::Io)?.len();
    if size > MAX_FILE_SIZE {
        return Err(IniError::TooLarge { size });
    }
    let mut content = Vec::new();
    file.take(MAX_FILE_SIZE + 1)
        .read_to_end(&mut content)
        .map_err(IniError::Io)?;
    parse(&content)
}

/// A section being read: its machines and the names already seen.
struct Section {
    name: String,
    machines: Vec<String>,
    seen: HashSet<String>,
}

/// Parses the content of a folder file.
pub fn parse(content: &[u8]) -> Result<FolderFile, IniError> {
    let mut warnings = Warnings::default();
    let text = decode(content, &mut warnings)?;
    // `\n` and `\r\n` line endings, or `\r` alone in old Mac files.
    let lines: Vec<&str> = if text.contains('\n') {
        text.lines().collect()
    } else {
        text.split('\r').collect()
    };

    let mut root: Option<Section> = None;
    let mut folders: Vec<Section> = Vec::new();
    // Index of each folder in `folders`, by exact name.
    let mut folder_index: HashMap<String, usize> = HashMap::new();
    // Where the lines go.
    #[derive(Clone, Copy)]
    enum Target {
        /// Before the first section: reported.
        Outside,
        /// `[FOLDER_SETTINGS]` and unnamed sections: ignored.
        Skipped,
        Root,
        Folder(usize),
    }
    let mut target = Target::Outside;
    let mut duplicates = 0;
    let mut any_section = false;

    for (index, line) in lines.into_iter().enumerate() {
        let number = index + 1;
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if let Some(name) = line
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
        {
            let name = name.trim();
            if name.eq_ignore_ascii_case(FOLDER_SETTINGS) {
                any_section = true;
                target = Target::Skipped;
            } else if name.eq_ignore_ascii_case(ROOT_FOLDER) {
                any_section = true;
                if root.is_some() {
                    warnings.add(IniWarningKind::DuplicateFolder, || {
                        format!("line {number}: [{name}]")
                    });
                }
                root.get_or_insert_with(|| Section::new(name));
                target = Target::Root;
            } else if name.is_empty() {
                warnings.add(IniWarningKind::UnnamedSection, || format!("line {number}"));
                target = Target::Skipped;
            } else {
                any_section = true;
                target = match folder_index.get(name) {
                    Some(&existing) => {
                        warnings.add(IniWarningKind::DuplicateFolder, || {
                            format!("line {number}: [{name}]")
                        });
                        Target::Folder(existing)
                    }
                    None => {
                        folder_index.insert(name.to_owned(), folders.len());
                        folders.push(Section::new(name));
                        Target::Folder(folders.len() - 1)
                    }
                };
            }
            continue;
        }
        let section = match target {
            Target::Skipped => continue,
            Target::Outside => {
                warnings.add(IniWarningKind::OutsideSection, || {
                    format!("line {number}: {line}")
                });
                continue;
            }
            Target::Root => root.as_mut(),
            Target::Folder(index) => folders.get_mut(index),
        };
        let Some(section) = section else { continue };
        if !is_short_name(line) {
            warnings.add(IniWarningKind::InvalidLine, || {
                format!("line {number}: {line}")
            });
            continue;
        }
        if !section.add(line) {
            duplicates += 1;
            warnings.add(IniWarningKind::DuplicateEntry, || {
                format!("[{}] {line}", section.name)
            });
        }
    }

    if !any_section {
        return Err(if text.trim().is_empty() {
            IniError::Empty
        } else {
            IniError::NotAFolderFile
        });
    }
    if root.is_none() {
        warnings.add(IniWarningKind::MissingRootFolder, || {
            "[ROOT_FOLDER]".to_owned()
        });
    }
    let root = root.map(|section| section.machines).unwrap_or_default();
    let empty_folders = folders
        .iter()
        .filter(|folder| folder.machines.is_empty())
        .count();
    let folders: Vec<Folder> = folders
        .into_iter()
        .filter(|folder| !folder.machines.is_empty())
        .map(|folder| Folder {
            name: folder.name,
            machines: folder.machines,
        })
        .collect();
    if root.is_empty() && folders.is_empty() {
        return Err(IniError::Empty);
    }
    Ok(FolderFile {
        root,
        folders,
        empty_folders,
        duplicates,
        warnings: warnings.0,
    })
}

impl Section {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            machines: Vec::new(),
            seen: HashSet::new(),
        }
    }

    /// Adds a machine; returns `false` if the section already has it.
    fn add(&mut self, machine: &str) -> bool {
        if !self.seen.insert(machine.to_owned()) {
            return false;
        }
        self.machines.push(machine.to_owned());
        true
    }
}

/// UTF-8 (with or without BOM), else Windows-1252, the encoding of older
/// files written on Windows. UTF-16 and binary content are refused.
fn decode(content: &[u8], warnings: &mut Warnings) -> Result<String, IniError> {
    if content.starts_with(b"\xFF\xFE") || content.starts_with(b"\xFE\xFF") {
        return Err(IniError::UnsupportedEncoding("UTF-16"));
    }
    let content = content.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(content);
    if content.contains(&0) {
        return Err(IniError::UnsupportedEncoding("binary data or UTF-16"));
    }
    match std::str::from_utf8(content) {
        Ok(text) => Ok(text.to_owned()),
        Err(error) => {
            warnings.add(IniWarningKind::NotUtf8, || {
                format!("invalid UTF-8 at byte {}", error.valid_up_to())
            });
            Ok(content.iter().map(|&byte| windows_1252(byte)).collect())
        }
    }
}
