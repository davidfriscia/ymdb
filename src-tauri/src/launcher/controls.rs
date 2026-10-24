//! Controller profile options of a launch.
//!
//! After the launch options are resolved, the controller profile of the
//! launch ([`crate::controllers::resolve`]) contributes MAME options with the
//! source [`OptionSource::Controllers`]:
//!
//! - the system profile `mame` contributes nothing: MAME's own configuration
//!   and the launch options stay in charge, exactly as without profiles;
//! - a user profile passes `-cfg_directory <its directory for the
//!   installation>`, replacing a `cfg_directory` launch option (with a
//!   warning), and either `-ctrlrpath <profile dir> -ctrlr base` when it has a
//!   `base.cfg`, or `-ctrlr ""`, so that a `ctrlr` set in `mame.ini` or another
//!   INI file is not applied on top of the profile (an empty value makes MAME
//!   skip the controller file; verified with MAME 0.289).
//!
//! [`compose`] has no side effects, so the preview shows exactly what a
//! launch runs. [`materialize`] creates the profile's directory right before
//! MAME starts.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::config::{OptionSource, ResolvedOption};
use super::options::OptionValue;
use crate::controllers::model::{Assignment, ControllersConfig, ControllersError};
use crate::controllers::resolve::{self, ControlsSource};
use crate::controllers::{BASE_NAME, ControllerPaths};

/// What controller management needs to prepare a launch.
#[derive(Debug, Clone, Copy)]
pub struct ControlsContext<'a> {
    pub config: &'a ControllersConfig,
    pub paths: &'a ControllerPaths,
}

/// The controls applied by a plan, shown by the preview.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanControls {
    pub profile_id: String,
    pub profile_name: String,
    pub profile_source: ControlsSource,
    /// Controller ids by player: preferences only, not bound to devices yet.
    pub players: Vec<String>,
    pub players_source: Option<ControlsSource>,
    /// MAME's configuration directory for a user profile (its persistence
    /// context), created when MAME starts; `None` for the `mame` profile.
    pub cfg_directory: Option<PathBuf>,
    /// The baseline controller file passed with `-ctrlr base`.
    pub base_file: Option<PathBuf>,
}

impl PlanControls {
    /// Whether the plan uses a configuration directory managed by YMDB.
    pub fn is_managed(&self) -> bool {
        self.cfg_directory.is_some()
    }

    /// The controls of the `mame` profile with nothing assigned.
    #[cfg(test)]
    pub fn native() -> Self {
        Self {
            profile_id: crate::controllers::model::MAME_PROFILE.to_owned(),
            profile_name: "MAME configuration".to_owned(),
            profile_source: ControlsSource::Default,
            players: Vec::new(),
            players_source: None,
            cfg_directory: None,
            base_file: None,
        }
    }
}

/// Resolves the controls of a launch and adds their options. Warnings are
/// appended to `warnings`.
pub fn compose(
    options: &mut Vec<ResolvedOption>,
    warnings: &mut Vec<String>,
    context: ControlsContext<'_>,
    installation_id: &str,
    machine: &str,
    software: Option<(&str, &str)>,
    request: Option<&Assignment>,
) -> Result<PlanControls, ControllersError> {
    let resolved = resolve::resolve(context.config, machine, software, request)?;
    let profile = context
        .config
        .profile(&resolved.profile_id)
        .ok_or_else(|| ControllersError::NotFound(resolved.profile_id.clone()))?;
    let mut controls = PlanControls {
        profile_id: profile.id.clone(),
        profile_name: profile.display_name().to_owned(),
        profile_source: resolved.profile_source,
        players: resolved.players,
        players_source: resolved.players_source,
        cfg_directory: None,
        base_file: None,
    };
    if !controls.players.is_empty() {
        warnings.push(
            "Controller assignments are not applied to MAME yet: MAME numbers the devices in the order the system reports them."
                .to_owned(),
        );
    }
    if !profile.is_managed() {
        return Ok(controls);
    }

    let cfg_directory = context.paths.context_dir(installation_id, &profile.id);
    let value = path_value(&cfg_directory)?;
    let replaced = options
        .iter()
        .position(|option| option.name == "cfg_directory");
    let option = ResolvedOption {
        name: "cfg_directory".to_owned(),
        value: OptionValue::Text(value),
        source: OptionSource::Controllers,
    };
    match replaced {
        Some(index) => {
            warnings.push(format!(
                "The launch option cfg_directory ({}) is not used: the controller profile '{}' keeps MAME's configuration in its own directory.",
                source_label(options[index].source),
                controls.profile_name
            ));
            options[index] = option;
        }
        None => options.push(option),
    }

    if profile.has_base {
        let base_file = context.paths.base_file(&profile.id);
        if !base_file.is_file() {
            return Err(ControllersError::Invalid(format!(
                "the controller file of the profile '{}' is missing ({}): import it again",
                controls.profile_name,
                base_file.display()
            )));
        }
        let directory = path_value(&context.paths.profile_dir(&profile.id))?;
        options.push(controls_option("ctrlrpath", directory));
        options.push(controls_option("ctrlr", BASE_NAME.to_owned()));
        controls.base_file = Some(base_file);
    } else {
        // An empty value makes MAME skip any controller file set in its INI files.
        options.push(controls_option("ctrlr", String::new()));
    }
    controls.cfg_directory = Some(cfg_directory);
    Ok(controls)
}

/// Creates what a plan needs before MAME starts: the configuration directory
/// of a user profile. Checks that its `base.cfg` is still there.
pub fn materialize(controls: &PlanControls) -> io::Result<()> {
    if let Some(base_file) = &controls.base_file
        && !base_file.is_file()
    {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "the controller file of the profile '{}' is missing ({})",
                controls.profile_name,
                base_file.display()
            ),
        ));
    }
    if let Some(directory) = &controls.cfg_directory {
        fs::create_dir_all(directory)?;
    }
    Ok(())
}

fn controls_option(name: &str, value: String) -> ResolvedOption {
    ResolvedOption {
        name: name.to_owned(),
        value: OptionValue::Text(value),
        source: OptionSource::Controllers,
    }
}

/// MAME reads `;` as a separator in its path options.
fn path_value(path: &Path) -> Result<String, ControllersError> {
    match path.to_str() {
        Some(text) if !text.contains(';') => Ok(text.to_owned()),
        _ => Err(ControllersError::Invalid(format!(
            "the YMDB configuration directory '{}' cannot be passed to MAME (it contains ';' or invalid characters)",
            path.display()
        ))),
    }
}

fn source_label(source: OptionSource) -> &'static str {
    match source {
        OptionSource::Global => "from the global options",
        OptionSource::Profile => "from the launch profile",
        OptionSource::Machine => "from the machine options",
        OptionSource::Software => "from the software options",
        OptionSource::RomManager => "from the ROM manager",
        OptionSource::Controllers => "from the controller profile",
    }
}
