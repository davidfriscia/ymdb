//! MAME support files: third-party classifications of MAME machines.
//!
//! - [`model`]: formats, dimensions and the normalized parsed data.
//! - [`parser`]: format-specific parsers (`catver.ini`, `genre.ini`,
//!   `catlist.ini`, `version.ini`, `series.ini`).
//! - [`version`]: semantic ordering of MAME version strings.
//! - [`repository`]: the global repository of imported sources, their active
//!   selection and revisions.
//!
//! Support files are imported manually and never downloaded. They are not
//! MAME data, so they never go into a catalog: each catalog keeps a
//! projection of the active classifications instead
//! (`catalog::classification`), matched by machine short name.

pub mod model;
pub mod parser;
pub mod repository;
pub mod version;

#[cfg(test)]
pub mod tests;
