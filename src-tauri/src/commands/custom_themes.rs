//! Commands of the custom themes (`custom-themes.json`) and of theme files.
//!
//! As with the saved filters, a change is applied to a copy, saved, and only
//! then kept; an unreadable or invalid file is reported by every command and
//! never replaced.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use serde::Serialize;
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::DialogExt;

use super::catalog::now_millis;
use super::{CommandError, data_change, run_blocking};
use crate::atomic_file;
use crate::custom_themes::{self, CustomTheme, CustomThemes, CustomThemesStore, NewTheme};

pub struct CustomThemesState {
    store: Option<CustomThemesStore>,
    themes: Mutex<Option<CustomThemes>>,
}

impl CustomThemesState {
    pub fn new(config_dir: Option<&Path>) -> Self {
        let store = config_dir.map(CustomThemesStore::in_dir);
        let themes = store.as_ref().and_then(|store| store.load().ok());
        Self {
            store,
            themes: Mutex::new(themes),
        }
    }

    fn store(&self) -> Result<&CustomThemesStore, CommandError> {
        self.store.as_ref().ok_or_else(|| CommandError::Settings {
            message: "the application configuration directory is unavailable".to_owned(),
        })
    }

    fn loaded(&self) -> Result<MutexGuard<'_, Option<CustomThemes>>, CommandError> {
        let mut guard = self.themes.lock().unwrap_or_else(PoisonError::into_inner);
        if guard.is_none() {
            let themes = self
                .store()?
                .load()
                .map_err(|error| CommandError::Settings {
                    message: error.to_string(),
                })?;
            *guard = Some(themes);
        }
        Ok(guard)
    }

    pub fn themes(&self) -> Result<Vec<CustomTheme>, CommandError> {
        let guard = self.loaded()?;
        Ok(guard
            .as_ref()
            .map(|themes| themes.themes.clone())
            .unwrap_or_default())
    }

    /// Applies `change` to a copy, saves it, then keeps it.
    pub fn update<T>(
        &self,
        change: impl FnOnce(&mut CustomThemes) -> Result<T, String>,
    ) -> Result<(T, Vec<CustomTheme>), CommandError> {
        let mut guard = self.loaded()?;
        let mut next = guard.clone().unwrap_or_default();
        let value =
            change(&mut next).map_err(|message| CommandError::InvalidRequest { message })?;
        self.store()?
            .save(&next)
            .map_err(|error| CommandError::Settings {
                message: error.to_string(),
            })?;
        let themes = next.themes.clone();
        *guard = Some(next);
        Ok((value, themes))
    }
}

/// The themes after a creation, with the id of the new one.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomThemeCreated {
    pub id: String,
    pub themes: Vec<CustomTheme>,
}

/// A theme file chosen for an import, not checked yet beyond JSON and size.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeFile {
    pub file_name: String,
    pub document: serde_json::Value,
}

#[tauri::command]
pub async fn get_custom_themes(app: AppHandle) -> Result<Vec<CustomTheme>, CommandError> {
    run_blocking(move || app.state::<CustomThemesState>().themes()).await?
}

/// Creates a theme from a clone or an import; never replaces an existing one.
#[tauri::command]
pub async fn create_custom_theme(
    name: String,
    source_theme_id: Option<String>,
    tokens_version: u32,
    tokens: serde_json::Value,
    app: AppHandle,
) -> Result<CustomThemeCreated, CommandError> {
    run_blocking(move || {
        let _change = data_change()?;
        let theme = NewTheme {
            name,
            source_theme_id,
            tokens_version,
            tokens,
        };
        let (id, themes) = app
            .state::<CustomThemesState>()
            .update(|themes| themes.create(theme, now_millis()))?;
        Ok(CustomThemeCreated { id, themes })
    })
    .await?
}

/// Saves the name and the tokens of a theme edited in the Theme Editor.
#[tauri::command]
pub async fn update_custom_theme(
    id: String,
    name: String,
    tokens_version: u32,
    tokens: serde_json::Value,
    app: AppHandle,
) -> Result<Vec<CustomTheme>, CommandError> {
    run_blocking(move || {
        let _change = data_change()?;
        let ((), themes) = app
            .state::<CustomThemesState>()
            .update(|themes| themes.update(&id, &name, tokens_version, tokens, now_millis()))?;
        Ok(themes)
    })
    .await?
}

#[tauri::command]
pub async fn rename_custom_theme(
    id: String,
    name: String,
    app: AppHandle,
) -> Result<Vec<CustomTheme>, CommandError> {
    run_blocking(move || {
        let _change = data_change()?;
        let ((), themes) = app
            .state::<CustomThemesState>()
            .update(|themes| themes.rename(&id, &name, now_millis()))?;
        Ok(themes)
    })
    .await?
}

