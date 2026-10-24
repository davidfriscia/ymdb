//! Commands of controller management: controllers, controller profiles,
//! assignments and the resolution of the controls of a machine or software.
//!
//! The configuration is kept in memory and saved to `controllers.json` after
//! every change. A change is applied to a copy and becomes visible only once
//! saved, so a failed save leaves everything as it was. An unreadable file is
//! never replaced: every command reports it until it can be read.
//!
//! Controllers are logical identities: assigning them to players records a
//! preference that MAME does not receive yet (no `mapdevice` binding).

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::DialogExt;

use super::installations::InstallationsState;
use super::{CommandError, run_blocking};
use crate::controllers::model::{
    Assignment, AssignmentLevel, AssignmentTarget, AssignmentUpdate, Assignments, Controller,
    ControllerInput, ControllerProfile, ControllersConfig, ControllersError, ProfileKind,
};
use crate::controllers::resolve::{self, LevelAssignment, ResolvedControls};
use crate::controllers::store::ControllersStore;
use crate::controllers::{ControllerPaths, base_file};

/// Managed state: the controllers configuration, loaded once.
pub struct ControllersState {
    store: Option<ControllersStore>,
    config: Mutex<Option<ControllersConfig>>,
}

impl ControllersState {
    /// Loads `controllers.json` if possible; a failed load is retried on next use.
    pub fn new(config_dir: Option<&Path>) -> Self {
        let store = config_dir.map(ControllersStore::in_dir);
        let config = store.as_ref().and_then(|store| store.load().ok());
        Self {
            store,
            config: Mutex::new(config),
        }
    }

    fn store(&self) -> Result<&ControllersStore, CommandError> {
        self.store.as_ref().ok_or_else(|| CommandError::Settings {
            message: "the application configuration directory is unavailable".to_owned(),
        })
    }

    /// The loaded configuration, under the lock that serializes changes.
    fn loaded(&self) -> Result<MutexGuard<'_, Option<ControllersConfig>>, CommandError> {
        let mut guard = self.config.lock().unwrap_or_else(PoisonError::into_inner);
        if guard.is_none() {
            let config = self
                .store()?
                .load()
                .map_err(|error| CommandError::Settings {
                    message: error.to_string(),
                })?;
            *guard = Some(config);
        }
        Ok(guard)
    }

    pub fn read<T>(&self, read: impl FnOnce(&ControllersConfig) -> T) -> Result<T, CommandError> {
        let guard = self.loaded()?;
        let config = guard.as_ref().ok_or_else(not_loaded)?;
        Ok(read(config))
    }

    /// Applies `change` to a copy of the configuration, saves it, then keeps it.
    pub fn update<T>(
        &self,
        change: impl FnOnce(&mut ControllersConfig) -> Result<T, CommandError>,
    ) -> Result<T, CommandError> {
        let mut guard = self.loaded()?;
        let mut next = guard.as_ref().ok_or_else(not_loaded)?.clone();
        let result = change(&mut next)?;
        self.save(&next)?;
        *guard = Some(next);
        Ok(result)
    }

    fn save(&self, config: &ControllersConfig) -> Result<(), CommandError> {
        self.store()?
            .save(config)
            .map_err(|error| CommandError::Settings {
                message: error.to_string(),
            })
    }

    /// The configuration and file locations a launch is prepared with.
    pub fn snapshot(&self) -> Result<(ControllersConfig, ControllerPaths), CommandError> {
        let paths = self.store()?.paths().clone();
        let config = self.read(Clone::clone).map_err(|error| match error {
            CommandError::Settings { message } => CommandError::Settings {
                message: format!("{message}. Fix or remove the controllers file to launch."),
            },
            other => other,
        })?;
        Ok((config, paths))
    }

    /// Creates a user profile, with `base` (a validated controller file) as
    /// its `base.cfg`. The file is written before the profile is saved, and
    /// removed if the profile cannot be saved, so `controllers.json` never
    /// declares a missing `base.cfg`.
    pub fn create_profile(
        &self,
        name: &str,
        base: Option<&ControllerFile>,
    ) -> Result<String, CommandError> {
        let store = self.store()?;
        let mut guard = self.loaded()?;
        let mut next = guard.as_ref().ok_or_else(not_loaded)?.clone();
        let id = next.create_profile(name, false, |id| store.paths().is_used(id))?;
        if let Some(file) = base {
            next.set_profile_base_file(&id, Some(file.file_name.clone()))?;
            store
                .write_base(&id, &file.content)
                .map_err(|error| CommandError::Settings {
                    message: format!("failed to copy the controller file: {error}"),
                })?;
        }
        if let Err(error) = self.save(&next) {
            if base.is_some() {
                store.discard_base(&id);
            }
            return Err(error);
        }
        *guard = Some(next);
        Ok(id)
    }

    /// Replaces the `base.cfg` of a user profile with `file` (a validated
    /// controller file).
    pub fn import_base(&self, profile_id: &str, file: &ControllerFile) -> Result<(), CommandError> {
        let store = self.store()?;
        let mut guard = self.loaded()?;
        let mut next = guard.as_ref().ok_or_else(not_loaded)?.clone();
        next.set_profile_base_file(profile_id, Some(file.file_name.clone()))?;
        store
            .write_base(profile_id, &file.content)
            .map_err(|error| CommandError::Settings {
                message: format!("failed to copy the controller file: {error}"),
            })?;
        // The file now exists, so saving the flag cannot declare a missing file.
        self.save(&next)?;
        *guard = Some(next);
        Ok(())
    }

    /// Removes the `base.cfg` of a user profile. The profile, its MAME
    /// configuration directories and every other file are kept. The file is
    /// moved aside first and deleted only once the change is saved; if it
    /// cannot be saved, the file is put back.
    pub fn remove_base(&self, profile_id: &str) -> Result<(), CommandError> {
        let store = self.store()?;
        let mut guard = self.loaded()?;
        let mut next = guard.as_ref().ok_or_else(not_loaded)?.clone();
        let has_base = next
            .profile(profile_id)
            .is_some_and(|profile| profile.has_base);
        next.set_profile_base_file(profile_id, None)?;
        if !has_base {
            return Err(CommandError::InvalidRequest {
                message: format!("the controller profile '{profile_id}' has no base file"),
            });
        }
        let aside = store
            .set_base_aside(profile_id)
            .map_err(|error| CommandError::Settings {
                message: format!("failed to remove the controller file: {error}"),
            })?;
        if let Err(error) = store.save(&next) {
            let mut message = error.to_string();
            if let Some(aside) = &aside
                && let Err(restore) = store.restore_base(profile_id, aside)
            {
                message.push_str(&format!(
                    "; the controller file could not be put back ({restore}): import it again"
                ));
            }
            return Err(CommandError::Settings { message });
        }
        if let Some(aside) = &aside {
            store.delete_base_aside(profile_id, aside);
        }
        *guard = Some(next);
        Ok(())
    }
}

