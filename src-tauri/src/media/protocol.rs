//! The `ymdb-media` URI scheme.
//!
//! The webview requests images by identifiers only:
//!
//! - `<category>/machine/<machine>`
//! - `<category>/software/<list>/<software>`
//!
//! (on Windows `http://ymdb-media.localhost/<path>`, elsewhere
//! `ymdb-media://localhost/<path>`; the path may be percent-encoded as one
//! segment). Every part is validated, and the file is found by
//! [`super::resolve`] inside the configured directories of the category, so no
//! request can name an arbitrary file.

use std::fs;

use super::config::{MediaConfig, is_category};
use super::resolve::{MAX_IMAGE_BYTES, MediaKey, resolve};

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
    let (category, key) = match segments.as_slice() {
        [category, "machine", name] => (*category, MediaKey::Machine(name)),
        [category, "software", list, name] => (*category, MediaKey::Software { list, name }),
        _ => return MediaResponse::error(400),
    };
    if !is_category(category) {
        return MediaResponse::error(400);
    }
    let Some(media) = resolve(config.sources(category), key) else {
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
