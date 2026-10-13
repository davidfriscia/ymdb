//! Media source commands and the `ymdb-media` protocol handler.

use std::path::PathBuf;
use std::sync::{PoisonError, RwLock};

use serde::Serialize;
use tauri::http::{Response, header};
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::DialogExt;

use super::{CommandError, run_blocking};
use crate::media::config::{
    CATEGORIES, MediaCategory, MediaConfig, MediaConfigStore, MediaStoreError, SourceStatus,
    source_status,
};
use crate::media::protocol::{CategoryImages, MediaTarget, list_images, respond};

/// Managed state: the media configuration is kept in memory because every
/// image request reads it.
pub struct MediaState {
    store: Option<MediaConfigStore>,
    config: RwLock<MediaConfig>,
    load_error: RwLock<Option<String>>,
}

impl MediaState {
    /// Loads `media.json`; an unreadable file means no sources until saved.
    pub fn new(store: Option<MediaConfigStore>) -> Self {
        let (config, load_error) = match store.as_ref().map(MediaConfigStore::load) {
            Some(Ok(config)) => (config, None),
            Some(Err(error)) => (MediaConfig::new(), Some(error.to_string())),
            None => (MediaConfig::new(), None),
        };
        Self {
            store,
            config: RwLock::new(config),
            load_error: RwLock::new(load_error),
        }
    }

    fn config(&self) -> MediaConfig {
        self.config
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceInfo {
    pub path: PathBuf,
    pub status: SourceStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CategorySources {
    pub category: String,
    pub sources: Vec<SourceInfo>,
}

/// The media configuration with the current status of every directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaSourcesView {
    pub categories: &'static [MediaCategory],
    pub config: MediaConfig,
    /// Directories per category, in order of precedence, with their status.
    pub sources: Vec<CategorySources>,
    /// Set when `media.json` could not be read; saving replaces it.
    pub load_error: Option<String>,
}

fn view(config: MediaConfig, load_error: Option<String>) -> MediaSourcesView {
    let sources = CATEGORIES
        .iter()
        .map(|category| {
            let infos = config
                .sources(category.id)
                .iter()
                .map(|path| SourceInfo {
                    path: path.clone(),
                    status: source_status(path),
                })
                .collect();
            CategorySources {
                category: category.id.to_owned(),
                sources: infos,
            }
        })
        .collect();
    MediaSourcesView {
        categories: CATEGORIES,
        config,
        sources,
        load_error,
    }
}

#[tauri::command]
pub async fn get_media_sources(app: AppHandle) -> Result<MediaSourcesView, CommandError> {
    // Checking directories may be slow (network shares): off the main thread.
    run_blocking(move || {
        let state = app.state::<MediaState>();
        let load_error = state
            .load_error
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        view(state.config(), load_error)
    })
    .await
}

#[tauri::command]
pub async fn save_media_sources(
    config: MediaConfig,
    app: AppHandle,
) -> Result<MediaSourcesView, CommandError> {
    run_blocking(move || {
        let state = app.state::<MediaState>();
        let store = state.store.as_ref().ok_or_else(|| CommandError::Settings {
            message: "the application configuration directory is unavailable".to_owned(),
        })?;
        // Held while saving so that concurrent saves cannot interleave.
        let mut current = state.config.write().unwrap_or_else(PoisonError::into_inner);
        let saved = store.save(config).map_err(|error| match error {
            MediaStoreError::Rejected(message) => CommandError::InvalidRequest { message },
            other => CommandError::Settings {
                message: other.to_string(),
            },
        })?;
        *current = saved.clone();
        drop(current);
        *state
            .load_error
            .write()
            .unwrap_or_else(PoisonError::into_inner) = None;
        Ok(view(saved, None))
    })
    .await?
}

/// Opens the native directory picker. Returns `None` when the user cancels.
#[tauri::command]
pub async fn pick_media_directory(app: AppHandle) -> Result<Option<PathBuf>, CommandError> {
    let selection = run_blocking(move || {
        app.dialog()
            .file()
            .set_title("Select an image directory")
            .blocking_pick_folder()
    })
    .await?;
    selection
        .map(|folder| folder.into_path())
        .transpose()
        .map_err(|error| CommandError::Internal {
            message: format!("unsupported folder selection: {error}"),
        })
}

/// The categories with images of `target` and their number of images. Only
/// the files of this title are looked up, off the main thread.
#[tauri::command]
pub async fn list_media(
    target: MediaTarget,
    app: AppHandle,
) -> Result<Vec<CategoryImages>, CommandError> {
    run_blocking(move || list_images(&app.state::<MediaState>().config(), &target)).await
}

/// Answers a `ymdb-media` request. Runs on the blocking pool (file I/O).
pub fn protocol_response(app: &AppHandle, uri_path: &str) -> Response<Vec<u8>> {
    let config = app.state::<MediaState>().config();
    let media = respond(&config, uri_path);
    let cache = if media.status == 200 {
        // URLs carry the configuration version, so caching is safe.
        "max-age=3600"
    } else {
        "no-store"
    };
    Response::builder()
        .status(media.status)
        .header(header::CONTENT_TYPE, media.content_type)
        .header(header::CACHE_CONTROL, cache)
        .header("X-Content-Type-Options", "nosniff")
        .body(media.body)
        .unwrap_or_else(|_| Response::new(Vec::new()))
}
