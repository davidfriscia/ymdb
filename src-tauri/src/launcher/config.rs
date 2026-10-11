//! Launch configuration: global options, profiles and overrides, and the
//! resolution of the options passed to MAME.
//!
//! Precedence, from lowest to highest: MAME's own configuration (never
//! written), global options, the selected profile, the machine override, the
//! software override. Only options set at some level are passed to MAME.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use super::options::{self, DEFINITIONS, OptionError, OptionSet, OptionValue};
use crate::catalog::is_short_name;

pub const SCHEMA_VERSION: u32 = 1;
pub const DEFAULT_PROFILE_ID: &str = "default";

const MAX_PROFILES: usize = 50;
const MAX_PROFILE_NAME_LENGTH: usize = 60;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LaunchConfig {
    #[serde(default = "schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub global_options: OptionSet,
    #[serde(default = "default_profiles")]
    pub profiles: Vec<Profile>,
    /// Profile used when a launch does not select one.
    #[serde(default = "default_profile_id")]
    pub default_profile_id: String,
    /// Options for one machine, by MAME short name.
    #[serde(default)]
    pub machine_overrides: BTreeMap<String, OptionSet>,
    /// Options for one software, by `<list>:<software>`.
    #[serde(default)]
    pub software_overrides: BTreeMap<String, OptionSet>,
}

/// A named set of options chosen at launch time. Profiles do not inherit
/// from each other.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Profile {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub options: OptionSet,
}

fn schema_version() -> u32 {
    SCHEMA_VERSION
}

fn default_profile_id() -> String {
    DEFAULT_PROFILE_ID.to_owned()
}

fn default_profiles() -> Vec<Profile> {
    vec![Profile {
        id: DEFAULT_PROFILE_ID.to_owned(),
        name: "Default".to_owned(),
        options: OptionSet::new(),
    }]
}

impl Default for LaunchConfig {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            global_options: OptionSet::new(),
            profiles: default_profiles(),
            default_profile_id: default_profile_id(),
            machine_overrides: BTreeMap::new(),
            software_overrides: BTreeMap::new(),
        }
    }
}

/// Key of a software override.
pub fn software_key(list: &str, software: &str) -> String {
    format!("{list}:{software}")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError(pub String);

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ConfigError {}

impl From<OptionError> for ConfigError {
    fn from(error: OptionError) -> Self {
        Self(error.to_string())
    }
}

impl LaunchConfig {
    /// Checks the whole configuration; nothing invalid is ever saved or used.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(ConfigError(format!(
                "unsupported launch configuration version {}",
                self.schema_version
            )));
        }
        options::validate_set(&self.global_options)?;
        if self.profiles.len() > MAX_PROFILES {
            return Err(ConfigError(format!(
                "at most {MAX_PROFILES} profiles are allowed"
            )));
        }
        for (index, profile) in self.profiles.iter().enumerate() {
            if !is_profile_id(&profile.id) {
                return Err(ConfigError(format!(
                    "invalid profile id '{}': use 1-40 lowercase letters, digits or '-'",
                    profile.id
                )));
            }
            if self.profiles[..index]
                .iter()
                .any(|other| other.id == profile.id)
            {
                return Err(ConfigError(format!(
                    "duplicate profile id '{}'",
                    profile.id
                )));
            }
            let name = profile.name.trim();
            if name.is_empty()
                || name.len() > MAX_PROFILE_NAME_LENGTH
                || name.chars().any(char::is_control)
            {
                return Err(ConfigError(format!(
                    "profile '{}' needs a name of 1-{MAX_PROFILE_NAME_LENGTH} characters",
                    profile.id
                )));
            }
            options::validate_set(&profile.options)?;
        }
        if self.profile(DEFAULT_PROFILE_ID).is_none() {
            return Err(ConfigError(
                "the default profile cannot be removed".to_owned(),
            ));
        }
        if self.profile(&self.default_profile_id).is_none() {
            return Err(ConfigError(format!(
                "the default profile '{}' does not exist",
                self.default_profile_id
            )));
        }
        for (machine, options) in &self.machine_overrides {
            if !is_short_name(machine) {
                return Err(ConfigError(format!("invalid machine name '{machine}'")));
            }
            options::validate_set(options)?;
        }
        for (key, options) in &self.software_overrides {
            let valid = key
                .split_once(':')
                .is_some_and(|(list, software)| is_short_name(list) && is_short_name(software));
            if !valid {
                return Err(ConfigError(format!(
                    "invalid software key '{key}': expected <list>:<software>"
                )));
            }
            options::validate_set(options)?;
        }
        Ok(())
    }

    /// Removes overrides without options, so the file only keeps what is set.
    pub fn normalized(mut self) -> Self {
        self.machine_overrides
            .retain(|_, options| !options.is_empty());
        self.software_overrides
            .retain(|_, options| !options.is_empty());
        for profile in &mut self.profiles {
            profile.name = profile.name.trim().to_owned();
        }
        self
    }

    pub fn profile(&self, id: &str) -> Option<&Profile> {
        self.profiles.iter().find(|profile| profile.id == id)
    }

    /// Resolves the options of a launch by precedence. `profile_id` defaults
    /// to the default profile.
    pub fn resolve(
        &self,
        profile_id: Option<&str>,
        machine: &str,
        software: Option<(&str, &str)>,
    ) -> Result<Vec<ResolvedOption>, ConfigError> {
        let profile_id = profile_id.unwrap_or(&self.default_profile_id);
        let profile = self
            .profile(profile_id)
            .ok_or_else(|| ConfigError(format!("unknown launch profile '{profile_id}'")))?;
        let empty = OptionSet::new();
        let software_options = software
            .and_then(|(list, name)| self.software_overrides.get(&software_key(list, name)));
        let levels = [
            (OptionSource::Global, &self.global_options),
            (OptionSource::Profile, &profile.options),
            (
                OptionSource::Machine,
                self.machine_overrides.get(machine).unwrap_or(&empty),
            ),
            (OptionSource::Software, software_options.unwrap_or(&empty)),
        ];
        // Definition order keeps command lines stable and readable.
        let resolved = DEFINITIONS
            .iter()
            .filter_map(|definition| {
                levels
                    .iter()
                    .rev()
                    .find_map(|(source, options)| {
                        options.get(definition.name).map(|value| (source, value))
                    })
                    .map(|(source, value)| ResolvedOption {
                        name: definition.name.to_owned(),
                        value: value.clone(),
                        source: *source,
                    })
            })
            .collect();
        Ok(resolved)
    }
}

