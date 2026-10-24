//! Controllers, controller profiles, assignments and their rules.

use std::collections::{BTreeMap, HashSet};
use std::fmt;

use serde::{Deserialize, Serialize};

use super::devices::{self, JoystickProvider};
use crate::catalog::is_short_name;

pub const SCHEMA_VERSION: u32 = 1;
/// Id of the system profile that leaves MAME's own configuration in charge.
pub const MAME_PROFILE: &str = "mame";
/// System profiles: they always exist and are named by the application.
pub const SYSTEM_PROFILES: &[(&str, &str)] = &[(MAME_PROFILE, "MAME configuration")];
/// MAME has inputs for players 1 to 10.
pub const MAX_PLAYERS: usize = 10;
const MAX_CONTROLLERS: usize = 100;
const MAX_PROFILES: usize = 100;
const MAX_ASSIGNMENTS: usize = 100_000;
const MAX_ID_LENGTH: usize = 40;
const MAX_NAME_LENGTH: usize = 60;
const MAX_DEVICE_ID_LENGTH: usize = 512;
const MAX_PROVIDER_LENGTH: usize = 32;
const MAX_FILE_NAME_LENGTH: usize = 255;

/// What a controller is, for display only: MAME does not use it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ControllerKind {
    ArcadeStick,
    Gamepad,
    Wheel,
    Trackball,
    Lightgun,
    Keyboard,
    Mouse,
    Other,
}

/// The MAME device class a controller belongs to (`JOYCODE`, `GUNCODE`,
/// `MOUSECODE`, `KEYCODE`). Only joysticks are mapped to players.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DeviceClass {
    Joystick,
    Lightgun,
    Mouse,
    Keyboard,
}

/// What a controller's device id identifies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ControllerBinding {
    /// The id identifies one specific device wherever it is connected (for
    /// example a device with its own USB serial number). Asserted by the user.
    Device,
    /// The id follows the port or the enumeration order of the provider: it
    /// names whatever device the provider reports there, not a specific one.
    PortOrder,
}

/// The logical identity of a physical device. With a device id, a provider
/// and a binding, the controller can be assigned to players: launches then
/// map it to their MAME joystick index (`mapdevice`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Controller {
    /// Stable identifier; never the displayed name.
    pub id: String,
    pub name: String,
    pub kind: ControllerKind,
    pub device_class: DeviceClass,
    /// The complete MAME device id reported for the device by `provider`
    /// (what `mapdevice` matches).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
    /// MAME joystick provider the device id was obtained with (for example
    /// `winhybrid`); device ids differ between providers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    /// `None` for controllers registered before device mapping: they are not
    /// mapped until the user completes their device configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binding: Option<ControllerBinding>,
}

/// The device a controller is mapped to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceBinding<'a> {
    pub provider: &'a str,
    pub device_id: &'a str,
    pub binding: ControllerBinding,
}

