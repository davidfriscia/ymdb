//! User lists of machines and software, global to YMDB.
//!
//! A list holds references to machines and software by their MAME names
//! ([`ListEntry`]), never by catalog ids, installation or description, so the
//! same entry matches the same title in the catalog of every MAME
//! installation where it exists. Entries that are missing from a catalog are
//! kept: they show up again with an installation that has them.
//!
//! Favorites is a system list (id [`model::FAVORITES`]): it always exists and
//! cannot be renamed or deleted. Lists created by the user share the same
//! model (kind `user`, with a name); this version manages Favorites only.
//!
//! - [`model`]: lists, entries, validation and membership changes.
//! - [`store`]: `lists.json` persistence in the configuration directory.
//! - [`exchange`]: the portable export format and its import.
//!
//! Lists never live in a catalog: searches restricted to a list receive its
//! entries from the caller ([`crate::catalog::search::ListMembers`]).

pub mod exchange;
pub mod model;
pub mod store;

#[cfg(test)]
mod tests;