/// A controller configuration file chosen by the user and validated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControllerFile {
    pub content: Vec<u8>,
    /// The name of the chosen file, without its directory.
    pub file_name: Option<String>,
}

fn not_loaded() -> CommandError {
    CommandError::Internal {
        message: "the controllers configuration is not loaded".to_owned(),
    }
}

impl From<ControllersError> for CommandError {
    fn from(error: ControllersError) -> Self {
        match error {
            ControllersError::NotFound(message) => Self::NotFound { message },
            ControllersError::Invalid(message) => Self::InvalidRequest { message },
            ControllersError::InUse {
                message,
                references,
            } => Self::ControllersInUse {
                message,
                references,
            },
        }
    }
}

/// A controller profile for the frontend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ControllerProfileView {
    pub id: String,
    pub kind: ProfileKind,
    pub name: String,
    pub has_base: bool,
    /// Name of the file the `base.cfg` was imported from, when known.
    pub base_file_name: Option<String>,
    /// Whether launches with this profile use a configuration directory of
    /// YMDB (`false` for the `mame` profile).
    pub managed: bool,
}

impl From<&ControllerProfile> for ControllerProfileView {
    fn from(profile: &ControllerProfile) -> Self {
        Self {
            id: profile.id.clone(),
            kind: profile.kind,
            name: profile.display_name().to_owned(),
            has_base: profile.has_base,
            base_file_name: profile.base_file_name.clone(),
            managed: profile.is_managed(),
        }
    }
}

/// The whole controllers configuration, returned by every command that changes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ControllersView {
    pub controllers: Vec<Controller>,
    pub profiles: Vec<ControllerProfileView>,
    pub assignments: Assignments,
}

pub fn controllers_view(config: &ControllersConfig) -> ControllersView {
    ControllersView {
        controllers: config.controllers.clone(),
        profiles: config.profiles.iter().map(Into::into).collect(),
        assignments: config.assignments.clone(),
    }
}

/// A software, by list and short name.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SoftwareKey {
    pub list: String,
    pub name: String,
}

