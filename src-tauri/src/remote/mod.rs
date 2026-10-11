//! Remote repositories of ROM, CHD and sample files.
//!
//! Three layers, independent of any protocol:
//!
//! - [`connections`]: global remote connections (`remote-connections.json`):
//!   how to reach a repository, with the provider type and its settings.
//! - [`sources`]: per installation, which connection, remote path and local
//!   destination each category of files uses (`remote-sources.json`).
//! - [`provider`]: the contract the ROM manager uses to talk to a repository
//!   ([`provider::RemoteRepository`]), with [`sftp`] as the first provider.
//!
//! [`path`] defines remote paths (POSIX, never local paths) and [`transfer`]
//! streams downloads with progress and cancellation. Nothing here downloads
//! ROMs yet; that belongs to the ROM manager.

pub mod connections;
pub mod path;
pub mod provider;
pub mod sftp;
pub mod sources;
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "used by the ROM manager's downloads")
)]
pub mod transfer;

#[cfg(test)]
pub mod test_server;
