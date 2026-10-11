//! The local side of a plan: where MAME would find a file, and where a
//! download goes.
//!
//! Presence is checked in the directories MAME will search for the launch
//! (its effective `rompath` or `samplepath`), not only in the download
//! destinations. The check is deliberately shallow: the file exists, is a
//! regular non-empty file and starts like the expected format. Archives are
//! never opened further and disks are never hashed.

use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::mame::ini::resolve_entry;
use crate::remote::sources::{LocalDestination, SearchPath, SourceCategory};

/// Longest accepted file or directory name taken from the catalog.
const MAX_NAME_LENGTH: usize = 128;

/// The directories MAME searches for a launch, resolved against its directory.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SearchDirs {
    pub rom: Vec<PathBuf>,
    pub sample: Vec<PathBuf>,
}

impl SearchDirs {
    /// `rompath` and `samplepath` entries as MAME receives them (relative
    /// entries are relative to `mame_dir`, where MAME runs).
    pub fn new(mame_dir: &Path, rompath: &[String], samplepath: &[String]) -> Self {
        let resolve = |entries: &[String]| {
            entries
                .iter()
                .map(|entry| resolve_entry(mame_dir, entry))
                .collect()
        };
        Self {
            rom: resolve(rompath),
            sample: resolve(samplepath),
        }
    }

    pub fn for_category(&self, category: SourceCategory) -> &[PathBuf] {
        match category.search_path() {
            SearchPath::Rom => &self.rom,
            SearchPath::Sample => &self.sample,
        }
    }
}

/// A file found locally.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalFile {
    pub path: PathBuf,
    pub size: u64,
    /// Set when the file does not look usable (empty, unexpected format).
    pub issue: Option<String>,
}

/// The first file MAME would find for `segments` in `dirs`. Archives may also
/// be 7-Zip files and samples FLAC files, as MAME accepts them.
pub fn find(dirs: &[PathBuf], segments: &[String]) -> Option<LocalFile> {
    let (file_name, parents) = segments.split_last()?;
    let names = alternative_names(file_name);
    for dir in dirs {
        let mut base = dir.clone();
        base.extend(parents);
        for name in &names {
            let path = base.join(name);
            let Ok(metadata) = fs::metadata(&path) else {
                continue;
            };
            if !metadata.is_file() {
                continue;
            }
            let size = metadata.len();
            return Some(LocalFile {
                issue: content_issue(&path, name, size),
                path,
                size,
            });
        }
    }
    None
}

fn alternative_names(file_name: &str) -> Vec<String> {
    let alternative =
        [(".zip", ".7z"), (".wav", ".flac")]
            .into_iter()
            .find_map(|(extension, other)| {
                file_name
                    .strip_suffix(extension)
                    .map(|stem| format!("{stem}{other}"))
            });
    std::iter::once(file_name.to_owned())
        .chain(alternative)
        .collect()
}

/// Signatures of the formats the ROM manager looks for.
const SIGNATURES: [(&str, &[&[u8]]); 5] = [
    (".zip", &[b"PK\x03\x04", b"PK\x05\x06"]),
    (".7z", &[b"7z\xBC\xAF\x27\x1C"]),
    (".chd", &[b"MComprHD"]),
    (".wav", &[b"RIFF"]),
    (".flac", &[b"fLaC"]),
];

fn content_issue(path: &Path, name: &str, size: u64) -> Option<String> {
    if size == 0 {
        return Some("The local file is empty.".to_owned());
    }
    let (extension, signatures) = SIGNATURES
        .iter()
        .find(|(extension, _)| name.ends_with(extension))?;
    let mut header = [0; 8];
    let read = File::open(path)
        .and_then(|mut file| {
            let mut read = 0;
            while read < header.len() {
                match file.read(&mut header[read..])? {
                    0 => break,
                    count => read += count,
                }
            }
            Ok(read)
        })
        .ok();
    match read {
        None => Some("The local file cannot be read.".to_owned()),
        Some(read) if signatures.iter().any(|s| header[..read].starts_with(s)) => None,
        Some(_) => Some(format!(
            "The local file does not look like a {} file; MAME may not be able to use it.",
            extension.trim_start_matches('.').to_uppercase()
        )),
    }
}

/// The directory a destination names (relative MAME directories are
/// relative to the MAME directory).
pub fn destination_dir(mame_dir: &Path, destination: &LocalDestination) -> PathBuf {
    match destination {
        LocalDestination::MameDirectory { directory } => resolve_entry(mame_dir, directory),
        LocalDestination::Custom { directory } => directory.clone(),
    }
}

/// Whether a name from the catalog can be used as a file or directory name
/// on every supported platform.
pub fn is_portable_name(name: &str) -> bool {
    const RESERVED: [&str; 22] = [
        "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
        "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
    ];
    let stem = name
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    !name.is_empty()
        && name.len() <= MAX_NAME_LENGTH
        && !name.starts_with('.')
        && !name.ends_with(['.', ' '])
        && !name.starts_with(' ')
        && !name.chars().any(|c| {
            c.is_control() || matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*')
        })
        && !RESERVED.contains(&stem.as_str())
}