impl Controller {
    /// The device of a controller that can be mapped to a player; the error
    /// says what is missing.
    pub fn device_binding(&self) -> Result<DeviceBinding<'_>, String> {
        if self.device_class != DeviceClass::Joystick {
            return Err(format!(
                "'{}' is not a joystick controller: only joysticks and gamepads can be mapped to players",
                self.name
            ));
        }
        match (&self.provider, &self.device_id, self.binding) {
            (Some(provider), Some(device_id), Some(binding)) => Ok(DeviceBinding {
                provider,
                device_id,
                binding,
            }),
            _ => Err(format!(
                "the device of '{}' is not configured: detect the devices, then set its provider, device id and binding",
                self.name
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProfileKind {
    /// Defined by the application (`mame`).
    System,
    /// Created by the user.
    User,
}

/// A reusable MAME input configuration. A user profile owns one MAME
/// configuration directory per installation and an optional controller
/// configuration file (`base.cfg`) used as the baseline of its inputs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ControllerProfile {
    /// Stable identifier; never the displayed name.
    pub id: String,
    pub kind: ProfileKind,
    /// Name of a user profile; system profiles are named by the application.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Whether the profile has an imported `base.cfg`.
    #[serde(default)]
    pub has_base: bool,
    /// Name of the file `base.cfg` was imported from, for display only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_file_name: Option<String>,
}

impl ControllerProfile {
    pub fn system(id: &str) -> Self {
        Self {
            id: id.to_owned(),
            kind: ProfileKind::System,
            name: None,
            has_base: false,
            base_file_name: None,
        }
    }

    pub fn display_name(&self) -> &str {
        match self.kind {
            ProfileKind::System => system_name(&self.id).unwrap_or(&self.id),
            ProfileKind::User => self.name.as_deref().unwrap_or(&self.id),
        }
    }

    /// Whether YMDB manages the profile's configuration directory; only the
    /// system profile leaves MAME's own configuration in charge.
    pub fn is_managed(&self) -> bool {
        self.kind == ProfileKind::User
    }

    fn validate(&self) -> Result<(), String> {
        match self.kind {
            ProfileKind::System => {
                if system_name(&self.id).is_none() {
                    return Err(format!("unknown system profile '{}'", self.id));
                }
                if self.name.is_some() {
                    return Err(format!(
                        "the system profile '{}' cannot be renamed",
                        self.id
                    ));
                }
                if self.has_base || self.base_file_name.is_some() {
                    return Err(format!(
                        "the system profile '{}' cannot have a base file",
                        self.id
                    ));
                }
            }
            ProfileKind::User => {
                validate_id("profile", &self.id)?;
                if system_name(&self.id).is_some() {
                    return Err(format!("the profile id '{}' is reserved", self.id));
                }
                validate_name(&format!("the profile '{}'", self.id), self.name.as_deref())?;
                if let Some(file_name) = &self.base_file_name
                    && (!self.has_base
                        || file_name.trim().is_empty()
                        || file_name.chars().count() > MAX_FILE_NAME_LENGTH
                        || file_name.chars().any(char::is_control))
                {
                    return Err(format!(
                        "invalid base file name for the profile '{}'",
                        self.id
                    ));
                }
            }
        }
        Ok(())
    }
}

pub fn system_name(id: &str) -> Option<&'static str> {
    SYSTEM_PROFILES
        .iter()
        .find(|(system_id, _)| *system_id == id)
        .map(|(_, name)| *name)
}

/// Controller ids by player, Player 1 first; `None` leaves a player without
/// a controller.
pub type Players = Vec<Option<String>>;

/// Controller preferences at one level. Each field is inherited separately
/// from the weaker levels when it is `None`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Assignment {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<String>,
    /// Controller ids by player (Player 1 first). `Some(vec![])` explicitly
    /// assigns no controller.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub players: Option<Players>,
}

impl Assignment {
    pub fn is_empty(&self) -> bool {
        self.profile_id.is_none() && self.players.is_none()
    }

    /// Drops the players without a controller at the end, so that equal
    /// assignments are stored identically.
    pub fn normalized(mut self) -> Self {
        if let Some(players) = &mut self.players {
            while players.last().is_some_and(Option::is_none) {
                players.pop();
            }
        }
        self
    }
}

/// A change of one field of an assignment.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum FieldUpdate<T> {
    /// The field keeps its stored value.
    #[default]
    Keep,
    Set {
        value: T,
    },
    /// The field is removed, so it is inherited from the weaker levels again.
    Inherit,
}

impl<T> FieldUpdate<T> {
    fn apply(self, field: &mut Option<T>) {
        match self {
            Self::Keep => {}
            Self::Set { value } => *field = Some(value),
            Self::Inherit => *field = None,
        }
    }
}

/// Changes to some fields of an assignment; the fields left out are kept.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AssignmentUpdate {
    #[serde(default)]
    pub profile_id: FieldUpdate<String>,
    #[serde(default)]
    pub players: FieldUpdate<Players>,
}

/// The levels an assignment can be stored at, from the weakest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AssignmentLevel {
    Default,
    Machine,
    Software,
    MachineSoftware,
}

/// Assignments by level. Keys use MAME names only: `<machine>`,
/// `<list>:<software>` and `<machine>/<list>:<software>`. Keys of machines
/// or software missing from a catalog are kept.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Assignments {
    #[serde(default = "default_assignment")]
    pub default: Assignment,
    #[serde(default)]
    pub machines: BTreeMap<String, Assignment>,
    #[serde(default)]
    pub software: BTreeMap<String, Assignment>,
    #[serde(default)]
    pub machine_software: BTreeMap<String, Assignment>,
}

fn default_assignment() -> Assignment {
    Assignment {
        profile_id: Some(MAME_PROFILE.to_owned()),
        players: None,
    }
}

