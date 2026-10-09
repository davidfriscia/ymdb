//! MAME machine catalog: parsing of `mame -listxml` and its SQLite storage.
//!
//! - [`parser`]: streaming parser for the `-listxml` document.
//! - [`model`]: in-memory model of one machine entry.
//! - [`schema`]: versioned SQLite schema and migrations.
//! - [`store`]: catalog database files, writing and atomic replacement.
//! - [`import`]: orchestration of an import from the MAME executable.
//! - [`freshness`]: whether a catalog matches the configured executable.

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
