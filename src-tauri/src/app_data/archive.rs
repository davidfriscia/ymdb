//! Reading and checking backup archives.
//!
//! An archive is not trusted because it contains a YMDB manifest. Before any
//! byte is extracted, [`open`] checks every entry: a single `ymdb-backup/`
//! top-level directory, logical paths accepted by `scope::classify` (no
//! absolute paths, no `..`, no drive letters), no symbolic links, no
//! encrypted entries, only stored or deflated content, no duplicate names
//! (also ignoring case), every manifest item present with its declared size
//! and no file missing from the manifest. [`copy_item`] then never reads
//! more bytes than declared, so a forged size cannot inflate the output
//! (decompression bombs), and compares the SHA-256 checksum.

use std::collections::{BTreeMap, HashSet};
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

use zip::{CompressionMethod, ZipArchive};

use super::manifest::{ARCHIVE_ROOT, MANIFEST_NAME, Manifest, ManifestItem};
use super::scope::{self, Classified};
use super::{BackupError, CopyError, Progress, copy_hashed};

/// Upper bounds that keep a malicious archive from exhausting resources.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub max_entries: usize,
    pub max_manifest_size: u64,
    /// Sum of the uncompressed sizes of all items.
    pub max_total_size: u64,
    /// Largest accepted uncompressed/compressed ratio of an entry (deflate
    /// cannot exceed about 1032:1).
    pub max_ratio: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_entries: 100_000,
            max_manifest_size: 32 * 1024 * 1024,
            max_total_size: 256 * 1024 * 1024 * 1024,
            max_ratio: 1100,
        }
    }
}

/// An archive whose structure and manifest have been checked.
pub struct CheckedArchive {
    archive: ZipArchive<File>,
    pub manifest: Manifest,
    /// Archive entry index of each manifest item, in manifest order.
    entries: Vec<(ManifestItem, Classified, usize)>,
}

impl CheckedArchive {
    pub fn items(&self) -> impl Iterator<Item = (&ManifestItem, &Classified)> {
        self.entries
            .iter()
            .map(|(item, classified, _)| (item, classified))
    }

    /// Copies item `index` (manifest order) to `writer`, checking its size
    /// and checksum.
    pub fn copy_item(
        &mut self,
        index: usize,
        writer: &mut impl io::Write,
        progress: &Progress,
    ) -> Result<(), BackupError> {
        let (item, _, entry) = &self.entries[index];
        let mut file = self
            .archive
            .by_index(*entry)
            .map_err(|error| BackupError::new(format!("cannot read '{}': {error}", item.path)))?;
        let (size, sha256) =
            copy_hashed(&mut file, writer, item.size, progress).map_err(|error| match error {
                CopyError::Cancelled => BackupError::cancelled(),
                CopyError::TooLarge => BackupError::new(format!(
                    "'{}' is larger than the size declared in the manifest",
                    item.path
                )),
                CopyError::Read(error) => {
                    BackupError::new(format!("'{}' is damaged: {error}", item.path))
                }
                CopyError::Write(error) => {
                    BackupError::new(format!("cannot write '{}': {error}", item.path))
                }
            })?;
        if size != item.size {
            return Err(BackupError::new(format!(
                "'{}' is incomplete ({size} of {} bytes)",
                item.path, item.size
            )));
        }
        if sha256 != item.sha256 {
            return Err(BackupError::new(format!(
                "the checksum of '{}' does not match the manifest: the file is damaged or was \
                 modified",
                item.path
            )));
        }
        Ok(())
    }

    /// Reads every item and checks its checksum, without extracting.
    pub fn verify_all(&mut self, progress: &Progress) -> Result<(), BackupError> {
        for index in 0..self.entries.len() {
            self.copy_item(index, &mut io::sink(), progress)?;
        }
        Ok(())
    }
}

fn entry_name(item_path: &str) -> String {
    format!("{ARCHIVE_ROOT}/{item_path}")
}