impl Default for Assignments {
    fn default() -> Self {
        Self {
            default: default_assignment(),
            machines: BTreeMap::new(),
            software: BTreeMap::new(),
            machine_software: BTreeMap::new(),
        }
    }
}

/// Key of a software assignment.
pub fn software_key(list: &str, software: &str) -> String {
    format!("{list}:{software}")
}

/// Key of a machine + software assignment.
pub fn machine_software_key(machine: &str, list: &str, software: &str) -> String {
    format!("{machine}/{list}:{software}")
}

fn is_software_key(key: &str) -> bool {
    key.split_once(':')
        .is_some_and(|(list, software)| is_short_name(list) && is_short_name(software))
}

fn is_machine_software_key(key: &str) -> bool {
    key.split_once('/')
        .is_some_and(|(machine, software)| is_short_name(machine) && is_software_key(software))
}

/// Where an assignment is stored: its level and, except for the default, its key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AssignmentTarget {
    pub level: AssignmentLevel,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
}

impl AssignmentTarget {
    /// Checks the level and the key (MAME short names only).
    pub fn validate(&self) -> Result<(), String> {
        let key = self.key.as_deref();
        let valid = match self.level {
            AssignmentLevel::Default => key.is_none(),
            AssignmentLevel::Machine => key.is_some_and(is_short_name),
            AssignmentLevel::Software => key.is_some_and(is_software_key),
            AssignmentLevel::MachineSoftware => key.is_some_and(is_machine_software_key),
        };
        if valid {
            Ok(())
        } else {
            Err(match self.level {
                AssignmentLevel::Default => "the default assignment has no key".to_owned(),
                AssignmentLevel::Machine => format!("invalid machine key '{}'", key.unwrap_or("")),
                AssignmentLevel::Software => format!(
                    "invalid software key '{}': expected <list>:<software>",
                    key.unwrap_or("")
                ),
                AssignmentLevel::MachineSoftware => format!(
                    "invalid machine and software key '{}': expected <machine>/<list>:<software>",
                    key.unwrap_or("")
                ),
            })
        }
    }
}

/// Why a change was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ControllersError {
    /// A controller or profile does not exist.
    NotFound(String),
    /// The controller or profile is referenced by these assignments.
    InUse {
        message: String,
        references: Vec<AssignmentTarget>,
    },
    Invalid(String),
}

impl fmt::Display for ControllersError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound(message) | Self::Invalid(message) | Self::InUse { message, .. } => {
                f.write_str(message)
            }
        }
    }
}

impl std::error::Error for ControllersError {}

/// The attributes of a controller set by the user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ControllerInput {
    pub name: String,
    pub kind: ControllerKind,
    pub device_class: DeviceClass,
    #[serde(default)]
    pub device_id: Option<String>,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub binding: Option<ControllerBinding>,
}

/// Everything stored in `controllers.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ControllersConfig {
    #[serde(default = "schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub controllers: Vec<Controller>,
    #[serde(default)]
    pub profiles: Vec<ControllerProfile>,
    #[serde(default)]
    pub assignments: Assignments,
}

fn schema_version() -> u32 {
    SCHEMA_VERSION
}

impl Default for ControllersConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl ControllersConfig {
    /// The initial state: the system profile, used by default.
    pub fn new() -> Self {
        let mut config = Self {
            schema_version: SCHEMA_VERSION,
            controllers: Vec::new(),
            profiles: Vec::new(),
            assignments: Assignments::default(),
        };
        config.ensure_system_profiles();
        config
    }

    /// Adds the system profiles that are missing, first, so they are always available.
    pub fn ensure_system_profiles(&mut self) {
        for (index, (id, _)) in SYSTEM_PROFILES.iter().enumerate() {
            if self.profile(id).is_none() {
                self.profiles.insert(index, ControllerProfile::system(id));
            }
        }
    }

