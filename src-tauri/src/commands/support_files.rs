//! Commands of the MAME support files and of the catalog classifications.
//!
//! The global repository (`support-files.sqlite` in the local data
//! directory) is changed only by explicit user actions: an import previewed
//! then confirmed, a removal, a change of active source. Catalog projections
//! are brought up to date when a catalog is accessed (status, search,
//! detail), from a cached snapshot of the repository revisions, so an access
//! without changes costs a few small queries and rebuilds nothing.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::DialogExt;

use super::catalog::now_millis;
use super::installations::InstallationsState;
use super::{CommandError, data_change, run_blocking};
use crate::catalog::classification::{
    self, ClassifiedDimension, ProjectionTarget, SyncOutcome, classified_dimensions,
};
use crate::catalog::search::request::MAX_FILTER_VALUES;
use crate::catalog::store::CatalogStore;
use crate::support_files::model::{
    Dimension, DimensionStatistics, ParseWarning, ParsedSupportFile, SupportFileFormat,
};
use crate::support_files::parser::{self, ParseError};
use crate::support_files::repository::{
    RepositoryError, RepositorySnapshot, SourceView, SupportFilesView, SupportRepository,
};

/// Managed state: the repository, a snapshot of its revisions and the import
/// waiting for confirmation.
pub struct SupportFilesState {
    repository: Option<SupportRepository>,
    /// Serializes changes, and keeps the snapshot in line with them.
    snapshot: Mutex<Option<RepositorySnapshot>>,
    pending_import: Mutex<Option<PendingImport>>,
}

struct PendingImport {
    id: u64,
    file_name: String,
    parsed: ParsedSupportFile,
}

impl SupportFilesState {
    pub fn new(data_dir: Option<&Path>) -> Self {
        Self {
            repository: data_dir.map(SupportRepository::in_dir),
            snapshot: Mutex::new(None),
            pending_import: Mutex::new(None),
        }
    }

    fn repository(&self) -> Result<&SupportRepository, CommandError> {
        self.repository
            .as_ref()
            .ok_or_else(|| CommandError::Settings {
                message: "the application data directory is unavailable".to_owned(),
            })
    }

    fn snapshot_guard(&self) -> MutexGuard<'_, Option<RepositorySnapshot>> {
        self.snapshot.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// What catalog projections must reflect now. The snapshot is read from
    /// the repository once, then refreshed by every change made here.
    pub fn target(&self) -> Result<ProjectionTarget, CommandError> {
        let repository = self.repository()?;
        let mut snapshot = self.snapshot_guard();
        if snapshot.is_none() {
            *snapshot = Some(repository.snapshot().map_err(CommandError::from)?);
        }
        let snapshot = snapshot.as_ref().expect("snapshot was just read");
        Ok(ProjectionTarget::new(repository.path(), snapshot))
    }

    /// Runs `change` on the repository, then refreshes the snapshot.
    fn change<T>(
        &self,
        change: impl FnOnce(&SupportRepository) -> Result<T, RepositoryError>,
    ) -> Result<(T, SupportFilesView), CommandError> {
        let repository = self.repository()?;
        let mut snapshot = self.snapshot_guard();
        let result = change(repository);
        // Read again even after a failure: nothing is assumed about the file.
        *snapshot = repository.snapshot().ok();
        let value = result?;
        Ok((value, repository.view()?))
    }

    pub fn view(&self) -> Result<SupportFilesView, CommandError> {
        Ok(self.repository()?.view()?)
    }

    /// Checks a parsed file and keeps it until it is applied or replaced.
    pub fn prepare_import(
        &self,
        file_name: String,
        parsed: ParsedSupportFile,
        expected: Option<SupportFileFormat>,
    ) -> Result<SupportFileImportPreview, CommandError> {
        if let Some(expected) = expected
            && expected != parsed.format
        {
            return Err(CommandError::InvalidFile {
                message: format!(
                    "the file is a {}, not a {expected}: import it as a new source instead",
                    parsed.format
                ),
            });
        }
        let view = self.view()?;
        let format = parsed.format;
        let dimensions = parsed
            .statistics()
            .into_iter()
            .map(|statistics| {
                let active = view
                    .dimensions
                    .iter()
                    .find(|entry| entry.dimension == statistics.dimension)
                    .and_then(|entry| entry.active_source);
                PreviewDimension {
                    statistics,
                    active_after_import: active.is_none() || active == Some(format),
                }
            })
            .collect();
        let replaces = view
            .sources
            .into_iter()
            .find(|source| source.format == format);
        let mut pending = self
            .pending_import
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let id = pending.as_ref().map_or(1, |previous| previous.id + 1);
        let preview = SupportFileImportPreview {
            import_id: id,
            format,
            file_name: file_name.clone(),
            declared_version: parsed.declared_version.clone(),
            machine_count: parsed.machine_count(),
            dimensions,
            warnings: parsed.warnings.clone(),
            replaces,
        };
        *pending = Some(PendingImport {
            id,
            file_name,
            parsed,
        });
        Ok(preview)
    }