/// Opens an archive and checks its structure and manifest.
pub fn open(path: &Path, limits: &Limits) -> Result<CheckedArchive, BackupError> {
    let file = File::open(path).map_err(|error| BackupError::io("open", path, &error))?;
    let mut archive = ZipArchive::new(file).map_err(|error| {
        BackupError::new(format!("the file is not a valid ZIP archive: {error}"))
    })?;
    if archive.len() > limits.max_entries {
        return Err(BackupError::new("the archive contains too many entries"));
    }

    // Structure of every entry, before reading any content.
    let manifest_name = entry_name(MANIFEST_NAME);
    let mut files: BTreeMap<String, usize> = BTreeMap::new();
    let mut folded = HashSet::new();
    for index in 0..archive.len() {
        let entry = archive
            .by_index_raw(index)
            .map_err(|error| BackupError::new(format!("unreadable archive entry: {error}")))?;
        let name = std::str::from_utf8(entry.name_raw())
            .map_err(|_| BackupError::new("an archive entry has an invalid name"))?
            .to_owned();
        let invalid = |reason: &str| BackupError::new(format!("archive entry '{name}' {reason}"));
        if entry.is_symlink() {
            return Err(invalid("is a symbolic link"));
        }
        if entry.encrypted() {
            return Err(invalid("is encrypted"));
        }
        if entry.enclosed_name().is_none() || name.starts_with('/') || name.contains('\\') {
            return Err(invalid("has an unsafe path"));
        }
        let Some(relative) = name.strip_prefix(&format!("{ARCHIVE_ROOT}/")) else {
            return Err(invalid("is outside the backup directory"));
        };
        if !folded.insert(name.to_lowercase()) {
            return Err(invalid("appears more than once"));
        }
        if entry.is_dir() {
            let directory = relative.trim_end_matches('/');
            if !directory.is_empty() && !directory.split('/').all(scope::is_safe_component) {
                return Err(invalid("has an unsafe path"));
            }
            continue;
        }
        if let Some(mode) = entry.unix_mode() {
            // Only regular files (or no type, as written by some tools).
            let file_type = mode & 0o170_000;
            if file_type != 0 && file_type != 0o100_000 {
                return Err(invalid("is not a regular file"));
            }
        }
        if !matches!(
            entry.compression(),
            CompressionMethod::Stored | CompressionMethod::Deflated
        ) {
            return Err(invalid("uses an unsupported compression method"));
        }
        if entry.size()
            > entry
                .compressed_size()
                .max(1)
                .saturating_mul(limits.max_ratio)
        {
            return Err(invalid("has an implausible compression ratio"));
        }
        if name != manifest_name && scope::classify(relative).is_none() {
            return Err(invalid("is not a file YMDB backs up"));
        }
        files.insert(name, index);
    }

    let manifest = read_manifest(&mut archive, &files, &manifest_name, limits)?;
    let mut entries = Vec::with_capacity(manifest.items.len());
    let mut seen = HashSet::new();
    let mut total: u64 = 0;
    for item in &manifest.items {
        let invalid = |reason: &str| BackupError::new(format!("'{}' {reason}", item.path));
        let classified =
            scope::classify(&item.path).ok_or_else(|| invalid("is not a file YMDB backs up"))?;
        if classified.category != item.category || classified.kind != item.kind {
            return Err(invalid("has a wrong category in the manifest"));
        }
        if !seen.insert(item.path.to_lowercase()) {
            return Err(invalid("appears more than once in the manifest"));
        }
        if item.sha256.len() != 64
            || !item
                .sha256
                .bytes()
                .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
        {
            return Err(invalid("has an invalid checksum in the manifest"));
        }
        let index = *files
            .get(&entry_name(&item.path))
            .ok_or_else(|| invalid("is missing from the archive"))?;
        let size = archive
            .by_index_raw(index)
            .map_err(|error| BackupError::new(format!("unreadable archive entry: {error}")))?
            .size();
        if size != item.size {
            return Err(invalid("does not have the size declared in the manifest"));
        }
        total = total.saturating_add(item.size);
        entries.push((item.clone(), classified, index));
    }
    if total > limits.max_total_size {
        return Err(BackupError::new("the backup is larger than YMDB accepts"));
    }
    if let Some(extra) = files.keys().find(|name| {
        **name != manifest_name && !seen.contains(&name[ARCHIVE_ROOT.len() + 1..].to_lowercase())
    }) {
        return Err(BackupError::new(format!(
            "archive entry '{extra}' is not listed in the manifest"
        )));
    }
    Ok(CheckedArchive {
        archive,
        manifest,
        entries,
    })
}

fn read_manifest(
    archive: &mut ZipArchive<File>,
    files: &BTreeMap<String, usize>,
    manifest_name: &str,
    limits: &Limits,
) -> Result<Manifest, BackupError> {
    let index = *files
        .get(manifest_name)
        .ok_or_else(|| BackupError::new("the archive has no YMDB backup manifest"))?;
    let entry = archive
        .by_index(index)
        .map_err(|error| BackupError::new(format!("cannot read the manifest: {error}")))?;
    let mut content = Vec::new();
    entry
        .take(limits.max_manifest_size + 1)
        .read_to_end(&mut content)
        .map_err(|error| BackupError::new(format!("cannot read the manifest: {error}")))?;
    if content.len() as u64 > limits.max_manifest_size {
        return Err(BackupError::new("the manifest is too large"));
    }
    serde_json::from_slice(&content)
        .map_err(|error| BackupError::new(format!("the manifest is invalid: {error}")))
}
