//! User lists of machines and software, global to YMDB.
//!
//! A list holds references to machines and software by their MAME names
//! ([`ListEntry`]), never by catalog ids, installation or description, so the
//! same entry matches the same title in the catalog of every MAME
//! installation where it exists. Entries that are missing from a catalog are
//! kept: they show up again with an installation that has them.
//!
//! Favorites is a system list (id [`model::FAVORITES`]): it always exists and
//! cannot be renamed or deleted. Custom lists created by the user share the
//! same model (kind `user`, with a name unique ignoring case); they are
//! functionally independent of Favorites.
//!
//! - [`model`]: lists, entries, validation and membership changes.
//! - [`store`]: `lists.json` persistence in the configuration directory.
//! - [`exchange`]: the portable export format and its import.
//! - [`ini`]: MAME folder files (`.ini` with `[ROOT_FOLDER]` and subfolders).
//! - [`folder_import`]: naming and merging the lists of a folder file.
//!
//! Lists never live in a catalog: searches restricted to a list receive its
//! entries from the caller ([`crate::catalog::search::ListMembers`]).

pub mod exchange;
pub mod folder_import;
pub mod ini;
pub mod model;
pub mod store;

#[cfg(test)]
mod import_tests;
#[cfg(test)]
mod tests;