    /// Stores the prepared import `import_id`.
    pub fn apply_import(
        &self,
        import_id: u64,
        imported_at: u64,
    ) -> Result<SupportFilesView, CommandError> {
        let mut pending = self
            .pending_import
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let prepared = match pending.as_ref() {
            Some(prepared) if prepared.id == import_id => prepared,
            _ => {
                return Err(CommandError::InvalidRequest {
                    message: "this import is no longer available; choose the file again".to_owned(),
                });
            }
        };
        let (_, view) = self.change(|repository| {
            repository.import(&prepared.file_name, &prepared.parsed, imported_at)
        })?;
        *pending = None;
        Ok(view)
    }

    pub fn remove(&self, format: SupportFileFormat) -> Result<SupportFilesView, CommandError> {
        Ok(self.change(|repository| repository.remove(format))?.1)
    }

    pub fn set_active(
        &self,
        dimension: Dimension,
        format: Option<SupportFileFormat>,
    ) -> Result<SupportFilesView, CommandError> {
        Ok(self
            .change(|repository| repository.set_active(dimension, format))?
            .1)
    }
}

impl From<RepositoryError> for CommandError {
    fn from(error: RepositoryError) -> Self {
        match error {
            RepositoryError::NotFound(message) => Self::NotFound { message },
            RepositoryError::Invalid(message) => Self::InvalidRequest { message },
            RepositoryError::Sqlite(_)
            | RepositoryError::Io { .. }
            | RepositoryError::UnsupportedSchema { .. } => Self::Settings {
                message: error.to_string(),
            },
        }
    }
}

/// Brings the classification projection of `store` up to date. Returns the
/// error message on failure: the previous projection stays in use and the
/// next access tries again, so a failure never blocks the catalog.
pub fn sync_catalog(
    store: &CatalogStore,
    target: Result<ProjectionTarget, CommandError>,
) -> Option<String> {
    let result = target.and_then(|target| {
        store
            .sync_classifications(&target)
            .map_err(CommandError::from)
    });
    match result {
        Ok(SyncOutcome::NoCatalog | SyncOutcome::UpToDate | SyncOutcome::Synced(_)) => None,
        Err(
            CommandError::Settings { message }
            | CommandError::CatalogUnreadable { message }
            | CommandError::InvalidRequest { message }
            | CommandError::NotFound { message }
            | CommandError::Internal { message },
        ) => Some(message),
        Err(error) => Some(format!("{error:?}")),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewDimension {
    #[serde(flatten)]
    pub statistics: DimensionStatistics,
    /// Whether the imported source will be the active source of the dimension.
    pub active_after_import: bool,
}

/// What an import would store, before confirming it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SupportFileImportPreview {
    /// Passed back to apply this import.
    pub import_id: u64,
    pub format: SupportFileFormat,
    pub file_name: String,
    pub declared_version: Option<String>,
    pub machine_count: u64,
    pub dimensions: Vec<PreviewDimension>,
    pub warnings: Vec<ParseWarning>,
    /// The source of the same format that the import would replace.
    pub replaces: Option<SourceView>,
}

/// The classifications available in one catalog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassificationStatus {
    /// Dimensions with an active source, as projected in this catalog.
    pub dimensions: Vec<ClassifiedDimension>,
    /// Why the projection could not be brought up to date, if it could not.
    pub sync_error: Option<String>,
}

#[tauri::command]
pub async fn get_support_files(app: AppHandle) -> Result<SupportFilesView, CommandError> {
    run_blocking(move || app.state::<SupportFilesState>().view()).await?
}

/// Lets the user choose a support file and checks it; nothing is stored
/// until the import is applied. `expected_format` is set when reimporting a
/// source, so that a file of another format is refused. `None` when the user
/// cancels.
#[tauri::command]
pub async fn preview_support_file_import(
    expected_format: Option<SupportFileFormat>,
    app: AppHandle,
) -> Result<Option<SupportFileImportPreview>, CommandError> {
    run_blocking(move || {
        let title = match expected_format {
            Some(format) => format!("Reimport {format}"),
            None => "Import support file".to_owned(),
        };
        let selection = app
            .dialog()
            .file()
            .set_title(title)
            .add_filter("Support files", &["ini"])
            .blocking_pick_file();
        let Some(selection) = selection else {
            return Ok(None);
        };
        let path = selection
            .into_path()
            .map_err(|error| CommandError::Internal {
                message: format!("unsupported file selection: {error}"),
            })?;
        preview_file(&app.state::<SupportFilesState>(), &path, expected_format).map(Some)
    })
    .await?
}