    /// Checks the whole configuration, including references between entities.
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(format!(
                "unsupported controllers file version {}",
                self.schema_version
            ));
        }
        if self.controllers.len() > MAX_CONTROLLERS {
            return Err(format!("at most {MAX_CONTROLLERS} controllers are allowed"));
        }
        let mut ids = HashSet::new();
        for controller in &self.controllers {
            validate_controller(controller)?;
            if !ids.insert(controller.id.as_str()) {
                return Err(format!(
                    "the controller '{}' is defined twice",
                    controller.id
                ));
            }
        }
        self.validate_devices()?;
        if self.profiles.len() > MAX_PROFILES {
            return Err(format!("at most {MAX_PROFILES} profiles are allowed"));
        }
        let mut ids = HashSet::new();
        for profile in &self.profiles {
            profile.validate()?;
            if !ids.insert(profile.id.as_str()) {
                return Err(format!("the profile '{}' is defined twice", profile.id));
            }
        }
        let count = self.assignments.machines.len()
            + self.assignments.software.len()
            + self.assignments.machine_software.len();
        if count > MAX_ASSIGNMENTS {
            return Err(format!("at most {MAX_ASSIGNMENTS} assignments are allowed"));
        }
        for (target, assignment) in self.assignment_entries() {
            target.validate()?;
            self.validate_assignment(assignment)
                .map_err(|error| format!("{}: {error}", describe(&target)))?;
        }
        Ok(())
    }

    /// Checks that an assignment only references existing entities.
    pub fn validate_assignment(&self, assignment: &Assignment) -> Result<(), String> {
        if let Some(id) = &assignment.profile_id
            && self.profile(id).is_none()
        {
            return Err(format!("the controller profile '{id}' does not exist"));
        }
        if let Some(players) = &assignment.players {
            if players.len() > MAX_PLAYERS {
                return Err(format!("at most {MAX_PLAYERS} players can be assigned"));
            }
            let mut seen = HashSet::new();
            for id in players.iter().flatten() {
                if self.controller(id).is_none() {
                    return Err(format!("the controller '{id}' does not exist"));
                }
                if !seen.insert(id) {
                    return Err(format!("the controller '{id}' is assigned to two players"));
                }
            }
        }
        Ok(())
    }

    /// Two mapped controllers cannot name the same device: MAME matches
    /// device ids as case-insensitive substrings, so the id of one must not
    /// contain the id of another of the same provider either.
    fn validate_devices(&self) -> Result<(), String> {
        let bound: Vec<(&Controller, DeviceBinding<'_>)> = self
            .controllers
            .iter()
            .filter_map(|controller| {
                controller
                    .device_binding()
                    .ok()
                    .map(|binding| (controller, binding))
            })
            .collect();
        for (index, (first, a)) in bound.iter().enumerate() {
            for (second, b) in &bound[index + 1..] {
                if a.provider == b.provider && device_ids_overlap(a.device_id, b.device_id) {
                    return Err(format!(
                        "the controllers '{}' and '{}' use the same {} device id: MAME could not tell them apart",
                        first.id, second.id, a.provider
                    ));
                }
            }
        }
        Ok(())
    }

    /// The controllers registered with the device id `device_id` of
    /// `provider` (compared without case, like MAME).
    pub fn controllers_with_device(&self, provider: &str, device_id: &str) -> Vec<&Controller> {
        self.controllers
            .iter()
            .filter(|controller| {
                controller.provider.as_deref() == Some(provider)
                    && controller
                        .device_id
                        .as_deref()
                        .is_some_and(|id| id.eq_ignore_ascii_case(device_id))
            })
            .collect()
    }

    /// Every stored assignment with its target, the default first.
    pub fn assignment_entries(&self) -> impl Iterator<Item = (AssignmentTarget, &Assignment)> {
        fn keyed(
            level: AssignmentLevel,
            map: &BTreeMap<String, Assignment>,
        ) -> impl Iterator<Item = (AssignmentTarget, &Assignment)> {
            map.iter().map(move |(key, assignment)| {
                (
                    AssignmentTarget {
                        level,
                        key: Some(key.clone()),
                    },
                    assignment,
                )
            })
        }
        std::iter::once((
            AssignmentTarget {
                level: AssignmentLevel::Default,
                key: None,
            },
            &self.assignments.default,
        ))
        .chain(keyed(AssignmentLevel::Machine, &self.assignments.machines))
        .chain(keyed(AssignmentLevel::Software, &self.assignments.software))
        .chain(keyed(
            AssignmentLevel::MachineSoftware,
            &self.assignments.machine_software,
        ))
    }

    pub fn controller(&self, id: &str) -> Option<&Controller> {
        self.controllers
            .iter()
            .find(|controller| controller.id == id)
    }

    pub fn profile(&self, id: &str) -> Option<&ControllerProfile> {
        self.profiles.iter().find(|profile| profile.id == id)
    }

    fn user_profile_mut(&mut self, id: &str) -> Result<&mut ControllerProfile, ControllersError> {
        let profile = self
            .profiles
            .iter_mut()
            .find(|profile| profile.id == id)
            .ok_or_else(|| profile_not_found(id))?;
        if profile.kind == ProfileKind::System {
            return Err(ControllersError::Invalid(format!(
                "the system profile '{id}' cannot be changed"
            )));
        }
        Ok(profile)
    }

    /// Adds a controller with an id derived from its name; returns the id.
    pub fn create_controller(
        &mut self,
        input: ControllerInput,
    ) -> Result<String, ControllersError> {
        let id = unique_id(&input.name, |id| self.controller(id).is_some());
        let controller = controller_from(id.clone(), input);
        validate_new_device(&controller).map_err(ControllersError::Invalid)?;
        if self.controllers.len() >= MAX_CONTROLLERS {
            return Err(ControllersError::Invalid(format!(
                "at most {MAX_CONTROLLERS} controllers are allowed"
            )));
        }
        self.controllers.push(controller);
        if let Err(error) = self.validate_devices() {
            self.controllers.pop();
            return Err(ControllersError::Invalid(error));
        }
        Ok(id)
    }

    /// Replaces the attributes of a controller; its id never changes.
    pub fn update_controller(
        &mut self,
        id: &str,
        input: ControllerInput,
    ) -> Result<(), ControllersError> {
        let controller = controller_from(id.to_owned(), input);
        validate_new_device(&controller).map_err(ControllersError::Invalid)?;
        let existing = self
            .controllers
            .iter_mut()
            .find(|controller| controller.id == id)
            .ok_or_else(|| controller_not_found(id))?;
        let previous = std::mem::replace(existing, controller);
        if let Err(error) = self.validate_devices() {
            if let Some(existing) = self
                .controllers
                .iter_mut()
                .find(|controller| controller.id == id)
            {
                *existing = previous;
            }
            return Err(ControllersError::Invalid(error));
        }
        Ok(())
    }

    /// Removes a controller that no assignment references.
    pub fn delete_controller(&mut self, id: &str) -> Result<(), ControllersError> {
        let index = self
            .controllers
            .iter()
            .position(|controller| controller.id == id)
            .ok_or_else(|| controller_not_found(id))?;
        let references = self.references(|assignment| {
            assignment
                .players
                .as_ref()
                .is_some_and(|players| players.iter().flatten().any(|player| player == id))
        });
        if !references.is_empty() {
            return Err(ControllersError::InUse {
                message: format!(
                    "the controller '{id}' is assigned in {} assignment(s); remove it from them first",
                    references.len()
                ),
                references,
            });
        }
        self.controllers.remove(index);
        Ok(())
    }

    /// Adds a user profile with an id derived from its name; returns the id.
    /// `used_on_disk` tells whether files of a former profile still use an
    /// id: a new profile never reuses them.
    pub fn create_profile(
        &mut self,
        name: &str,
        has_base: bool,
        used_on_disk: impl Fn(&str) -> bool,
    ) -> Result<String, ControllersError> {
        let id = unique_id(name, |id| {
            self.profile(id).is_some() || system_name(id).is_some() || used_on_disk(id)
        });
        let profile = ControllerProfile {
            id: id.clone(),
            kind: ProfileKind::User,
            name: Some(name.trim().to_owned()),
            has_base,
            base_file_name: None,
        };
        profile.validate().map_err(ControllersError::Invalid)?;
        if self.profiles.len() >= MAX_PROFILES {
            return Err(ControllersError::Invalid(format!(
                "at most {MAX_PROFILES} profiles are allowed"
            )));
        }
        self.profiles.push(profile);
        Ok(id)
    }

    pub fn rename_profile(&mut self, id: &str, name: &str) -> Result<(), ControllersError> {
        let profile = self.user_profile_mut(id)?;
        let mut renamed = profile.clone();
        renamed.name = Some(name.trim().to_owned());
        renamed.validate().map_err(ControllersError::Invalid)?;
        *profile = renamed;
        Ok(())
    }

    /// Records the `base.cfg` of a user profile: `Some` with the name of the
    /// file it was imported from (if known), or `None` without a base.
    pub fn set_profile_base_file(
        &mut self,
        id: &str,
        base: Option<Option<String>>,
    ) -> Result<(), ControllersError> {
        let profile = self.user_profile_mut(id)?;
        let mut changed = profile.clone();
        changed.has_base = base.is_some();
        changed.base_file_name = base.flatten();
        changed.validate().map_err(ControllersError::Invalid)?;
        *profile = changed;
        Ok(())
    }

    /// Removes a user profile that no assignment references. Its files are kept.
    pub fn delete_profile(&mut self, id: &str) -> Result<(), ControllersError> {
        self.user_profile_mut(id)?;
        let references = self.references(|assignment| assignment.profile_id.as_deref() == Some(id));
        if !references.is_empty() {
            return Err(ControllersError::InUse {
                message: format!(
                    "the profile '{id}' is used by {} assignment(s); change them first",
                    references.len()
                ),
                references,
            });
        }
        self.profiles.retain(|profile| profile.id != id);
        Ok(())
    }

    fn references(&self, uses: impl Fn(&Assignment) -> bool) -> Vec<AssignmentTarget> {
        self.assignment_entries()
            .filter(|(_, assignment)| uses(assignment))
            .map(|(target, _)| target)
            .collect()
    }

    pub fn assignment(&self, target: &AssignmentTarget) -> Option<&Assignment> {
        let key = target.key.as_deref().unwrap_or_default();
        match target.level {
            AssignmentLevel::Default => Some(&self.assignments.default),
            AssignmentLevel::Machine => self.assignments.machines.get(key),
            AssignmentLevel::Software => self.assignments.software.get(key),
            AssignmentLevel::MachineSoftware => self.assignments.machine_software.get(key),
        }
    }

    /// Removes the assignment of a machine or software level; the default
    /// assignment returns to the initial state (the `mame` profile).
    pub fn reset_assignment(&mut self, target: &AssignmentTarget) -> Result<(), ControllersError> {
        let assignment = match target.level {
            AssignmentLevel::Default => default_assignment(),
            _ => Assignment::default(),
        };
        self.set_assignment(target, assignment)
    }

    /// Changes some fields of the assignment of a target and keeps the
    /// others; a keyed assignment left empty is removed.
    pub fn update_assignment(
        &mut self,
        target: &AssignmentTarget,
        update: AssignmentUpdate,
    ) -> Result<(), ControllersError> {
        target.validate().map_err(ControllersError::Invalid)?;
        let mut assignment = self.assignment(target).cloned().unwrap_or_default();
        update.profile_id.apply(&mut assignment.profile_id);
        update.players.apply(&mut assignment.players);
        self.set_assignment(target, assignment)
    }

    /// Replaces the assignment of a target; an empty assignment removes a
    /// keyed one (its fields are then inherited again). The default
    /// assignment always exists, possibly empty.
    pub fn set_assignment(
        &mut self,
        target: &AssignmentTarget,
        assignment: Assignment,
    ) -> Result<(), ControllersError> {
        target.validate().map_err(ControllersError::Invalid)?;
        let assignment = assignment.normalized();
        self.validate_assignment(&assignment)
            .map_err(ControllersError::Invalid)?;
        let map = match target.level {
            AssignmentLevel::Default => {
                self.assignments.default = assignment;
                return Ok(());
            }
            AssignmentLevel::Machine => &mut self.assignments.machines,
            AssignmentLevel::Software => &mut self.assignments.software,
            AssignmentLevel::MachineSoftware => &mut self.assignments.machine_software,
        };
        let key = target.key.clone().unwrap_or_default();
        if assignment.is_empty() {
            map.remove(&key);
        } else {
            map.insert(key, assignment);
        }
        Ok(())
    }
}