/// The configuration level an option comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum OptionSource {
    Global,
    Profile,
    Machine,
    Software,
    /// A search path extended with the local destinations of the ROM
    /// manager (see `search_paths`).
    RomManager,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedOption {
    pub name: String,
    pub value: OptionValue,
    pub source: OptionSource,
}

fn is_profile_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 40
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(entries: &[(&str, OptionValue)]) -> OptionSet {
        entries
            .iter()
            .map(|(name, value)| ((*name).to_owned(), value.clone()))
            .collect()
    }

    fn config() -> LaunchConfig {
        let mut config = LaunchConfig {
            global_options: set(&[
                ("window", OptionValue::Boolean(true)),
                ("volume", OptionValue::Number(-6.0)),
                ("skip_gameinfo", OptionValue::Boolean(true)),
            ]),
            ..LaunchConfig::default()
        };
        config.profiles.push(Profile {
            id: "quiet".to_owned(),
            name: "Quiet".to_owned(),
            options: set(&[
                ("volume", OptionValue::Number(-30.0)),
                ("speed", OptionValue::Number(2.0)),
            ]),
        });
        config.profiles[0].options = set(&[("throttle", OptionValue::Boolean(true))]);
        config.machine_overrides.insert(
            "nes".to_owned(),
            set(&[
                ("speed", OptionValue::Number(1.0)),
                ("window", OptionValue::Boolean(false)),
            ]),
        );
        config.software_overrides.insert(
            software_key("nes", "smb"),
            set(&[("window", OptionValue::Boolean(true))]),
        );
        config
    }

    fn summary(options: &[ResolvedOption]) -> Vec<(String, OptionSource)> {
        options
            .iter()
            .map(|option| (format!("{}={:?}", option.name, option.value), option.source))
            .collect()
    }

    #[test]
    fn default_configuration_is_valid_and_empty() {
        let config = LaunchConfig::default();
        config.validate().expect("valid");
        assert_eq!(config.profiles.len(), 1);
        assert!(
            config
                .resolve(None, "pacman", None)
                .expect("resolve")
                .is_empty()
        );
    }

    #[test]
    fn resolves_by_precedence() {
        let config = config();
        config.validate().expect("valid");

        // Default profile, machine without override.
        let options = config.resolve(None, "pacman", None).expect("resolve");
        assert_eq!(
            summary(&options),
            [
                ("window=Boolean(true)".to_owned(), OptionSource::Global),
                ("throttle=Boolean(true)".to_owned(), OptionSource::Profile),
                ("volume=Number(-6.0)".to_owned(), OptionSource::Global),
                (
                    "skip_gameinfo=Boolean(true)".to_owned(),
                    OptionSource::Global
                ),
            ]
        );

        // A selected profile replaces the default profile and beats global options.
        let options = config
            .resolve(Some("quiet"), "pacman", None)
            .expect("resolve");
        assert!(options.iter().all(|option| option.name != "throttle"));
        let volume = options
            .iter()
            .find(|option| option.name == "volume")
            .expect("volume");
        assert_eq!(
            (volume.value.clone(), volume.source),
            (OptionValue::Number(-30.0), OptionSource::Profile)
        );

        // The machine override beats the profile; the software override beats the machine.
        let options = config
            .resolve(Some("quiet"), "nes", Some(("nes", "smb")))
            .expect("resolve");
        let find = |name: &str| {
            options
                .iter()
                .find(|option| option.name == name)
                .expect(name)
        };
        assert_eq!(find("speed").source, OptionSource::Machine);
        assert_eq!(find("speed").value, OptionValue::Number(1.0));
        assert_eq!(find("window").source, OptionSource::Software);
        assert_eq!(find("window").value, OptionValue::Boolean(true));

        // Without the software, the machine override applies.
        let options = config.resolve(None, "nes", None).expect("resolve");
        let window = options
            .iter()
            .find(|option| option.name == "window")
            .expect("window");
        assert_eq!(
            (window.value.clone(), window.source),
            (OptionValue::Boolean(false), OptionSource::Machine)
        );
    }

    #[test]
    fn uses_the_configured_default_profile() {
        let mut config = config();
        config.default_profile_id = "quiet".to_owned();
        let options = config.resolve(None, "pacman", None).expect("resolve");
        assert!(options.iter().any(|option| option.name == "speed"));
        assert!(config.resolve(Some("missing"), "pacman", None).is_err());
    }

    #[test]
    fn rejects_invalid_configurations() {
        type Change = Box<dyn Fn(&mut LaunchConfig)>;
        let cases: Vec<(&str, Change)> = vec![
            (
                "unknown option",
                Box::new(|c| {
                    c.global_options
                        .insert("nosuch".to_owned(), OptionValue::Boolean(true));
                }),
            ),
            (
                "bad profile id",
                Box::new(|c| c.profiles[1].id = "Bad Id".to_owned()),
            ),
            (
                "duplicate id",
                Box::new(|c| c.profiles[1].id = DEFAULT_PROFILE_ID.to_owned()),
            ),
            (
                "empty name",
                Box::new(|c| c.profiles[1].name = "  ".to_owned()),
            ),
            (
                "default removed",
                Box::new(|c| {
                    c.profiles.remove(0);
                }),
            ),
            (
                "missing default",
                Box::new(|c| c.default_profile_id = "missing".to_owned()),
            ),
            (
                "bad machine",
                Box::new(|c| {
                    c.machine_overrides
                        .insert("Pac Man".to_owned(), OptionSet::new());
                }),
            ),
            (
                "bad software key",
                Box::new(|c| {
                    c.software_overrides
                        .insert("smb".to_owned(), OptionSet::new());
                }),
            ),
            (
                "bad override value",
                Box::new(|c| {
                    c.machine_overrides.insert(
                        "pacman".to_owned(),
                        set(&[("volume", OptionValue::Number(50.0))]),
                    );
                }),
            ),
            ("future version", Box::new(|c| c.schema_version = 2)),
        ];
        for (label, change) in cases {
            let mut config = config();
            change(&mut config);
            assert!(config.validate().is_err(), "{label}");
        }
    }

    #[test]
    fn normalization_drops_empty_overrides() {
        let mut config = config();
        config
            .machine_overrides
            .insert("pacman".to_owned(), OptionSet::new());
        config.profiles[1].name = "  Quiet  ".to_owned();
        let config = config.normalized();
        assert!(!config.machine_overrides.contains_key("pacman"));
        assert!(config.machine_overrides.contains_key("nes"));
        assert_eq!(config.profiles[1].name, "Quiet");
    }

    #[test]
    fn serializes_to_the_documented_format() {
        let json = serde_json::to_value(LaunchConfig::default()).expect("serialize");
        assert_eq!(
            json,
            serde_json::json!({
                "schemaVersion": 1,
                "globalOptions": {},
                "profiles": [{ "id": "default", "name": "Default", "options": {} }],
                "defaultProfileId": "default",
                "machineOverrides": {},
                "softwareOverrides": {}
            })
        );
        let parsed: LaunchConfig = serde_json::from_str("{}").expect("defaults");
        assert_eq!(parsed, LaunchConfig::default());
        assert!(serde_json::from_str::<LaunchConfig>(r#"{ "unknown": 1 }"#).is_err());
    }
}
