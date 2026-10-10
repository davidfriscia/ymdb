//! The structured MAME command line.
//!
//! A [`LaunchPlan`] is built once and used both for the preview shown to the
//! user and for the process that is started, so the two cannot differ.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;

use super::config::{OptionSource, ResolvedOption};
use super::options;
use super::resolve::MediaAssignment;

/// What an argument of the command line does.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "role",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ArgumentRole {
    /// The machine short name.
    Machine,
    /// A software part mounted into a media device.
    Media { device: String, part: String },
    /// A MAME option and the configuration level it comes from.
    Option { name: String, source: OptionSource },
}

/// One logical argument and the command line tokens it produces.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanArgument {
    #[serde(flatten)]
    pub role: ArgumentRole,
    pub tokens: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchPlan {
    /// The MAME installation the plan was prepared for.
    pub installation_id: String,
    /// The launch profile applied.
    pub profile_id: String,
    pub executable: PathBuf,
    /// The executable's directory, so MAME finds its `mame.ini` and relative paths.
    pub working_directory: PathBuf,
    pub arguments: Vec<PlanArgument>,
}

/// The software part of a launch.
pub struct SoftwareMedia<'a> {
    pub list: &'a str,
    pub software: &'a str,
    pub media: &'a [MediaAssignment],
}

/// The installation and profile a plan is prepared with.
pub struct PlanOrigin<'a> {
    pub installation_id: &'a str,
    pub profile_id: &'a str,
}

impl LaunchPlan {
    /// Builds `mame <machine> [-<device> <list>:<software>:<part>]... [options]`.
    /// Inputs must already be validated.
    pub fn build(
        origin: &PlanOrigin<'_>,
        executable: &Path,
        machine: &str,
        software: Option<&SoftwareMedia<'_>>,
        options: &[ResolvedOption],
    ) -> Self {
        let mut arguments = vec![PlanArgument {
            role: ArgumentRole::Machine,
            tokens: vec![machine.to_owned()],
        }];
        if let Some(software) = software {
            arguments.extend(software.media.iter().map(|assignment| PlanArgument {
                role: ArgumentRole::Media {
                    device: assignment.device.clone(),
                    part: assignment.part.clone(),
                },
                tokens: vec![
                    format!("-{}", assignment.device),
                    format!(
                        "{}:{}:{}",
                        software.list, software.software, assignment.part
                    ),
                ],
            }));
        }
        arguments.extend(options.iter().map(|option| PlanArgument {
            role: ArgumentRole::Option {
                name: option.name.clone(),
                source: option.source,
            },
            tokens: options::to_arguments(&option.name, &option.value),
        }));
        Self {
            installation_id: origin.installation_id.to_owned(),
            profile_id: origin.profile_id.to_owned(),
            executable: executable.to_path_buf(),
            working_directory: executable
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_default(),
            arguments,
        }
    }

    /// The arguments passed to MAME, in order.
    pub fn command_line(&self) -> Vec<String> {
        self.arguments
            .iter()
            .flat_map(|argument| argument.tokens.iter().cloned())
            .collect()
    }

    /// The process to start: direct execution, never through a shell.
    pub fn command(&self) -> Command {
        let mut command = Command::new(&self.executable);
        command
            .args(self.command_line().into_iter().map(OsString::from))
            .current_dir(&self.working_directory);
        command
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::launcher::options::OptionValue;

    const ORIGIN: PlanOrigin<'static> = PlanOrigin {
        installation_id: "0123456789abcdef0123456789abcdef",
        profile_id: "default",
    };

    fn option(name: &str, value: OptionValue, source: OptionSource) -> ResolvedOption {
        ResolvedOption {
            name: name.to_owned(),
            value,
            source,
        }
    }

    #[test]
    fn builds_a_machine_command_line() {
        let executable = Path::new("/opt/Émulateurs MAME ✓/mame");
        let plan = LaunchPlan::build(
            &ORIGIN,
            executable,
            "pacman",
            None,
            &[
                option("window", OptionValue::Boolean(false), OptionSource::Global),
                option(
                    "rompath",
                    OptionValue::List(vec!["roms".to_owned(), "/srv/MAME ROMs".to_owned()]),
                    OptionSource::Profile,
                ),
            ],
        );
        assert_eq!(
            plan.command_line(),
            ["pacman", "-nowindow", "-rompath", "roms;/srv/MAME ROMs"]
        );
        assert_eq!(plan.working_directory, Path::new("/opt/Émulateurs MAME ✓"));

        let command = plan.command();
        assert_eq!(command.get_program(), executable.as_os_str());
        assert_eq!(
            command.get_current_dir(),
            Some(Path::new("/opt/Émulateurs MAME ✓"))
        );
        let args: Vec<_> = command.get_args().collect();
        assert_eq!(
            args,
            ["pacman", "-nowindow", "-rompath", "roms;/srv/MAME ROMs"]
        );
    }

    #[test]
    fn mounts_every_assigned_part_before_options() {
        let media = [
            MediaAssignment {
                device: "flop1".to_owned(),
                part: "flop1".to_owned(),
            },
            MediaAssignment {
                device: "flop2".to_owned(),
                part: "flop2".to_owned(),
            },
        ];
        let plan = LaunchPlan::build(
            &ORIGIN,
            Path::new("C:\\MAME\\mame.exe"),
            "apple2e",
            Some(&SoftwareMedia {
                list: "apple2_flop_orig",
                software: "ultima4",
                media: &media,
            }),
            &[option(
                "speed",
                OptionValue::Number(2.0),
                OptionSource::Software,
            )],
        );
        assert_eq!(
            plan.command_line(),
            [
                "apple2e",
                "-flop1",
                "apple2_flop_orig:ultima4:flop1",
                "-flop2",
                "apple2_flop_orig:ultima4:flop2",
                "-speed",
                "2"
            ]
        );
    }

    #[test]
    fn preview_serializes_roles_sources_and_tokens() {
        let media = [MediaAssignment {
            device: "cart".to_owned(),
            part: "cart".to_owned(),
        }];
        let plan = LaunchPlan::build(
            &ORIGIN,
            Path::new("/opt/mame/mame"),
            "nes",
            Some(&SoftwareMedia {
                list: "nes",
                software: "smb",
                media: &media,
            }),
            &[option(
                "window",
                OptionValue::Boolean(true),
                OptionSource::Machine,
            )],
        );
        let json = serde_json::to_value(&plan).expect("serialize");
        assert_eq!(
            json["arguments"],
            serde_json::json!([
                { "role": "machine", "tokens": ["nes"] },
                { "role": "media", "device": "cart", "part": "cart", "tokens": ["-cart", "nes:smb:cart"] },
                { "role": "option", "name": "window", "source": "machine", "tokens": ["-window"] }
            ])
        );
        assert_eq!(json["workingDirectory"], "/opt/mame");
        assert_eq!(json["installationId"], ORIGIN.installation_id);
        assert_eq!(json["profileId"], "default");
    }
}
