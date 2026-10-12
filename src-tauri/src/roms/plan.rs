//! The download plan of a launch, as shown to the user.
//!
//! A plan is built in three steps: [`build_artifacts`] maps the planner's
//! candidates to local and remote paths and checks local presence,
//! [`check_remote`] looks up the missing files in their repositories, and
//! [`finish`] detects destination conflicts, decides what is downloaded and
//! summarizes. Missing candidates never make a plan fail: they are warnings.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::availability::{RemoteCheck, check_paths};
use super::local::{LocalFile, SearchDirs, destination_dir, find, is_portable_name};
use super::planner::{AbsentNote, Candidate};
use super::{Connector, Reason};
use crate::launcher::LaunchRequest;
use crate::launcher::session::LaunchTarget;
use crate::mame::platform;
use crate::remote::connections::ConnectionsConfig;
use crate::remote::path::{RelativePath, RemotePath};
use crate::remote::provider::{EntryKind, RemoteError, RemoteRepository};
use crate::remote::sources::{CollectionOrganization, RemoteSourcesConfig, SourceCategory};

/// Whether a file exists locally or remotely.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Availability {
    /// Not found locally and not looked up remotely (no usable source).
    Missing,
    /// Found where MAME searches. Says nothing about its content.
    Present,
    /// Not found locally, found in the remote repository.
    RemoteAvailable,
    /// Not found locally nor in the remote repository.
    RemoteMissing,
    /// Not found locally; the remote lookup failed or the path cannot be built.
    Unresolved,
}

/// What a warning is about. Only `informational` warnings are expected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum WarningKind {
    /// A candidate file is not in the remote repository.
    Availability,
    /// An expected absence (for example a clone in a merged collection), or
    /// a remark about a local file.
    Informational,
    /// The ROM manager settings do not allow looking the file up.
    Configuration,
    /// The remote lookup failed for a technical reason.
    Unresolved,
    /// Two files would be written to the same local path.
    Conflict,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanWarning {
    pub kind: WarningKind,
    pub message: String,
}