/// The effective controls of a machine or software and how they are inherited.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlsResolution {
    #[serde(flatten)]
    pub effective: ResolvedControls,
    pub profile_name: String,
    /// The levels that apply, from the weakest, with what each one stores.
    pub levels: Vec<LevelAssignment>,
    /// MAME's configuration directory of the profile for the installation
    /// (`None` for the `mame` profile). It may not exist yet: it is created
    /// when MAME first starts with the profile.
    pub cfg_directory: Option<PathBuf>,
}

/// Resolves the controls of `machine` (running `software`, if any) without
/// any side effect. Names are not looked up in a catalog.
pub fn resolve_controls_with(
    config: &ControllersConfig,
    paths: &ControllerPaths,
    installation_id: &str,
    machine: &str,
    software: Option<&SoftwareKey>,
) -> Result<ControlsResolution, CommandError> {
    let software = software.map(|key| (key.list.as_str(), key.name.as_str()));
    let target = |level, key: String| AssignmentTarget {
        level,
        key: Some(key),
    };
    // Same validation as stored keys: MAME short names only.
    let mut checks = vec![target(AssignmentLevel::Machine, machine.to_owned())];
    if let Some((list, name)) = software {
        checks.push(target(
            AssignmentLevel::Software,
            crate::controllers::model::software_key(list, name),
        ));
    }
    for check in &checks {
        check
            .validate()
            .map_err(|message| CommandError::InvalidRequest { message })?;
    }
    let effective = resolve::resolve(config, machine, software, None)?;
    let profile = config
        .profile(&effective.profile_id)
        .ok_or_else(|| CommandError::NotFound {
            message: format!(
                "the controller profile '{}' does not exist",
                effective.profile_id
            ),
        })?;
    Ok(ControlsResolution {
        profile_name: profile.display_name().to_owned(),
        levels: resolve::levels(config, machine, software),
        cfg_directory: profile
            .is_managed()
            .then(|| paths.context_dir(installation_id, &profile.id)),
        effective,
    })
}

#[tauri::command]
pub async fn get_controllers(app: AppHandle) -> Result<ControllersView, CommandError> {
    run_blocking(move || app.state::<ControllersState>().read(controllers_view)).await?
}

#[tauri::command]
pub async fn create_controller(
    controller: ControllerInput,
    app: AppHandle,
) -> Result<ControllersView, CommandError> {
    run_blocking(move || {
        let state = app.state::<ControllersState>();
        state.update(|config| Ok(config.create_controller(controller)?))?;
        state.read(controllers_view)
    })
    .await?
}

#[tauri::command]
pub async fn update_controller(
    controller_id: String,
    controller: ControllerInput,
    app: AppHandle,
) -> Result<ControllersView, CommandError> {
    run_blocking(move || {
        let state = app.state::<ControllersState>();
        state.update(|config| Ok(config.update_controller(&controller_id, controller)?))?;
        state.read(controllers_view)
    })
    .await?
}

/// Removes a controller; refused while an assignment uses it.
#[tauri::command]
pub async fn delete_controller(
    controller_id: String,
    app: AppHandle,
) -> Result<ControllersView, CommandError> {
    run_blocking(move || {
        let state = app.state::<ControllersState>();
        state.update(|config| Ok(config.delete_controller(&controller_id)?))?;
        state.read(controllers_view)
    })
    .await?
}

/// Creates a user profile. With `importBase`, the user first chooses a MAME
/// controller configuration file, which is checked and copied as the
/// profile's `base.cfg`; `None` when the user cancels that choice.
#[tauri::command]
pub async fn create_controller_profile(
    name: String,
    import_base: bool,
    app: AppHandle,
) -> Result<Option<ControllersView>, CommandError> {
    run_blocking(move || {
        let base = if import_base {
            match pick_controller_file(&app)? {
                Some(file) => Some(file),
                None => return Ok(None),
            }
        } else {
            None
        };
        let state = app.state::<ControllersState>();
        state.create_profile(&name, base.as_ref())?;
        state.read(controllers_view).map(Some)
    })
    .await?
}

#[tauri::command]
pub async fn rename_controller_profile(
    profile_id: String,
    name: String,
    app: AppHandle,
) -> Result<ControllersView, CommandError> {
    run_blocking(move || {
        let state = app.state::<ControllersState>();
        state.update(|config| Ok(config.rename_profile(&profile_id, &name)?))?;
        state.read(controllers_view)
    })
    .await?
}

/// Removes a user profile; refused while an assignment uses it. Its
/// `base.cfg` and the MAME configuration directories of its installations
/// are kept.
#[tauri::command]
pub async fn delete_controller_profile(
    profile_id: String,
    app: AppHandle,
) -> Result<ControllersView, CommandError> {
    run_blocking(move || {
        let state = app.state::<ControllersState>();
        state.update(|config| Ok(config.delete_profile(&profile_id)?))?;
        state.read(controllers_view)
    })
    .await?
}

