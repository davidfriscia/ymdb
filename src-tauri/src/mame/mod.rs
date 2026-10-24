//! Integration with a local MAME installation.
//!
//! The submodules are independent building blocks, designed to be reused by
//! later features such as the catalog importer and the launcher:
//!
//! - [`process`]: runs an external program directly (never through a shell) with a timeout.
//! - [`version`]: recognizes the output of `mame -version`.
//! - [`validation`]: checks that a path points to a usable MAME executable.
//! - [`detection`]: finds candidate MAME executables in well-known locations.
//! - [`ini`]: reads MAME's own search paths and input settings from its INI
//!   files (never writes them).
//! - [`input_log`]: recognizes MAME's verbose lines about input devices.
//! - `platform`: operating system specific details, isolated in one place.

pub mod detection;
pub mod ini;
pub mod input_log;
pub(crate) mod platform;
pub mod process;
pub mod validation;
pub mod version;
