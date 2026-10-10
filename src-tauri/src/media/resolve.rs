//! Finds the image of a machine or software in the configured directories.
//!
//! Naming conventions, checked against the common MAME artwork collections
//! and MAME's own behavior:
//!
//! - Machines: `<dir>/<machine>.<ext>`, as in the artwork collections
//!   (`snap/pacman.png`, `flyers/pacman.jpg`); then `<dir>/<machine>/0000.png`,
//!   the first screenshot saved by MAME itself (`snapname` default `%g/%i`).
//! - Software: `<dir>/<list>/<software>.<ext>`, as used by the software list
//!   packs of the collections and by MAME's internal interface. Software names
//!   are only unique within a list, so the list is always part of the path.
//!
//! Directories are searched in order of precedence and, within a directory,
//! extensions in the order of [`FORMATS`]; the first match wins. There is no
//! parent/clone fallback: a clone may be a different title, and showing its
//! parent's image would attribute it to the wrong game.
//!
//! Only file names built from validated short names are looked up, and the
//! result must stay inside the configured directory after resolving symbolic
//! links and junctions.

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

    /// Relative candidate paths, in order.
    fn candidates(&self) -> Vec<PathBuf> {
        match self {
            Self::Machine(name) => FORMATS
                .iter()
                .map(|(extension, _)| PathBuf::from(format!("{name}.{extension}")))
                .chain(std::iter::once(Path::new(name).join("0000.png")))
                .collect(),
            Self::Software { list, name } => FORMATS
                .iter()
                .map(|(extension, _)| Path::new(list).join(format!("{name}.{extension}")))
                .collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedMedia {
    pub path: PathBuf,
    pub content_type: &'static str,
}

/// The first existing image for `key` in `sources`, if any.
pub fn resolve(sources: &[PathBuf], key: MediaKey<'_>) -> Option<ResolvedMedia> {
    if !key.is_valid() {
        return None;
    }
    let candidates = key.candidates();
    sources.iter().find_map(|root| {
        // A source that cannot be resolved (missing, offline) is skipped.
        let root = fs::canonicalize(root).ok()?;
        candidates
            .iter()
            .find_map(|candidate| contained_file(&root, candidate))
    })
}

/// The canonical path of `root/relative` if it is a regular file inside `root`.
fn contained_file(root: &Path, relative: &Path) -> Option<ResolvedMedia> {
    let path = fs::canonicalize(root.join(relative)).ok()?;
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