#[tauri::command]
pub async fn delete_custom_theme(
    id: String,
    app: AppHandle,
) -> Result<Vec<CustomTheme>, CommandError> {
    run_blocking(move || {
        let _change = data_change()?;
        let ((), themes) = app
            .state::<CustomThemesState>()
            .update(|themes| themes.delete(&id))?;
        Ok(themes)
    })
    .await?
}

/// Lets the user choose a theme file and reads it. Nothing changes until the
/// theme is imported. `None` when the user cancels.
#[tauri::command]
pub async fn choose_theme_file(app: AppHandle) -> Result<Option<ThemeFile>, CommandError> {
    run_blocking(move || {
        let selection = app
            .dialog()
            .file()
            .set_title("Import theme")
            .add_filter("YMDB theme", &["json"])
            .blocking_pick_file();
        let Some(selection) = selection else {
            return Ok(None);
        };
        let path = selection
            .into_path()
            .map_err(|error| CommandError::Internal {
                message: format!("unsupported file selection: {error}"),
            })?;
        let document = custom_themes::read_theme_file(&path)
            .map_err(|message| CommandError::InvalidFile { message })?;
        let file_name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        Ok(Some(ThemeFile {
            file_name,
            document,
        }))
    })
    .await?
}

/// Writes a theme document, built by the frontend, to a file chosen with the
/// native save dialog. Returns the file written, or `None` when the user
/// cancels.
#[tauri::command]
pub async fn export_theme_file(
    name: String,
    document: serde_json::Value,
    app: AppHandle,
) -> Result<Option<PathBuf>, CommandError> {
    run_blocking(move || {
        let content = custom_themes::theme_file_content(&document)
            .map_err(|message| CommandError::InvalidRequest { message })?;
        let selection = app
            .dialog()
            .file()
            .set_title("Export theme")
            .set_file_name(custom_themes::export_file_name(&name))
            .add_filter("YMDB theme", &["json"])
            .blocking_save_file();
        let Some(selection) = selection else {
            return Ok(None);
        };
        let path = selection
            .into_path()
            .map_err(|error| CommandError::Internal {
                message: format!("unsupported file selection: {error}"),
            })?;
        atomic_file::write(&path, &content).map_err(|error| CommandError::Settings {
            message: format!("failed to write '{}': {error}", path.display()),
        })?;
        Ok(Some(path))
    })
    .await?
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn theme(name: &str) -> NewTheme {
        NewTheme {
            name: name.to_owned(),
            source_theme_id: Some("light".to_owned()),
            tokens_version: 1,
            tokens: json!({}),
        }
    }

    #[test]
    fn keeps_themes_in_memory_only_once_saved() {
        let dir = tempfile::tempdir().expect("temp dir");
        let state = CustomThemesState::new(Some(dir.path()));
        let (id, themes) = state
            .update(|themes| themes.create(theme("Paper"), 1))
            .expect("create");
        assert_eq!(themes[0].id, id);
        assert!(dir.path().join("custom-themes.json").is_file());

        // A refused change leaves the themes and the file as they were.
        let refused = state.update(|themes| themes.create(theme("paper"), 2));
        assert!(matches!(refused, Err(CommandError::InvalidRequest { .. })));
        assert_eq!(state.themes().expect("themes").len(), 1);

        // Reloaded from the file by a new state.
        let reloaded = CustomThemesState::new(Some(dir.path()));
        assert_eq!(reloaded.themes().expect("themes"), themes);
    }

    #[test]
    fn reports_an_invalid_file_on_every_command() {
        let dir = tempfile::tempdir().expect("temp dir");
        std::fs::write(dir.path().join("custom-themes.json"), "{ broken").expect("write");
        let state = CustomThemesState::new(Some(dir.path()));
        assert!(matches!(state.themes(), Err(CommandError::Settings { .. })));
        let refused = state.update(|themes| themes.create(theme("New"), 1));
        assert!(matches!(refused, Err(CommandError::Settings { .. })));
        assert_eq!(
            std::fs::read_to_string(dir.path().join("custom-themes.json")).expect("read"),
            "{ broken"
        );
    }

    #[test]
    fn serializes_command_results() {
        let file = ThemeFile {
            file_name: "a.ymdb-theme.json".to_owned(),
            document: json!({ "format": "ymdb-theme" }),
        };
        assert_eq!(
            serde_json::to_value(file).expect("serialize"),
            json!({ "fileName": "a.ymdb-theme.json", "document": { "format": "ymdb-theme" } })
        );
    }
}