impl PlanWarning {
    fn new(kind: WarningKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

/// One candidate file of a plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanArtifact {
    /// Stable within the plan: `<category>:<relative path>`.
    pub id: String,
    pub category: SourceCategory,
    pub set: String,
    pub reason: Reason,
    pub file_name: String,
    /// Below the collection root, with `/` separators.
    pub relative_path: String,
    pub connection_id: Option<String>,
    pub remote_path: Option<String>,
    /// Where a download is written.
    pub destination: Option<PathBuf>,
    /// The file MAME would find, when present.
    pub local: Option<LocalFile>,
    pub availability: Availability,
    pub remote_size: Option<u64>,
    pub remote_error: Option<RemoteError>,
    /// Whether the plan downloads this file.
    pub download: bool,
    pub warning: Option<PlanWarning>,
    #[serde(skip)]
    organization: Option<CollectionOrganization>,
    #[serde(skip)]
    absent_note: Option<AbsentNote>,
    /// The remote base directory of the source.
    #[serde(skip)]
    remote_base: Option<String>,
}

/// A connection that could not be used to look files up.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionIssue {
    pub connection_id: String,
    pub connection_name: String,
    pub error: RemoteError,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanSummary {
    pub present: usize,
    pub to_download: usize,
    /// Looked up and not found remotely.
    pub not_found: usize,
    /// Missing without a usable source, or unresolved.
    pub unresolved: usize,
    /// Warnings other than informational ones.
    pub warnings: usize,
    /// Sum of the known sizes of the files to download.
    pub download_bytes: u64,
    /// Files to download whose size is unknown.
    pub unknown_sizes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadPlan {
    pub id: String,
    /// The installation, launch request and sources the plan was built with.
    pub installation_id: String,
    pub installation_label: String,
    pub request: LaunchRequest,
    pub target: LaunchTarget,
    pub title: String,
    pub created_at: u64,
    pub artifacts: Vec<PlanArtifact>,
    /// Warnings not tied to one file.
    pub warnings: Vec<PlanWarning>,
    pub connection_issues: Vec<ConnectionIssue>,
    pub summary: PlanSummary,
}

/// Maps candidates to their paths and checks local presence. Blocking
/// (local file system).
pub fn build_artifacts(
    candidates: Vec<Candidate>,
    mame_dir: &Path,
    search: &SearchDirs,
    sources: &RemoteSourcesConfig,
    connections: &ConnectionsConfig,
) -> Vec<PlanArtifact> {
    candidates
        .into_iter()
        .map(|candidate| build_artifact(candidate, mame_dir, search, sources, connections))
        .collect()
}

fn category_key(category: SourceCategory) -> &'static str {
    match category {
        SourceCategory::MachineRoms => "machineRoms",
        SourceCategory::MachineChds => "machineChds",
        SourceCategory::SoftwareListRoms => "softwareListRoms",
        SourceCategory::SoftwareListChds => "softwareListChds",
        SourceCategory::Samples => "samples",
    }
}

fn build_artifact(
    candidate: Candidate,
    mame_dir: &Path,
    search: &SearchDirs,
    sources: &RemoteSourcesConfig,
    connections: &ConnectionsConfig,
) -> PlanArtifact {
    let relative_path = candidate.segments.join("/");
    let mut artifact = PlanArtifact {
        id: format!("{}:{relative_path}", category_key(candidate.category)),
        category: candidate.category,
        set: candidate.set,
        reason: candidate.reason,
        file_name: candidate.segments.last().cloned().unwrap_or_default(),
        relative_path,
        connection_id: None,
        remote_path: None,
        destination: None,
        local: None,
        availability: Availability::Missing,
        remote_size: None,
        remote_error: None,
        download: false,
        warning: None,
        organization: None,
        absent_note: candidate.absent_note,
        remote_base: None,
    };
    let segments = &candidate.segments;
    let relative = RelativePath::from_segments(segments)
        .ok()
        .filter(|_| segments.iter().all(|segment| is_portable_name(segment)));
    let Some(relative) = relative else {
        artifact.availability = Availability::Unresolved;
        artifact.warning = Some(PlanWarning::new(
            WarningKind::Unresolved,
            format!(
                "The name '{}' cannot be used as a file path.",
                artifact.relative_path
            ),
        ));
        return artifact;
    };

    if let Some(local) = find(search.for_category(artifact.category), segments) {
        artifact.availability = Availability::Present;
        artifact.warning = local
            .issue
            .clone()
            .map(|issue| PlanWarning::new(WarningKind::Informational, issue));
        artifact.local = Some(local);
    }

    let label = artifact.category.label();
    let source = sources
        .categories
        .get(&artifact.category)
        .filter(|source| source.enabled);
    let Some(source) = source else {
        if artifact.availability == Availability::Missing {
            artifact.warning = Some(PlanWarning::new(
                WarningKind::Configuration,
                format!("No remote source is enabled for {label}."),
            ));
        }
        return artifact;
    };
    artifact.organization = source.organization;
    artifact.destination = source.destination.as_ref().map(|destination| {
        let mut path = destination_dir(mame_dir, destination);
        path.extend(segments);
        path
    });
    let base = source
        .remote_base_path
        .as_deref()
        .and_then(|base| RemotePath::parse(base).ok());
    artifact.remote_path = base.as_ref().map(|base| base.join(&relative).to_string());
    artifact.remote_base = base.map(|base| base.to_string());
    artifact.connection_id = source
        .connection_id
        .clone()
        .filter(|id| connections.get(id).is_some());
    if artifact.availability == Availability::Missing && artifact.connection_id.is_none() {
        artifact.warning = Some(PlanWarning::new(
            WarningKind::Configuration,
            format!(
                "The remote connection of {label} no longer exists. Choose another one in the ROM Manager settings."
            ),
        ));
    }
    artifact
}

/// Looks up the missing files of `artifacts` that have a source, one
/// connection per repository. Connections that fail are reported; their
/// files stay unresolved.
pub async fn check_remote<C: Connector>(
    artifacts: &mut [PlanArtifact],
    connections: &ConnectionsConfig,
    connector: &C,
) -> Vec<ConnectionIssue> {
    let mut groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (index, artifact) in artifacts.iter().enumerate() {
        if artifact.availability != Availability::Missing || artifact.remote_path.is_none() {
            continue;
        }
        if let Some(id) = &artifact.connection_id {
            groups.entry(id.clone()).or_default().push(index);
        }
    }
    let mut issues = Vec::new();
    for (connection_id, indices) in groups {
        let Some(connection) = connections.get(&connection_id) else {
            continue;
        };
        let results = match connector.connect(connection).await {
            Ok(repository) => {
                let missing_bases = check_bases(&repository, artifacts, &indices).await;
                let (base_missing, lookups): (Vec<usize>, Vec<usize>) =
                    indices.iter().partition(|&&index| {
                        missing_bases
                            .iter()
                            .any(|(base, _)| artifacts[index].remote_base.as_ref() == Some(base))
                    });
                let paths: Vec<RemotePath> = lookups
                    .iter()
                    .filter_map(|&index| {
                        artifacts[index]
                            .remote_path
                            .as_deref()
                            .and_then(|path| RemotePath::parse(path).ok())
                    })
                    .collect();
                let checked = check_paths(&repository, &paths).await;
                repository.close().await;
                for &index in &base_missing {
                    apply_missing_base(&mut artifacts[index], &missing_bases);
                }
                // Results of the files whose base exists, in `indices` order.
                let mut checked = lookups.into_iter().zip(checked);
                let mut results = Vec::with_capacity(indices.len());
                for &index in &indices {
                    if base_missing.contains(&index) {
                        results.push(None);
                    } else {
                        results.push(checked.next().map(|(_, result)| result));
                    }
                }
                results
            }
            Err(error) => {
                issues.push(ConnectionIssue {
                    connection_id: connection_id.clone(),
                    connection_name: connection.name.clone(),
                    error: error.clone(),
                });
                vec![Some(RemoteCheck::Unresolved(error)); indices.len()]
            }
        };
        for (&index, result) in indices.iter().zip(results) {
            if let Some(result) = result {
                apply_check(&mut artifacts[index], result);
            }
        }
    }
    issues
}

/// The remote base directories (among those of `indices`) that do not exist
/// or are not directories, with the reason. A missing base means a wrong
/// configuration, not missing files: for example a path written as the
/// server's shell shows it while its SFTP service shows another tree
/// (Synology serves shared folders at the SFTP root, without `/volume1`).
/// Other failures are left to the lookups of the files.
async fn check_bases<R: RemoteRepository>(
    repository: &R,
    artifacts: &[PlanArtifact],
    indices: &[usize],
) -> Vec<(String, String)> {
    let mut bases: Vec<String> = Vec::new();
    for &index in indices {
        if let Some(base) = &artifacts[index].remote_base
            && !bases.contains(base)
        {
            bases.push(base.clone());
        }
    }
    let mut missing = Vec::new();
    for base in bases {
        let Ok(path) = RemotePath::parse(&base) else {
            continue;
        };
        let reason = match repository.stat(&path).await {
            Ok(metadata) if matches!(metadata.kind, EntryKind::Directory | EntryKind::Symlink) => {
                continue;
            }
            Ok(_) => "is not a directory",
            Err(RemoteError::NotFound { .. }) => "does not exist",
            Err(_) => continue,
        };
        missing.push((base, reason.to_owned()));
    }
    missing
}

fn apply_missing_base(artifact: &mut PlanArtifact, missing: &[(String, String)]) {
    let Some((base, reason)) = missing
        .iter()
        .find(|(base, _)| artifact.remote_base.as_ref() == Some(base))
    else {
        return;
    };
    artifact.availability = Availability::Unresolved;
    artifact.remote_error = Some(RemoteError::NotFound { path: base.clone() });
    artifact.warning = Some(PlanWarning::new(
        WarningKind::Configuration,
        format!(
            "The remote base path {base} of {} {reason} on the server, so its files were not looked up. Check the path with Browse… in Settings → Remote Connections: the SFTP service may show a different tree than a shell (on Synology, shared folders are at the root, without /volume1).",
            artifact.category.label()
        ),
    ));
}

fn apply_check(artifact: &mut PlanArtifact, check: RemoteCheck) {
    let remote_path = artifact.remote_path.clone().unwrap_or_default();
    match check {
        RemoteCheck::Available { size } => {
            artifact.availability = Availability::RemoteAvailable;
            artifact.remote_size = size;
        }
        RemoteCheck::Missing => {
            artifact.availability = Availability::RemoteMissing;
            let expected = artifact.absent_note.as_ref().filter(|note| {
                artifact
                    .organization
                    .is_some_and(|organization| note.organizations.contains(&organization))
            });
            artifact.warning = Some(match expected {
                Some(note) => PlanWarning::new(WarningKind::Informational, note.message.clone()),
                None => PlanWarning::new(
                    WarningKind::Availability,
                    format!("{remote_path} was not found in the remote repository."),
                ),
            });
        }
        RemoteCheck::Unresolved(error) => {
            artifact.availability = Availability::Unresolved;
            artifact.warning = Some(PlanWarning::new(
                WarningKind::Unresolved,
                format!("{remote_path} could not be looked up: {error}."),
            ));
            artifact.remote_error = Some(error);
        }
    }
}

/// Marks the files to download, refusing those whose destination is also
/// the destination of another file, and computes the summary.
pub fn finish(artifacts: &mut [PlanArtifact], warnings: &[PlanWarning]) -> PlanSummary {
    for artifact in artifacts.iter_mut() {
        artifact.download = artifact.availability == Availability::RemoteAvailable
            && artifact.destination.is_some();
    }
    let destinations: Vec<Option<PathBuf>> = artifacts
        .iter()
        .map(|artifact| artifact.destination.clone())
        .collect();
    let downloads: Vec<bool> = artifacts.iter().map(|artifact| artifact.download).collect();
    for index in 0..artifacts.len() {
        let Some(destination) = &destinations[index] else {
            continue;
        };
        let other = (0..artifacts.len()).find(|&other| {
            other != index
                && (downloads[index] || downloads[other])
                && destinations[other]
                    .as_deref()
                    .is_some_and(|path| platform::same_path(path, destination))
        });
        if let Some(other) = other {
            let other_id = artifacts[other].id.clone();
            let artifact = &mut artifacts[index];
            artifact.download = false;
            artifact.warning = Some(PlanWarning::new(
                WarningKind::Conflict,
                format!(
                    "{} and {other_id} would be written to the same file {}; neither is downloaded.",
                    artifact.id,
                    destination.display()
                ),
            ));
        }
    }

    let mut summary = PlanSummary {
        warnings: warnings
            .iter()
            .filter(|warning| warning.kind != WarningKind::Informational)
            .count(),
        ..PlanSummary::default()
    };
    for artifact in artifacts.iter() {
        match artifact.availability {
            Availability::Present => summary.present += 1,
            Availability::RemoteMissing => summary.not_found += 1,
            Availability::Missing | Availability::Unresolved => summary.unresolved += 1,
            Availability::RemoteAvailable => {}
        }
        if artifact.download {
            summary.to_download += 1;
            match artifact.remote_size {
                Some(size) => summary.download_bytes += size,
                None => summary.unknown_sizes += 1,
            }
        }
        if artifact
            .warning
            .as_ref()
            .is_some_and(|warning| warning.kind != WarningKind::Informational)
        {
            summary.warnings += 1;
        }
    }
    summary
}

#[cfg(test)]
impl PlanArtifact {
    /// A machine ROM archive to download, for tests of the commands.
    pub fn available(
        set: &str,
        connection_id: &str,
        remote_path: &str,
        destination: PathBuf,
    ) -> Self {
        Self {
            id: format!("machineRoms:{set}.zip"),
            category: SourceCategory::MachineRoms,
            set: set.to_owned(),
            reason: Reason::Selected,
            file_name: format!("{set}.zip"),
            relative_path: format!("{set}.zip"),
            connection_id: Some(connection_id.to_owned()),
            remote_path: Some(remote_path.to_owned()),
            destination: Some(destination),
            local: None,
            availability: Availability::RemoteAvailable,
            remote_size: None,
            remote_error: None,
            download: true,
            warning: None,
            organization: None,
            absent_note: None,
            remote_base: None,
        }
    }
}
