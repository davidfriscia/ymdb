//! Finds the images of a machine or software in the configured directories.
//!
//! Naming conventions, checked against the common MAME artwork collections
//! and MAME's own behavior:
//!
//! - Machines: `<dir>/<machine>.<ext>`, as in the artwork collections
//!   (`snap/pacman.png`, `flyers/pacman.jpg`); then the numbered screenshots
//!   saved by MAME itself in `<dir>/<machine>/` (`0000.png`, `0001.png`, …,
//!   `snapname` default `%g/%i`).
//! - Software: `<dir>/<list>/<software>.<ext>`, as used by the software list
//!   packs of the collections and by MAME's internal interface. Software names
//!   are only unique within a list, so the list is always part of the path.
//!
//! A category may hold several images of a title. Directories are searched in
//! order of precedence and each contributes, in this order, its
//! `<name>.<ext>` image (the first extension in the order of [`FORMATS`]) and,
//! for machines, the numbered screenshots sorted by number. The first image is
//! the one shown where a single image is needed. Only the files of the
//! requested title are looked up (at most one directory listing per source):
//! the directories are never scanned as a whole. There is no parent/clone
//! fallback: a clone may be a different title, and showing its parent's image
//! would attribute it to the wrong game.
//!
//! Only file names built from validated short names are looked up, and the
//! result must stay inside the configured directory after resolving symbolic
//! links and junctions.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::catalog::is_short_name;

/// Supported image formats (all displayed by WebView2, WebKitGTK and WKWebView),
/// in order of preference.
pub const FORMATS: &[(&str, &str)] = &[
    ("png", "image/png"),
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("webp", "image/webp"),
];

/// Images larger than this are not served.
pub const MAX_IMAGE_BYTES: u64 = 32 * 1024 * 1024;

/// At most this many images are listed per category and title.
pub const MAX_IMAGES: usize = 100;

/// What an image is requested for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKey<'a> {
    Machine(&'a str),
    Software { list: &'a str, name: &'a str },
}

impl MediaKey<'_> {
    fn is_valid(&self) -> bool {
        match self {
            Self::Machine(name) => is_short_name(name),
            Self::Software { list, name } => is_short_name(list) && is_short_name(name),
        }
    }

    /// The images of this key inside one canonical source directory, in order.
    fn files_in(&self, root: &Path) -> Vec<ResolvedMedia> {
        let (directory, name) = match self {
            Self::Machine(name) => (root.to_path_buf(), *name),
            Self::Software { list, name } => (root.join(list), *name),
        };
        let named = FORMATS.iter().find_map(|(extension, _)| {
            contained_file(root, &directory.join(format!("{name}.{extension}")))
        });
        let mut files: Vec<ResolvedMedia> = named.into_iter().collect();
        // MAME saves numbered screenshots of machines only.
        if let Self::Machine(name) = self {
            files.extend(numbered_files(root, &directory.join(name)));
        }
        files
    }

    /// The first image of this key inside one canonical source directory,
    /// without listing the screenshot directory when a named image exists.
    fn first_in(&self, root: &Path) -> Option<ResolvedMedia> {
        let (directory, name) = match self {
            Self::Machine(name) => (root.to_path_buf(), *name),
            Self::Software { list, name } => (root.join(list), *name),
        };
        FORMATS
            .iter()
            .find_map(|(extension, _)| {
                contained_file(root, &directory.join(format!("{name}.{extension}")))
            })
            .or_else(|| match self {
                Self::Machine(name) => numbered_files(root, &directory.join(name))
                    .into_iter()
                    .next(),
                Self::Software { .. } => None,
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedMedia {
    pub path: PathBuf,
    pub content_type: &'static str,
}

/// Every image for `key` in `sources`, in order, without duplicates (a file
/// reachable from two sources is listed once), at most [`MAX_IMAGES`].
pub fn resolve_all(sources: &[PathBuf], key: MediaKey<'_>) -> Vec<ResolvedMedia> {
    if !key.is_valid() {
        return Vec::new();
    }
    let mut seen = HashSet::new();
    sources
        .iter()
        // A source that cannot be resolved (missing, offline) is skipped.
        .filter_map(|root| fs::canonicalize(root).ok())
        .flat_map(|root| key.files_in(&root))
        .filter(|media| seen.insert(media.path.clone()))
        .take(MAX_IMAGES)
        .collect()
}

/// The first existing image for `key` in `sources`, if any: the first one of
/// [`resolve_all`].
pub fn resolve(sources: &[PathBuf], key: MediaKey<'_>) -> Option<ResolvedMedia> {
    if !key.is_valid() {
        return None;
    }
    sources
        .iter()
        .filter_map(|root| fs::canonicalize(root).ok())
        .find_map(|root| key.first_in(&root))
}

/// The image at `index` in the order of [`resolve_all`], if any.
pub fn resolve_nth(sources: &[PathBuf], key: MediaKey<'_>, index: usize) -> Option<ResolvedMedia> {
    if index == 0 {
        return resolve(sources, key);
    }
    resolve_all(sources, key).into_iter().nth(index)
}

/// Numbered images (`0000.png`, `0001.png`, …) of `directory`, sorted by number.
fn numbered_files(root: &Path, directory: &Path) -> Vec<ResolvedMedia> {
    let Ok(entries) = fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
        .filter(|name| is_numbered_image(name))
        .collect();
    // MAME pads numbers with zeros; a longer number is a larger one.
    names.sort_by(|a, b| a.len().cmp(&b.len()).then_with(|| a.cmp(b)));
    names
        .iter()
        .filter_map(|name| contained_file(root, &directory.join(name)))
        .take(MAX_IMAGES)
        .collect()
}

fn is_numbered_image(name: &str) -> bool {
    let Some((stem, extension)) = name.rsplit_once('.') else {
        return false;
    };
    (1..=8).contains(&stem.len())
        && stem.bytes().all(|byte| byte.is_ascii_digit())
        && FORMATS
            .iter()
            .any(|(known, _)| extension.eq_ignore_ascii_case(known))
}

/// The canonical path of `path` if it is a regular file inside `root`.
fn contained_file(root: &Path, path: &Path) -> Option<ResolvedMedia> {
    let path = fs::canonicalize(path).ok()?;
    if !path.starts_with(root) || !path.is_file() {
        return None;
    }
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    let content_type = FORMATS
        .iter()
        .find(|(known, _)| *known == extension)
        .map(|(_, content_type)| *content_type)?;
    Some(ResolvedMedia { path, content_type })
}
