//! Controller profile and player mapping options of a launch.
//!
//! After the launch options are resolved, the controls of the launch
//! ([`crate::controllers::resolve`]) contribute MAME options with the source
//! [`OptionSource::Controllers`]:
//!
//! - the system profile `mame` without mapped players contributes nothing:
//!   MAME's own configuration and the launch options stay in charge, exactly
//!   as without profiles;
//! - a user profile passes `-cfg_directory <its directory for the
//!   installation>`, replacing a `cfg_directory` launch option (with a
//!   warning), and either `-ctrlrpath <profile dir> -ctrlr base` when it has a
//!   `base.cfg`, or `-ctrlr ""`, so that a `ctrlr` set in `mame.ini` or another
//!   INI file is not applied on top of the profile (an empty value makes MAME
//!   skip the controller file; verified with MAME 0.289);
//! - players assigned controllers are mapped to their joystick index with a
//!   generated controller file ([`crate::controllers::ctrlr_file`]):
//!   `-ctrlrpath <generated dir> -ctrlr ymdb-<hash>`, which keeps the
//!   sections of the baseline (the profile's `base.cfg`, or for the `mame`
//!   profile the controller file of `mame.ini`), `-joystickprovider` of the
//!   controllers' device ids and `-verbose`, whose output shows which
//!   mappings MAME applied. The `mame` profile keeps MAME's configuration
//!   directory. A controller file set by another INI file (per machine,
//!   source, orientation...) cannot be composed safely: the launch is refused.
//!
//! [`compose`] has no side effects (it only reads the baseline files), so
//! the preview shows exactly what a launch runs. [`materialize`] creates the
//! profile's directory and writes the generated file right before MAME starts.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::config::{OptionSource, ResolvedOption};
use super::options::OptionValue;
use crate::atomic_file;
use crate::controllers::base_file;
use crate::controllers::ctrlr_file::{self, Baseline, PlayerDevice};
use crate::controllers::devices::JoystickProvider;
use crate::controllers::model::{
    Assignment, ControllerBinding, ControllerProfile, ControllersConfig, ControllersError, Players,
};
use crate::controllers::resolve::{self, ControlsSource};
use crate::controllers::{BASE_NAME, ControllerPaths};
use crate::mame::ini::{self, NativeInputSettings};

/// What controller management needs to prepare a launch.
#[derive(Debug, Clone, Copy)]
pub struct ControlsContext<'a> {
    pub config: &'a ControllersConfig,
    pub paths: &'a ControllerPaths,
}

/// A player mapped to the device of a controller.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanMapping {
    /// 1-based player number, also the MAME joystick index (`JOYCODE_<n>`).
    pub player: u32,
    pub controller_id: String,
    pub controller_name: String,
    pub provider: JoystickProvider,
    pub device_id: String,
    pub binding: ControllerBinding,
}

/// The generated controller file of a plan; written when MAME starts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedControllerFile {
    /// The `-ctrlr` value.
    pub name: String,
    pub path: PathBuf,
    pub hash: String,
    pub content: String,
    /// The file whose sections it keeps: the profile's `base.cfg` or the
    /// controller file of `mame.ini`.
    pub baseline: Option<PathBuf>,
}

/// The controls applied by a plan, shown by the preview.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanControls {
    pub profile_id: String,
    pub profile_name: String,
    pub profile_source: ControlsSource,
    /// Controller ids by player, Player 1 first.
    pub players: Players,
    pub players_source: Option<ControlsSource>,
    /// MAME's configuration directory for a user profile (its persistence
    /// context), created when MAME starts; `None` for the `mame` profile.
    pub cfg_directory: Option<PathBuf>,
    /// The profile's baseline controller file.
    pub base_file: Option<PathBuf>,
    /// The players mapped to devices, Player 1 first.
    pub mappings: Vec<PlanMapping>,
    /// The `-joystickprovider` of the mappings.
    pub joystick_provider: Option<JoystickProvider>,
    /// The controller file of the mappings.
    pub controller_file: Option<PlannedControllerFile>,
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
            mappings: Vec::new(),
            joystick_provider: None,
            controller_file: None,
        }
    }
}