fn controller_from(id: String, input: ControllerInput) -> Controller {
    let optional = |value: Option<String>| {
        value
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
    };
    Controller {
        id,
        name: input.name.trim().to_owned(),
        kind: input.kind,
        device_class: input.device_class,
        device_id: optional(input.device_id),
        provider: optional(input.provider),
        binding: input.binding,
    }
}

fn validate_controller(controller: &Controller) -> Result<(), String> {
    validate_id("controller", &controller.id)?;
    validate_name(
        &format!("the controller '{}'", controller.id),
        Some(&controller.name),
    )?;
    if let Some(device_id) = &controller.device_id
        && (device_id.trim().is_empty()
            || device_id.chars().count() > MAX_DEVICE_ID_LENGTH
            || device_id.chars().any(char::is_control))
    {
        return Err(format!(
            "the device id of '{}' must have 1 to {MAX_DEVICE_ID_LENGTH} characters without control characters",
            controller.id
        ));
    }
    if let Some(provider) = &controller.provider {
        let valid = !provider.is_empty()
            && provider.len() <= MAX_PROVIDER_LENGTH
            && provider
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
        if !valid {
            return Err(format!(
                "invalid input provider '{provider}' for '{}'",
                controller.id
            ));
        }
    }
    if let Some(binding) = controller.binding {
        let device = controller.device_binding().map_err(|error| {
            format!(
                "the binding of the controller '{}' cannot be used: {error}",
                controller.id
            )
        })?;
        let provider = JoystickProvider::from_name(device.provider).ok_or_else(|| {
            format!(
                "'{}' is not a MAME joystick provider that YMDB maps devices with (controller '{}')",
                device.provider, controller.id
            )
        })?;
        let identity = devices::identity(provider, device.device_id);
        if !identity.allows(binding) {
            return Err(format!(
                "the device id of the controller '{}' is {}: only the port order binding describes it",
                controller.id,
                identity.describe()
            ));
        }
    }
    Ok(())
}

