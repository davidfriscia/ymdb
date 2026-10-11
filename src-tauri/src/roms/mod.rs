//! ROM manager: set-based download plans and downloads.
//!
//! The ROM manager works with whole files, never with the ROMs inside them:
//! for a launch it lists the sets that may be needed (the machine, its
//! parents and BIOS, its devices, its disks, its samples and, for a
//! software, the software and its parents), checks which files MAME can
//! already find locally, looks for the missing ones in the remote sources
//! and downloads those that exist. Redundancy is accepted on purpose: a set
//! that may not be needed is downloaded rather than analysing archives.
//!
//! - [`planner`]: the candidate files of a launch, from the catalog.
//! - [`local`]: where MAME would find each file, and where a download goes.
//! - [`availability`]: whether missing files exist in the remote repositories.
//! - [`plan`]: the download plan shown to the user.
//! - [`download`]: sequential downloads into temporary files, finalized
//!   without ever replacing an existing file.
//!
//! A present or downloaded file is not verified against MAME's data: only
//! MAME can tell whether a set is complete.

pub mod availability;
pub mod download;
pub mod local;
pub mod plan;
pub mod planner;

use std::collections::HashMap;
use std::future::Future;

use serde::Serialize;

use crate::remote::connections::RemoteConnection;
use crate::remote::provider::{self, ConnectOptions, RemoteError, RemoteRepository, Repository};

/// Opens the repository of a connection. The ROM manager only uses this and
/// [`RemoteRepository`], never a protocol.
pub trait Connector: Sync {
    type Repository: RemoteRepository;

    fn connect(
        &self,
        connection: &RemoteConnection,
    ) -> impl Future<Output = Result<Self::Repository, RemoteError>> + Send;
}

/// Connects with the provider each connection names, with per-connection
/// options (passphrases are given for one operation and dropped with it).
pub struct ProviderConnector {
    pub defaults: ConnectOptions,
    pub per_connection: HashMap<String, ConnectOptions>,
}

impl Connector for ProviderConnector {
    type Repository = Repository;

    async fn connect(&self, connection: &RemoteConnection) -> Result<Repository, RemoteError> {
        let options = self
            .per_connection
            .get(&connection.id)
            .unwrap_or(&self.defaults);
        provider::connect(connection, options)
            .await
            .map(|(repository, _)| repository)
    }
}

/// Why a file is part of a plan: the relation of its set to the launch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Reason {
    /// The machine launched on its own.
    Selected,
    /// The machine running the selected software.
    System,
    /// A parent of a machine (clone of, ROM of).
    Parent,
    /// A BIOS set of a machine.
    Bios,
    /// A device with ROMs of the machine.
    Device,
    /// The selected software.
    Software,
    /// A parent of the selected software.
    SoftwareParent,
    /// The sample set of the machine.
    Sample,
}

#[cfg(test)]
pub(crate) mod tests;

#[cfg(test)]
mod e2e;