/// Resolves the controls of a launch and adds their options. `mame_dir` is
/// the installation's directory (for its INI files). Warnings are appended
/// to `warnings`.
#[allow(clippy::too_many_arguments)]
pub fn compose(
    options: &mut Vec<ResolvedOption>,
    warnings: &mut Vec<String>,
    context: ControlsContext<'_>,
    installation_id: &str,
    mame_dir: &Path,
    machine: &str,
    software: Option<(&str, &str)>,
    request: Option<&Assignment>,
) -> Result<PlanControls, ControllersError> {
    let resolved = resolve::resolve(context.config, machine, software, request)?;
    let profile = context
        .config
        .profile(&resolved.profile_id)
        .ok_or_else(|| ControllersError::NotFound(resolved.profile_id.clone()))?;
    let (mappings, provider) = player_mappings(context.config, &resolved.players, warnings)?;
    let mut controls = PlanControls {
        profile_id: profile.id.clone(),
        profile_name: profile.display_name().to_owned(),
        profile_source: resolved.profile_source,
        players: resolved.players,
        players_source: resolved.players_source,
        cfg_directory: None,
        base_file: None,
        mappings,
        joystick_provider: provider,
        controller_file: None,
    };

    // The baseline of a generated controller file: label, path and content.
    let mut baseline: Option<(String, PathBuf, Vec<u8>)> = None;
    if profile.is_managed() {
        set_cfg_directory(
            options,
            warnings,
            context,
            installation_id,
            profile,
            &mut controls,
        )?;
        if profile.has_base {
            let base_file = context.paths.base_file(&profile.id);
            let content = read_base(&base_file, &controls.profile_name)?;
            if controls.mappings.is_empty() {
                let directory = path_value(&context.paths.profile_dir(&profile.id))?;
                options.push(controls_option("ctrlrpath", directory));
                options.push(controls_option("ctrlr", BASE_NAME.to_owned()));
            } else {
                baseline = Some((
                    format!(
                        "the controller file of the profile '{}'",
                        controls.profile_name
                    ),
                    base_file.clone(),
                    content,
                ));
            }
            controls.base_file = Some(base_file);
        } else if controls.mappings.is_empty() {
            // An empty value makes MAME skip any controller file set in its INI files.
            options.push(controls_option("ctrlr", String::new()));
        }
    } else if !controls.mappings.is_empty() {
        let native = ini::read_input_settings(mame_dir);
        baseline = native_baseline(&native, mame_dir)?;
        if let (Some(value), Some(provider)) =
            (&native.joystickprovider, controls.joystick_provider)
            && value != provider.name()
            && !(value == "auto" && JoystickProvider::available().first() == Some(&provider))
        {
            warnings.push(format!(
                "mame.ini sets the joystick provider '{value}': this launch uses '{}', the provider of the assigned controllers' device ids.",
                provider.name()
            ));
        }
    }

    if let Some(provider) = controls.joystick_provider {
        let devices: Vec<PlayerDevice> = controls
            .mappings
            .iter()
            .map(|mapping| PlayerDevice {
                player: mapping.player,
                device_id: mapping.device_id.clone(),
            })
            .collect();
        let file = ctrlr_file::compose(
            &devices,
            baseline
                .as_ref()
                .map(|(label, _, content)| Baseline { label, content }),
        )
        .map_err(ControllersError::Invalid)?;
        let directory = context.paths.generated_dir();
        options.push(controls_option("ctrlrpath", path_value(directory)?));
        options.push(controls_option("ctrlr", file.name.clone()));
        options.push(controls_option(
            "joystickprovider",
            provider.name().to_owned(),
        ));
        options.push(ResolvedOption {
            name: "verbose".to_owned(),
            value: OptionValue::Boolean(true),
            source: OptionSource::Controllers,
        });
        controls.controller_file = Some(PlannedControllerFile {
            path: directory.join(file.file_name()),
            name: file.name,
            hash: file.hash,
            content: file.content,
            baseline: baseline.map(|(_, path, _)| path),
        });
    }
    Ok(controls)
}

/// The mapped players of `players` and their common joystick provider.
fn player_mappings(
    config: &ControllersConfig,
    players: &Players,
    warnings: &mut Vec<String>,
) -> Result<(Vec<PlanMapping>, Option<JoystickProvider>), ControllersError> {
    let invalid = ControllersError::Invalid;
    let mut mappings = Vec::new();
    for (index, id) in players.iter().enumerate() {
        let Some(id) = id else {
            continue;
        };
        let controller = config.controller(id).ok_or_else(|| {
            ControllersError::NotFound(format!("the controller '{id}' does not exist"))
        })?;
        let player = u32::try_from(index + 1).unwrap_or(u32::MAX);
        let device = controller
            .device_binding()
            .map_err(|error| invalid(format!("Player {player}: {error}")))?;
        let provider = JoystickProvider::from_name(device.provider)
            .filter(|provider| provider.is_available())
            .ok_or_else(|| {
                invalid(format!(
                    "Player {player}: the joystick provider '{}' of '{}' is not available on this operating system",
                    device.provider, controller.name
                ))
            })?;
        mappings.push(PlanMapping {
            player,
            controller_id: controller.id.clone(),
            controller_name: controller.name.clone(),
            provider,
            device_id: device.device_id.to_owned(),
            binding: device.binding,
        });
    }
    let Some(first) = mappings.first() else {
        return Ok((mappings, None));
    };
    let provider = first.provider;
    if let Some(other) = mappings.iter().find(|mapping| mapping.provider != provider) {
        return Err(invalid(format!(
            "the controllers assigned to players use different joystick providers ('{}' uses {}, '{}' uses {}): MAME uses one provider per launch, so detect them with the same provider",
            first.controller_name,
            provider.name(),
            other.controller_name,
            other.provider.name()
        )));
    }
    let last = mappings.last().map_or(0, |mapping| mapping.player);
    for player in 1..last {
        if !mappings.iter().any(|mapping| mapping.player == player) {
            warnings.push(format!(
                "Player {player} has no assigned controller: MAME gives it the joystick that ends up number {player} after the mapping."
            ));
        }
    }
    let port_order: Vec<&str> = mappings
        .iter()
        .filter(|mapping| mapping.binding == ControllerBinding::PortOrder)
        .map(|mapping| mapping.controller_name.as_str())
        .collect();
    if !port_order.is_empty() {
        warnings.push(format!(
            "Identified by USB port or connection order: {}. Swapping ports may change player assignments.",
            port_order.join(", ")
        ));
    }
    Ok((mappings, Some(provider)))
}

