//! The `ymdb-media` URI scheme.
//!
//! The webview requests images by identifiers only:
//!
//! - `<category>/machine/<machine>[/<index>]`
//! - `<category>/software/<list>/<software>[/<index>]`
//!
//! where `index` (default 0) selects one of the images of the title in the
//! order of [`super::resolve::resolve_all`].
//!
//! (on Windows `http://ymdb-media.localhost/<path>`, elsewhere
//! `ymdb-media://localhost/<path>`; the path may be percent-encoded as one
//! segment). Every part is validated, and the file is found by
//! [`super::resolve`] inside the configured directories of the category, so no
//! request can name an arbitrary file.

use std::fs;

use serde::{Deserialize, Serialize};

use super::config::{CATEGORIES, MediaConfig, is_category};
use super::resolve::{MAX_IMAGE_BYTES, MAX_IMAGES, MediaKey, resolve_all, resolve_nth};

pub const SCHEME: &str = "ymdb-media";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaResponse {
    pub status: u16,
    pub content_type: &'static str,
    pub body: Vec<u8>,
}

impl MediaResponse {
    fn error(status: u16) -> Self {
        Self {
            status,
            content_type: "text/plain",
            body: Vec::new(),
        }
    }
}

/// Answers a request for `uri_path` (the URI path, without query).
pub fn respond(config: &MediaConfig, uri_path: &str) -> MediaResponse {
    let Some(path) = percent_decode(uri_path) else {
        return MediaResponse::error(400);
    };
    let segments: Vec<&str> = path.trim_start_matches('/').split('/').collect();
    let (category, key, index) = match segments.as_slice() {
        [category, "machine", name] => (*category, MediaKey::Machine(name), Some(0)),
        [category, "machine", name, index] => {
            (*category, MediaKey::Machine(name), parse_index(index))
        }
        [category, "software", list, name] => {
            (*category, MediaKey::Software { list, name }, Some(0))
        }
        [category, "software", list, name, index] => (
            *category,
            MediaKey::Software { list, name },
            parse_index(index),
        ),
        _ => return MediaResponse::error(400),
    };
    let Some(index) = index else {
        return MediaResponse::error(400);
    };
    if !is_category(category) {
        return MediaResponse::error(400);
    }
    let Some(media) = resolve_nth(config.sources(category), key, index) else {
        return MediaResponse::error(404);
    };
    match fs::metadata(&media.path) {
        Ok(metadata) if metadata.len() > MAX_IMAGE_BYTES => return MediaResponse::error(413),
        Ok(_) => {}
        Err(_) => return MediaResponse::error(404),
    }
    match fs::read(&media.path) {
        Ok(body) => MediaResponse {
            status: 200,
            content_type: media.content_type,
            body,
        },
        Err(_) => MediaResponse::error(500),
    }
}

/// A decimal image index below [`MAX_IMAGES`], without sign or padding.
fn parse_index(text: &str) -> Option<usize> {
    if text.is_empty() || text.len() > 3 || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    if text.len() > 1 && text.starts_with('0') {
        return None;
    }
    text.parse().ok().filter(|index| *index < MAX_IMAGES)
}

/// Decodes `%XX` escapes; `None` for malformed escapes or invalid UTF-8.
fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = text.get(index + 1..index + 3)?;
            decoded.push(u8::from_str_radix(hex, 16).ok()?);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).ok()
}

/// What the images of a title are listed for. Mirrors `MediaTarget` in
/// `src/features/media/mediaApi.ts`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum MediaTarget {
    Machine { name: String },
    Software { list: String, name: String },
}

impl MediaTarget {
    fn key(&self) -> MediaKey<'_> {
        match self {
            Self::Machine { name } => MediaKey::Machine(name),
            Self::Software { list, name } => MediaKey::Software { list, name },
        }
    }
}

/// The number of images of a title in one category.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryImages {
    pub category: &'static str,
    pub count: usize,
}

/// The categories with at least one image of `target`, in category order,
/// with their number of images (requested by index through the protocol).
pub fn list_images(config: &MediaConfig, target: &MediaTarget) -> Vec<CategoryImages> {
    CATEGORIES
        .iter()
        .map(|category| CategoryImages {
            category: category.id,
            count: resolve_all(config.sources(category.id), target.key()).len(),
        })
        .filter(|images| images.count > 0)
        .collect()
}