/// Lets the user choose a MAME controller configuration file and copies it
/// as the `base.cfg` of a user profile, replacing the previous one. The
/// chosen file is only read. `None` when the user cancels.
#[tauri::command]
pub async fn import_controller_profile_base(
    profile_id: String,
    app: AppHandle,
) -> Result<Option<ControllersView>, CommandError> {
    run_blocking(move || {
        let state = app.state::<ControllersState>();
        // Fail before opening the dialog when the profile cannot have a base.
        state.read(|config| {
            config
                .profile(&profile_id)
                .filter(|profile| profile.is_managed())
                .map(|_| ())
                .ok_or_else(|| CommandError::InvalidRequest {
                    message: format!("'{profile_id}' is not a user controller profile"),
                })
        })??;
        let Some(file) = pick_controller_file(&app)? else {
            return Ok(None);
        };
        state.import_base(&profile_id, &file)?;
        state.read(controllers_view).map(Some)
    })
    .await?
}

/// Removes the `base.cfg` of a user profile. The profile and the MAME
/// configuration saved in its directories are kept; MAME files are never
/// touched.
#[tauri::command]
pub async fn remove_controller_profile_base(
    profile_id: String,
    app: AppHandle,
) -> Result<ControllersView, CommandError> {
    run_blocking(move || {
        let state = app.state::<ControllersState>();
        state.remove_base(&profile_id)?;
        state.read(controllers_view)
    })
    .await?
}

/// Sets the assignment of the default, a machine, a software or a machine
/// running a software. An empty assignment resets a machine or software
/// level, whose fields are then inherited again.
#[tauri::command]
pub async fn set_controller_assignment(
    target: AssignmentTarget,
    assignment: Assignment,
    app: AppHandle,
) -> Result<ControllersView, CommandError> {
    run_blocking(move || {
        let state = app.state::<ControllersState>();
        state.update(|config| Ok(config.set_assignment(&target, assignment)?))?;
        state.read(controllers_view)
    })
    .await?
}

/// Changes some fields of the assignment of the default, a machine, a
/// software or a machine running a software, keeping the others: a field
/// set to `inherit` is inherited again while the others stay. A machine or
/// software assignment left empty is removed.
#[tauri::command]
pub async fn update_controller_assignment(
    target: AssignmentTarget,
    update: AssignmentUpdate,
    app: AppHandle,
) -> Result<ControllersView, CommandError> {
    run_blocking(move || {
        let state = app.state::<ControllersState>();
        state.update(|config| Ok(config.update_assignment(&target, update)?))?;
        state.read(controllers_view)
    })
    .await?
}

/// Removes the assignment of a machine or software level; the default
/// assignment returns to the `mame` profile without controllers.
#[tauri::command]
pub async fn reset_controller_assignment(
    target: AssignmentTarget,
    app: AppHandle,
) -> Result<ControllersView, CommandError> {
    run_blocking(move || {
        let state = app.state::<ControllersState>();
        state.update(|config| Ok(config.reset_assignment(&target)?))?;
        state.read(controllers_view)
    })
    .await?
}

/// The effective controls of a machine, or of a machine running a software,
/// for an installation, with their inheritance. Nothing is created.
#[tauri::command]
pub async fn resolve_controls(
    installation_id: String,
    machine: String,
    software: Option<SoftwareKey>,
    app: AppHandle,
) -> Result<ControlsResolution, CommandError> {
    run_blocking(move || {
        let resolved = app
            .state::<InstallationsState>()
            .resolve(&installation_id)?;
        let state = app.state::<ControllersState>();
        let paths = state.store()?.paths().clone();
        state.read(|config| {
            resolve_controls_with(
                config,
                &paths,
                &resolved.installation.id,
                &machine,
                software.as_ref(),
            )
        })?
    })
    .await?
}

/// Lets the user choose a controller configuration file and checks it.
fn pick_controller_file(app: &AppHandle) -> Result<Option<ControllerFile>, CommandError> {
    let selection = app
        .dialog()
        .file()
        .set_title("Choose a MAME controller configuration file")
        .add_filter("MAME configuration", &["cfg"])
        .blocking_pick_file();
    let Some(selection) = selection else {
        return Ok(None);
    };
    let path = selection
        .into_path()
        .map_err(|error| CommandError::Internal {
            message: format!("unsupported file selection: {error}"),
        })?;
    let content =
        base_file::read(&path).map_err(|message| CommandError::InvalidFile { message })?;
    Ok(Some(ControllerFile {
        content,
        file_name: path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned()),
    }))
}

#[cfg(test)]
mod tests;