/// Checks a controller being created or changed: a mapped device must also
/// use a joystick provider of this operating system.
fn validate_new_device(controller: &Controller) -> Result<(), String> {
    validate_controller(controller)?;
    if controller.binding.is_some()
        && let Some(provider) = controller.provider.as_deref()
        && !JoystickProvider::from_name(provider).is_some_and(JoystickProvider::is_available)
    {
        return Err(format!(
            "the joystick provider '{provider}' is not available on this operating system"
        ));
    }
    Ok(())
}

/// Whether MAME could match one device id with the other: it compares them
/// as case-insensitive substrings (`input_device::match_device_id`).
pub fn device_ids_overlap(a: &str, b: &str) -> bool {
    let (a, b) = (a.to_lowercase(), b.to_lowercase());
    a.contains(&b) || b.contains(&a)
}

/// Ids are path segments: 1-40 lowercase ASCII letters, digits or '-'.
pub fn is_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_ID_LENGTH
        && !id.starts_with('-')
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn validate_id(what: &str, id: &str) -> Result<(), String> {
    if is_id(id) {
        Ok(())
    } else {
        Err(format!(
            "invalid {what} id '{id}': use 1-{MAX_ID_LENGTH} lowercase letters, digits or '-'"
        ))
    }
}

fn validate_name(what: &str, name: Option<&str>) -> Result<(), String> {
    let name = name.map(str::trim).unwrap_or_default();
    if name.is_empty()
        || name.chars().count() > MAX_NAME_LENGTH
        || name.chars().any(char::is_control)
    {
        return Err(format!(
            "{what} needs a name of 1 to {MAX_NAME_LENGTH} characters"
        ));
    }
    Ok(())
}