fn set_cfg_directory(
    options: &mut Vec<ResolvedOption>,
    warnings: &mut Vec<String>,
    context: ControlsContext<'_>,
    installation_id: &str,
    profile: &ControllerProfile,
    controls: &mut PlanControls,
) -> Result<(), ControllersError> {
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
    controls.cfg_directory = Some(cfg_directory);
    Ok(())
}

/// Reads and checks a profile's `base.cfg`. A file imported before its
/// checks became stricter is reported, never changed or removed.
fn read_base(base_file: &Path, profile_name: &str) -> Result<Vec<u8>, ControllersError> {
    let content = match fs::read(base_file) {
        Ok(content) => content,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(ControllersError::Invalid(format!(
                "the controller file of the profile '{profile_name}' is missing ({}): import it again",
                base_file.display()
            )));
        }
        Err(error) => {
            return Err(ControllersError::Invalid(format!(
                "the controller file of the profile '{profile_name}' cannot be read ({}): {error}",
                base_file.display()
            )));
        }
    };
    base_file::validate(&content).map_err(|error| {
        ControllersError::Invalid(format!(
            "the controller file of the profile '{profile_name}' cannot be used: {error}. Import a corrected file ({} is kept as it is)",
            base_file.display()
        ))
    })?;
    Ok(content)
}

/// The controller file of `mame.ini`, kept by a generated file of the `mame`
/// profile. Refused when another INI file sets one, since MAME applies those
/// by machine, parent, source file or screen type.
fn native_baseline(
    native: &NativeInputSettings,
    mame_dir: &Path,
) -> Result<Option<(String, PathBuf, Vec<u8>)>, ControllersError> {
    if let Some(file) = native.other_ctrlr.first() {
        return Err(ControllersError::Invalid(format!(
            "MAME's INI file '{}' sets a controller file (ctrlr), which YMDB cannot combine safely with the player assignments: use a controller profile with that file as its base, or remove the player assignments",
            file.display()
        )));
    }
    let Some(ctrlr) = &native.ctrlr else {
        return Ok(None);
    };
    let label = format!("the controller file '{ctrlr}' of mame.ini");
    let file_name = format!("{ctrlr}.cfg");
    let path = native
        .ctrlrpath
        .iter()
        .map(|entry| ini::resolve_entry(mame_dir, entry).join(&file_name))
        .find(|path| path.is_file())
        .ok_or_else(|| {
            ControllersError::Invalid(format!(
                "{label} is not in its ctrlrpath ({}): MAME could not start with it",
                native.ctrlrpath.join(";")
            ))
        })?;
    let content = fs::read(&path).map_err(|error| {
        ControllersError::Invalid(format!(
            "{label} cannot be read ({}): {error}",
            path.display()
        ))
    })?;
    if content.len() as u64 > base_file::MAX_FILE_SIZE {
        return Err(ControllersError::Invalid(format!(
            "{label} is too large for a controller configuration file"
        )));
    }
    Ok(Some((label, path, content)))
}

/// Creates what a plan needs before MAME starts: the configuration directory
/// of a user profile and the generated controller file. Checks that the
/// profile's `base.cfg` is still there. A generated file already present
/// with the same content is kept; any other content is replaced atomically.
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
    if let Some(file) = &controls.controller_file {
        let current = fs::read(&file.path).ok();
        if current.as_deref() != Some(file.content.as_bytes()) {
            atomic_file::write(&file.path, file.content.as_bytes())?;
        }
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
            "the YMDB directory '{}' cannot be passed to MAME (it contains ';' or invalid characters)",
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
