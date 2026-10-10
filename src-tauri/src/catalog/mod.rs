//! MAME machine catalog: parsing of `mame -listxml` and its SQLite storage.
//!
//! - [`parser`]: streaming parser for the `-listxml` document.
//! - [`model`]: in-memory model of one machine entry.
//! - [`schema`]: versioned SQLite schema and migrations.
//! - [`store`]: catalog database files, writing and atomic replacement.
//! - [`import`]: orchestration of an import from the MAME executable.
//! - [`freshness`]: whether a catalog matches the configured executable.

pub mod detail;
pub mod freshness;
pub mod import;
pub mod model;
pub mod parser;
pub mod schema;
pub mod search;
pub mod software_model;
pub mod software_parser;
pub mod store;
mod xml;

/// Whether `name` is a MAME short name of a machine, software list or
/// software: 1-64 lowercase ASCII letters, digits or `_` (all names of MAME
/// 0.280 satisfy this). Names checked this way are safe as path segments.
pub fn is_short_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}