/// Parses `path` (never modified) and prepares its import.
pub fn preview_file(
    state: &SupportFilesState,
    path: &Path,
    expected_format: Option<SupportFileFormat>,
) -> Result<SupportFileImportPreview, CommandError> {
    let parsed = parser::read_file(path).map_err(|error| CommandError::InvalidFile {
        message: describe_parse_error(path, &error),
    })?;
    let file_name = path
        .file_name()
        .map_or_else(PathBuf::new, PathBuf::from)
        .to_string_lossy()
        .into_owned();
    state.prepare_import(file_name, parsed, expected_format)
}

fn describe_parse_error(path: &Path, error: &ParseError) -> String {
    let name = path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    );
    format!("'{name}' cannot be imported: {error}")
}

#[tauri::command]
pub async fn apply_support_file_import(
    import_id: u64,
    app: AppHandle,
) -> Result<SupportFilesView, CommandError> {
    run_blocking(move || {
        let _change = data_change()?;
        app.state::<SupportFilesState>()
            .apply_import(import_id, now_millis())
    })
    .await?
}

/// Removes a source from the repository; the file it came from is not touched.
#[tauri::command]
pub async fn remove_support_file(
    format: SupportFileFormat,
    app: AppHandle,
) -> Result<SupportFilesView, CommandError> {
    run_blocking(move || {
        let _change = data_change()?;
        app.state::<SupportFilesState>().remove(format)
    })
    .await?
}

#[tauri::command]
pub async fn set_active_support_source(
    dimension: Dimension,
    format: Option<SupportFileFormat>,
    app: AppHandle,
) -> Result<SupportFilesView, CommandError> {
    run_blocking(move || {
        let _change = data_change()?;
        app.state::<SupportFilesState>()
            .set_active(dimension, format)
    })
    .await?
}

/// The classifications of an installation's catalog, synchronized first.
#[tauri::command]
pub async fn get_classification_status(
    installation_id: String,
    installations: State<'_, InstallationsState>,
    support: State<'_, SupportFilesState>,
) -> Result<ClassificationStatus, CommandError> {
    let store = installations.resolve(&installation_id)?.catalog;
    let target = support.target();
    run_blocking(move || {
        let sync_error = sync_catalog(&store, target);
        let dimensions = store
            .with_reader(|connection| Ok(classified_dimensions(connection)?))?
            .ok_or(CommandError::CatalogNotImported)?;
        Ok(ClassificationStatus {
            dimensions,
            sync_error,
        })
    })
    .await?
}

/// Classification values of one dimension.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DimensionValues {
    pub dimension: Dimension,
    pub values: Vec<String>,
}

/// The values that no machine of the catalog has (for example criteria of a
/// saved filter whose source was removed), per dimension; dimensions with
/// none are left out. Such values match nothing in a search.
pub fn missing_values(
    store: &CatalogStore,
    values: &[DimensionValues],
) -> Result<Vec<DimensionValues>, CommandError> {
    if let Some(entry) = values
        .iter()
        .find(|entry| entry.values.len() > MAX_FILTER_VALUES)
    {
        return Err(CommandError::InvalidRequest {
            message: format!(
                "at most {MAX_FILTER_VALUES} values can be checked for {}",
                entry.dimension.as_str()
            ),
        });
    }
    store
        .with_reader(|connection| {
            let mut missing = Vec::new();
            for entry in values {
                let values =
                    classification::missing_values(connection, entry.dimension, &entry.values)?;
                if !values.is_empty() {
                    missing.push(DimensionValues {
                        dimension: entry.dimension,
                        values,
                    });
                }
            }
            Ok(missing)
        })?
        .ok_or(CommandError::CatalogNotImported)
}

#[tauri::command]
pub async fn find_missing_classification_values(
    installation_id: String,
    values: Vec<DimensionValues>,
    installations: State<'_, InstallationsState>,
    support: State<'_, SupportFilesState>,
) -> Result<Vec<DimensionValues>, CommandError> {
    let store = installations.resolve(&installation_id)?.catalog;
    let target = support.target();
    run_blocking(move || {
        let _ = sync_catalog(&store, target);
        missing_values(&store, &values)
    })
    .await?
}

#[cfg(test)]
mod tests;
