//! Commands of the saved filter presets (`saved-filters.json`).
//!
//! As with the lists, a change is applied to a copy, saved, and only then
//! kept; an unreadable or invalid file is reported by every command and
//! never replaced.

use std::path::Path;
use std::sync::{Mutex, MutexGuard, PoisonError};

use serde::Serialize;
use tauri::{AppHandle, Manager};

use super::catalog::now_millis;
use super::{CommandError, data_change, run_blocking};
use crate::saved_filters::{SavedFilter, SavedFilters, SavedFiltersStore};

pub struct SavedFiltersState {
    store: Option<SavedFiltersStore>,
    filters: Mutex<Option<SavedFilters>>,
}

impl SavedFiltersState {
    pub fn new(config_dir: Option<&Path>) -> Self {
        let store = config_dir.map(SavedFiltersStore::in_dir);
        let filters = store.as_ref().and_then(|store| store.load().ok());
        Self {
            store,
            filters: Mutex::new(filters),
        }
    }

    fn store(&self) -> Result<&SavedFiltersStore, CommandError> {
        self.store.as_ref().ok_or_else(|| CommandError::Settings {
            message: "the application configuration directory is unavailable".to_owned(),
        })
    }

    fn loaded(&self) -> Result<MutexGuard<'_, Option<SavedFilters>>, CommandError> {
        let mut guard = self.filters.lock().unwrap_or_else(PoisonError::into_inner);
        if guard.is_none() {
            let filters = self
                .store()?
                .load()
                .map_err(|error| CommandError::Settings {
                    message: error.to_string(),
                })?;
            *guard = Some(filters);
        }
        Ok(guard)
    }

    pub fn presets(&self) -> Result<Vec<SavedFilter>, CommandError> {
        let guard = self.loaded()?;
        Ok(guard
            .as_ref()
            .map(|filters| filters.presets.clone())
            .unwrap_or_default())
    }

    /// Applies `change` to a copy, saves it, then keeps it.
    pub fn update<T>(
        &self,
        change: impl FnOnce(&mut SavedFilters) -> Result<T, String>,
    ) -> Result<(T, Vec<SavedFilter>), CommandError> {
        let mut guard = self.loaded()?;
        let mut next = guard.clone().unwrap_or_default();
        let value =
            change(&mut next).map_err(|message| CommandError::InvalidRequest { message })?;
        self.store()?
            .save(&next)
            .map_err(|error| CommandError::Settings {
                message: error.to_string(),
            })?;
        let presets = next.presets.clone();
        *guard = Some(next);
        Ok((value, presets))
    }
}

/// The presets after a creation, with the id of the new one.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedFilterCreated {
    pub id: String,
    pub presets: Vec<SavedFilter>,
}

#[tauri::command]
pub async fn get_saved_filters(app: AppHandle) -> Result<Vec<SavedFilter>, CommandError> {
    run_blocking(move || app.state::<SavedFiltersState>().presets()).await?
}

#[tauri::command]
pub async fn create_saved_filter(
    name: String,
    criteria_version: u32,
    criteria: serde_json::Value,
    app: AppHandle,
) -> Result<SavedFilterCreated, CommandError> {
    run_blocking(move || {
        let _change = data_change()?;
        let (id, presets) = app
            .state::<SavedFiltersState>()
            .update(|filters| filters.create(&name, criteria_version, criteria, now_millis()))?;
        Ok(SavedFilterCreated { id, presets })
    })
    .await?
}

/// Replaces the criteria of a preset with the current ones.
#[tauri::command]
pub async fn update_saved_filter(
    id: String,
    criteria_version: u32,
    criteria: serde_json::Value,
    app: AppHandle,
) -> Result<Vec<SavedFilter>, CommandError> {
    run_blocking(move || {
        let _change = data_change()?;
        let ((), presets) = app
            .state::<SavedFiltersState>()
            .update(|filters| filters.update(&id, criteria_version, criteria, now_millis()))?;
        Ok(presets)
    })
    .await?
}

#[tauri::command]
pub async fn rename_saved_filter(
    id: String,
    name: String,
    app: AppHandle,
) -> Result<Vec<SavedFilter>, CommandError> {
    run_blocking(move || {
        let _change = data_change()?;
        let ((), presets) = app
            .state::<SavedFiltersState>()
            .update(|filters| filters.rename(&id, &name, now_millis()))?;
        Ok(presets)
    })
    .await?
}

#[tauri::command]
pub async fn delete_saved_filter(
    id: String,
    app: AppHandle,
) -> Result<Vec<SavedFilter>, CommandError> {
    run_blocking(move || {
        let _change = data_change()?;
        let ((), presets) = app
            .state::<SavedFiltersState>()
            .update(|filters| filters.delete(&id))?;
        Ok(presets)
    })
    .await?
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn keeps_presets_in_memory_only_once_saved() {
        let dir = tempfile::tempdir().expect("temp dir");
        let state = SavedFiltersState::new(Some(dir.path()));
        let (id, presets) = state
            .update(|filters| filters.create("Shooters", 1, json!({}), 1))
            .expect("create");
        assert_eq!(presets[0].id, id);
        assert!(dir.path().join("saved-filters.json").is_file());

        // A refused change leaves the presets and the file as they were.
        let refused = state.update(|filters| filters.create("shooters", 1, json!({}), 2));
        assert!(matches!(refused, Err(CommandError::InvalidRequest { .. })));
        assert_eq!(state.presets().expect("presets").len(), 1);

        // Reloaded from the file by a new state.
        let reloaded = SavedFiltersState::new(Some(dir.path()));
        assert_eq!(reloaded.presets().expect("presets"), presets);
    }

    #[test]
    fn reports_an_invalid_file_on_every_command() {
        let dir = tempfile::tempdir().expect("temp dir");
        std::fs::write(dir.path().join("saved-filters.json"), "{ broken").expect("write");
        let state = SavedFiltersState::new(Some(dir.path()));
        assert!(matches!(
            state.presets(),
            Err(CommandError::Settings { .. })
        ));
        let refused = state.update(|filters| filters.create("New", 1, json!({}), 1));
        assert!(matches!(refused, Err(CommandError::Settings { .. })));
        assert_eq!(
            std::fs::read_to_string(dir.path().join("saved-filters.json")).expect("read"),
            "{ broken"
        );
    }
}
