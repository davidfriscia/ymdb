//! MAME launcher: builds and runs MAME command lines for catalog machines and
//! software.
//!
//! - [`options`]: supported MAME options and their validation.
//! - [`config`]: global options, profiles, overrides and their precedence.
//! - [`store`]: `launch.json` persistence.
//! - [`resolve`]: catalog lookups (machine, software, candidate machines).
//! - [`command`]: the structured command line used for preview and execution.
//! - [`search_paths`]: search paths extended with the ROM manager destinations.
//! - [`session`]: started MAME processes.
//!
//! [`prepare`] is the single path from a launch request to a command line,
//! used by both the preview and the launch.

pub mod command;
pub mod config;
pub mod options;
pub mod resolve;
pub mod search_paths;
pub mod session;
pub mod store;

use std::fmt;
use std::path::Path;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::catalog::is_short_name;
use command::{LaunchPlan, PlanOrigin, SoftwareMedia};
use config::LaunchConfig;
use resolve::{ListRelation, MediaAssignment};
use search_paths::RequiredSearchPaths;
use session::{LaunchTarget, SoftwareRef};

/// A launch requested by the user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LaunchRequest {
    /// The MAME installation whose executable, catalog and launch
    /// configuration are used.
    pub installation_id: String,
    pub machine: String,
    #[serde(default)]
    pub software: Option<SoftwareSelection>,
    /// `None` uses the default profile.
    #[serde(default)]
    pub profile_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SoftwareSelection {
    pub list: String,
    pub name: String,
    /// Parts to mount and the media devices receiving them.
    pub media: Vec<MediaAssignment>,
}

/// A validated launch, ready to preview or start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedLaunch {
    pub plan: LaunchPlan,
    pub target: LaunchTarget,
    pub title: String,
    /// Compatibility caveats the user should know about.
    pub warnings: Vec<String>,
}

/// What the preview shows: the same plan that a launch would execute.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchPreview {
    pub plan: LaunchPlan,
    pub command_line: Vec<String>,
    pub warnings: Vec<String>,
}

impl From<PreparedLaunch> for LaunchPreview {
    fn from(prepared: PreparedLaunch) -> Self {
        Self {
            command_line: prepared.plan.command_line(),
            plan: prepared.plan,
            warnings: prepared.warnings,
        }
    }
}

#[derive(Debug)]
pub enum LaunchError {
    /// The request cannot be launched as is.
    Invalid(String),
    /// A machine or software is not in the catalog.
    NotFound(String),
    Database(rusqlite::Error),
}

impl fmt::Display for LaunchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(message) | Self::NotFound(message) => f.write_str(message),
            Self::Database(error) => write!(f, "catalog error: {error}"),
        }
    }
}

impl std::error::Error for LaunchError {}

impl From<rusqlite::Error> for LaunchError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Database(error)
    }
}

/// Validates a request against the catalog and the launch configuration of
/// its installation and builds its command line. `executable` is the
/// installation's executable; `search_paths` lists the directories the
/// installation's remote sources need MAME to search.
pub fn prepare(
    connection: &Connection,
    executable: &Path,
    config: &LaunchConfig,
    search_paths: &RequiredSearchPaths,
    request: &LaunchRequest,
) -> Result<PreparedLaunch, LaunchError> {
    let invalid = |message: String| LaunchError::Invalid(message);
    if !is_short_name(&request.machine) {
        return Err(invalid(format!(
            "invalid machine name '{}'",
            request.machine
        )));
    }
    let machine = resolve::machine(connection, &request.machine)?.ok_or_else(|| {
        LaunchError::NotFound(format!(
            "machine '{}' is not in the catalog",
            request.machine
        ))
    })?;
    if !machine.is_runnable {
        return Err(invalid(format!(
            "'{}' is a device or component that MAME cannot run on its own",
            machine.name
        )));
    }

    let mut warnings = Vec::new();
    let (target, title) = match &request.software {
        None => (
            LaunchTarget {
                machine: machine.name.clone(),
                software: None,
            },
            machine.description.clone(),
        ),
        Some(selection) => {
            if !is_short_name(&selection.list) || !is_short_name(&selection.name) {
                return Err(invalid(format!(
                    "invalid software '{}:{}'",
                    selection.list, selection.name
                )));
            }
            let info = resolve::software_launch_info(connection, &selection.list, &selection.name)?
                .ok_or_else(|| {
                    LaunchError::NotFound(format!(
                        "software '{}:{}' is not in the catalog",
                        selection.list, selection.name
                    ))
                })?;
            let candidate = info
                .candidates
                .iter()
                .find(|candidate| candidate.name == machine.name)
                .ok_or_else(|| {
                    invalid(format!(
                        "'{}' does not declare the software list '{}'",
                        machine.name, selection.list
                    ))
                })?;
            resolve::validate_media(&candidate.devices, &info.parts, &selection.media)
                .map_err(invalid)?;
            if candidate.relation == ListRelation::Compatible {
                warnings.push(format!(
                    "{} declares {} as a compatible list, not its own: the software may not work fully.",
                    machine.name, selection.list
                ));
            }
            if !candidate.filter_match {
                warnings.push(format!(
                    "The list filter of {} ({}) does not match this software: MAME warns that it might not work.",
                    machine.name,
                    candidate.filters.join(", ")
                ));
            }
            (
                LaunchTarget {
                    machine: machine.name.clone(),
                    software: Some(SoftwareRef {
                        list: selection.list.clone(),
                        name: selection.name.clone(),
                    }),
                },
                format!("{} ({})", info.description, machine.description),
            )
        }
    };

    let software = target
        .software
        .as_ref()
        .map(|software| (software.list.as_str(), software.name.as_str()));
    let profile_id = request
        .profile_id
        .as_deref()
        .unwrap_or(&config.default_profile_id);
    let mut options = config
        .resolve(Some(profile_id), &machine.name, software)
        .map_err(|error| invalid(error.0))?;
    if let Some(mame_dir) = executable.parent() {
        search_paths::compose(&mut options, search_paths, mame_dir);
    }
    let media = request.software.as_ref().map(|selection| SoftwareMedia {
        list: &selection.list,
        software: &selection.name,
        media: &selection.media,
    });
    let origin = PlanOrigin {
        installation_id: &request.installation_id,
        profile_id,
    };
    let plan = LaunchPlan::build(&origin, executable, &machine.name, media.as_ref(), &options);
    Ok(PreparedLaunch {
        plan,
        target,
        title,
        warnings,
    })
}

#[cfg(test)]
mod tests;