/// A new id from a name (`Arcade Stick P1` → `arcade-stick-p1`), with a
/// numeric suffix when `taken`.
fn unique_id(name: &str, taken: impl Fn(&str) -> bool) -> String {
    let mut base = String::new();
    for c in name.trim().chars() {
        if c.is_ascii_alphanumeric() {
            base.push(c.to_ascii_lowercase());
        } else if !base.is_empty() && !base.ends_with('-') {
            base.push('-');
        }
    }
    base.truncate(MAX_ID_LENGTH - 4);
    let base = base.trim_end_matches('-');
    let base = if base.is_empty() { "item" } else { base };
    if !taken(base) {
        return base.to_owned();
    }
    (2..)
        .map(|suffix| format!("{base}-{suffix}"))
        .find(|id| !taken(id))
        .unwrap_or_default()
}

fn describe(target: &AssignmentTarget) -> String {
    match &target.key {
        None => "default assignment".to_owned(),
        Some(key) => format!("assignment '{key}'"),
    }
}

fn controller_not_found(id: &str) -> ControllersError {
    ControllersError::NotFound(format!("the controller '{id}' does not exist"))
}

fn profile_not_found(id: &str) -> ControllersError {
    ControllersError::NotFound(format!("the controller profile '{id}' does not exist"))
}
