//! The effective controller profile and controllers of a launch.
//!
//! Precedence, from the weakest: the default assignment, the machine, the
//! software (`<list>:<software>`, on any machine), the machine running that
//! software, then the temporary override of the launch request. The profile
//! and the players are resolved separately: a level that sets only one of
//! them inherits the other from the weaker levels. Without any profile the
//! system profile `mame` applies.
//!
//! This precedence belongs to controller management only; the launch
//! options keep their own precedence (`launcher::config`).

use serde::Serialize;

use super::model::{
    Assignment, AssignmentLevel, AssignmentTarget, ControllersConfig, ControllersError,
    MAME_PROFILE, Players, machine_software_key, software_key,
};

/// Where an effective value comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ControlsSource {
    Default,
    Machine,
    Software,
    MachineSoftware,
    /// The temporary override of the launch request (never saved).
    Request,
}

impl From<AssignmentLevel> for ControlsSource {
    fn from(level: AssignmentLevel) -> Self {
        match level {
            AssignmentLevel::Default => Self::Default,
            AssignmentLevel::Machine => Self::Machine,
            AssignmentLevel::Software => Self::Software,
            AssignmentLevel::MachineSoftware => Self::MachineSoftware,
        }
    }
}

/// The effective controls of a launch and where each field comes from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedControls {
    pub profile_id: String,
    pub profile_source: ControlsSource,
    /// Controller ids by player, Player 1 first; `None` for a player
    /// without a controller.
    pub players: Players,
    /// `None` when no level assigns controllers.
    pub players_source: Option<ControlsSource>,
}

/// The stored assignment of one level that applies to a launch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LevelAssignment {
    #[serde(flatten)]
    pub target: AssignmentTarget,
    /// `None` when nothing is stored at this level.
    pub assignment: Option<Assignment>,
}

/// The levels that apply to a machine, optionally running a software, from
/// the weakest, with what each one stores.
pub fn levels(
    config: &ControllersConfig,
    machine: &str,
    software: Option<(&str, &str)>,
) -> Vec<LevelAssignment> {
    let mut targets = vec![
        AssignmentTarget {
            level: AssignmentLevel::Default,
            key: None,
        },
        AssignmentTarget {
            level: AssignmentLevel::Machine,
            key: Some(machine.to_owned()),
        },
    ];
    if let Some((list, name)) = software {
        targets.push(AssignmentTarget {
            level: AssignmentLevel::Software,
            key: Some(software_key(list, name)),
        });
        targets.push(AssignmentTarget {
            level: AssignmentLevel::MachineSoftware,
            key: Some(machine_software_key(machine, list, name)),
        });
    }
    targets
        .into_iter()
        .map(|target| LevelAssignment {
            assignment: config.assignment(&target).cloned(),
            target,
        })
        .collect()
}

/// Resolves the controls of a launch. `request` is the temporary override;
/// it must reference existing profiles and controllers.
pub fn resolve(
    config: &ControllersConfig,
    machine: &str,
    software: Option<(&str, &str)>,
    request: Option<&Assignment>,
) -> Result<ResolvedControls, ControllersError> {
    let request = request.cloned().map(Assignment::normalized);
    let request = request.as_ref();
    if let Some(request) = request {
        config
            .validate_assignment(request)
            .map_err(ControllersError::Invalid)?;
    }
    let levels = levels(config, machine, software);
    let stored = levels.iter().filter_map(|level| {
        level
            .assignment
            .as_ref()
            .map(|assignment| (ControlsSource::from(level.target.level), assignment))
    });
    let ordered: Vec<(ControlsSource, &Assignment)> = stored
        .chain(request.map(|request| (ControlsSource::Request, request)))
        .collect();
    let (profile_id, profile_source) = ordered
        .iter()
        .rev()
        .find_map(|(source, assignment)| {
            assignment
                .profile_id
                .as_ref()
                .map(|profile_id| (profile_id.clone(), *source))
        })
        .unwrap_or_else(|| (MAME_PROFILE.to_owned(), ControlsSource::Default));
    let (players, players_source) = ordered
        .iter()
        .rev()
        .find_map(|(source, assignment)| {
            assignment
                .players
                .as_ref()
                .map(|players| (players.clone(), Some(*source)))
        })
        .unwrap_or_default();
    if config.profile(&profile_id).is_none() {
        // Stored assignments are validated; this only happens with a
        // configuration that was not.
        return Err(ControllersError::NotFound(format!(
            "the controller profile '{profile_id}' does not exist"
        )));
    }
    Ok(ResolvedControls {
        profile_id,
        profile_source,
        players,
        players_source,
    })
}
