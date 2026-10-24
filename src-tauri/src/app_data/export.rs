//! Export: a consistent snapshot of the application data, written to a
//! verified archive.
//!
//! 1. [`snapshot`] (with the data gate held exclusively by the caller):
//!    copies the configuration files and controller profile files byte for
//!    byte and the databases with the SQLite backup API into a work
//!    directory. This is the only step that reads the live data.
//! 2. [`write_archive`]: compresses the copies into `<destination>.partial`,
//!    with their sizes and checksums in the manifest.
//! 3. The partial archive is opened and checked like an import would, every
//!    checksum included, then renamed to the destination. A failure removes
//!    the partial file and the work directory: no incomplete archive is ever
//!    left under the chosen name, and the application data is never modified.

use std::fs::{self, File};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};

use zip::CompressionMethod;
use zip::write::{SimpleFileOptions, ZipWriter};

use super::archive::{self, Limits};
use super::manifest::{
    ARCHIVE_ROOT, FORMAT, FORMAT_VERSION, MANIFEST_NAME, Manifest, ManifestItem, Platform, Schemas,
};
use super::scope::{self, CONFIG_FILES, CONTROLLER_PROFILES_DIR, INSTALLATIONS_DIR};
use super::{AppDirs, BackupError, CopyError, Phase, Progress, Root, copy_hashed, now_ms};
use crate::catalog::store::{CATALOG_FILE_NAME, CatalogStore};
use crate::installations::is_installation_id;
use crate::support_files::repository::{REPOSITORY_FILE_NAME, SupportRepository};

/// Largest accepted interface preferences document.
pub const MAX_INTERFACE_PREFERENCES: usize = 64 * 1024;

/// A file copied by [`snapshot`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotFile {
    /// Logical path (see `scope::classify`).
    pub path: String,
    pub file: PathBuf,
}

/// Where the live data is read from.
pub struct SnapshotSources<'a> {
    pub dirs: &'a AppDirs,
    /// The store of an installation's catalog (the shared instance for
    /// registered installations, so that its file lock is respected).
    pub catalog: &'a dyn Fn(&str) -> CatalogStore,
    /// The interface preferences, as sent by the frontend.
    pub interface_preferences: Option<&'a serde_json::Value>,
}

/// Copies every file a backup contains into `work`. Missing files (data
/// created only on first use) are simply not part of the backup.
pub fn snapshot(
    sources: &SnapshotSources<'_>,
    work: &Path,
    progress: &Progress,
) -> Result<Vec<SnapshotFile>, BackupError> {
    let mut files = Vec::new();
    let config = &sources.dirs.config;
    for name in CONFIG_FILES {
        let source = config.join(name);
        if is_regular_file(&source) {
            files.push(copy_file(&source, work, &format!("config/{name}"))?);
        }
    }
    for directory in [INSTALLATIONS_DIR, CONTROLLER_PROFILES_DIR] {
        copy_tree(
            &config.join(directory),
            &format!("config/{directory}"),
            work,
            &mut files,
            progress,
        )?;
    }

    let data = &sources.dirs.data;
    for id in installation_dirs(&data.join(INSTALLATIONS_DIR))? {
        progress.check_cancel()?;
        let path = format!("data/{INSTALLATIONS_DIR}/{id}/{CATALOG_FILE_NAME}");
        let target = work.join(&path);
        create_parent(&target)?;
        let copied = (sources.catalog)(&id)
            .with_reader(|connection| {
                super::sqlite::backup(connection, &target)
                    .map_err(crate::catalog::store::StoreError::Integrity)
            })
            .map_err(|error| BackupError::new(format!("cannot copy the catalog {id}: {error}")))?;
        if copied.is_some() {
            files.push(SnapshotFile { path, file: target });
        }
    }
    if data.join(REPOSITORY_FILE_NAME).is_file() {
        let path = format!("data/{REPOSITORY_FILE_NAME}");
        let target = work.join(&path);
        create_parent(&target)?;
        SupportRepository::in_dir(data)
            .backup_to(&target)
            .map_err(|error| BackupError::new(format!("cannot copy the support files: {error}")))?;
        files.push(SnapshotFile { path, file: target });
    }

    if let Some(preferences) = sources.interface_preferences {
        let content = serde_json::to_vec_pretty(preferences)
            .map_err(|error| BackupError::new(error.to_string()))?;
        let target = work.join(scope::INTERFACE_PREFERENCES);
        write_new(&target, &content)?;
        files.push(SnapshotFile {
            path: scope::INTERFACE_PREFERENCES.to_owned(),
            file: target,
        });
    }
    Ok(files)
}

/// Writes the archive of `files` to `partial` and returns its manifest.
pub fn write_archive(
    files: &[SnapshotFile],
    partial: &Path,
    app_version: &str,
    progress: &Progress,
) -> Result<Manifest, BackupError> {
    let total = files
        .iter()
        .map(|file| fs::metadata(&file.file).map_or(0, |metadata| metadata.len()))
        .sum();
    progress.start(Phase::Writing, total);
    let output =
        File::create(partial).map_err(|error| BackupError::io("create", partial, &error))?;
    let mut zip = ZipWriter::new(BufWriter::new(output));
    let mut items = Vec::with_capacity(files.len());
    for file in files {
        let classified = scope::classify(&file.path)
            .ok_or_else(|| BackupError::new(format!("'{}' cannot be backed up", file.path)))?;
        let size = fs::metadata(&file.file)
            .map_err(|error| BackupError::io("read", &file.file, &error))?
            .len();
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .large_file(size >= u64::from(u32::MAX));
        zip.start_file(format!("{ARCHIVE_ROOT}/{}", file.path), options)
            .map_err(|error| BackupError::new(format!("cannot write the archive: {error}")))?;
        let mut source =
            File::open(&file.file).map_err(|error| BackupError::io("read", &file.file, &error))?;
        let (copied, sha256) = copy_hashed(&mut source, &mut zip, u64::MAX, progress)
            .map_err(|error| copy_error(error, &file.file, partial))?;
        items.push(ManifestItem {
            path: file.path.clone(),
            category: classified.category,
            kind: classified.kind,
            size: copied,
            sha256,
        });
    }
    let manifest = Manifest {
        format: FORMAT.to_owned(),
        format_version: FORMAT_VERSION,
        app_version: app_version.to_owned(),
        created_at: now_ms(),
        platform: Platform::current(),
        schemas: Schemas::current(),
        items,
    };
    let content = serde_json::to_vec_pretty(&manifest)
        .map_err(|error| BackupError::new(error.to_string()))?;
    zip.start_file(
        format!("{ARCHIVE_ROOT}/{MANIFEST_NAME}"),
        SimpleFileOptions::default().compression_method(CompressionMethod::Deflated),
    )
    .and_then(|()| zip.write_all(&content).map_err(Into::into))
    .map_err(|error| BackupError::new(format!("cannot write the archive: {error}")))?;
    let output = zip
        .finish()
        .map_err(|error| BackupError::new(format!("cannot write the archive: {error}")))?
        .into_inner()
        .map_err(|error| BackupError::io("write", partial, error.error()))?;
    output
        .sync_all()
        .map_err(|error| BackupError::io("write", partial, &error))?;
    Ok(manifest)
}

fn copy_error(error: CopyError, source: &Path, partial: &Path) -> BackupError {
    match error {
        CopyError::Cancelled => BackupError::cancelled(),
        CopyError::Read(error) => BackupError::io("read", source, &error),
        CopyError::Write(error) => BackupError::io("write", partial, &error),
        CopyError::TooLarge => BackupError::new("unexpected file size"),
    }
}

/// The complete export once the snapshot exists: archive, verification,
/// final rename. Returns the archive size. The partial file is removed on
/// failure; `work` is left to the caller.
pub fn finish(
    files: &[SnapshotFile],
    destination: &Path,
    app_version: &str,
    progress: &Progress,
) -> Result<u64, BackupError> {
    let partial = partial_path(destination);
    let result = write_archive(files, &partial, app_version, progress).and_then(|manifest| {
        progress.start(Phase::Verifying, manifest.total_size());
        let mut checked = archive::open(&partial, &Limits::default())?;
        if checked.manifest != manifest {
            return Err(BackupError::new("the archive does not match its manifest"));
        }
        checked.verify_all(progress)?;
        drop(checked);
        progress.check_cancel()?;
        fs::rename(&partial, destination)
            .map_err(|error| BackupError::io("write", destination, &error))?;
        fs::metadata(destination)
            .map(|metadata| metadata.len())
            .map_err(|error| BackupError::io("read", destination, &error))
    });
    if result.is_err() {
        let _ = fs::remove_file(&partial);
    }
    result
}

pub fn partial_path(destination: &Path) -> PathBuf {
    let mut name = destination.file_name().unwrap_or_default().to_os_string();
    name.push(".partial");
    destination.with_file_name(name)
}

/// A new, empty work directory for an export below the data root.
pub fn work_dir(dirs: &AppDirs) -> Result<PathBuf, BackupError> {
    let work = dirs
        .root(Root::Data)
        .join(scope::EXPORT_WORK_DIR)
        .join(uuid::Uuid::new_v4().simple().to_string());
    fs::create_dir_all(&work).map_err(|error| BackupError::io("create", &work, &error))?;
    Ok(work)
}

/// Removes every export work directory (left by this or a previous run).
pub fn remove_work_dirs(dirs: &AppDirs) {
    let _ = fs::remove_dir_all(dirs.root(Root::Data).join(scope::EXPORT_WORK_DIR));
}

/// Ids of the per-installation directories (registered or not: a backup
/// keeps catalogs of removed installations as they are on disk).
pub fn installation_dirs(dir: &Path) -> Result<Vec<String>, BackupError> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(BackupError::io("read", dir, &error)),
    };
    let mut ids: Vec<String> = entries
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| is_installation_id(name))
        .collect();
    ids.sort();
    Ok(ids)
}

fn is_regular_file(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_file())
}

fn copy_file(source: &Path, work: &Path, path: &str) -> Result<SnapshotFile, BackupError> {
    let target = work.join(path);
    create_parent(&target)?;
    fs::copy(source, &target).map_err(|error| BackupError::io("copy", source, &error))?;
    Ok(SnapshotFile {
        path: path.to_owned(),
        file: target,
    })
}

/// Copies the regular files of a directory tree. Symbolic links are not
/// followed; names YMDB could not restore on every platform are refused.
fn copy_tree(
    source: &Path,
    path: &str,
    work: &Path,
    files: &mut Vec<SnapshotFile>,
    progress: &Progress,
) -> Result<(), BackupError> {
    let entries = match fs::read_dir(source) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(BackupError::io("read", source, &error)),
    };
    let mut entries: Vec<_> = entries
        .collect::<Result<_, _>>()
        .map_err(|error| BackupError::io("read", source, &error))?;
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        progress.check_cancel()?;
        let kind = entry
            .file_type()
            .map_err(|error| BackupError::io("read", &entry.path(), &error))?;
        if kind.is_symlink() {
            continue;
        }
        let name = entry.file_name();
        let name = name
            .to_str()
            .filter(|name| scope::is_safe_component(name))
            .ok_or_else(|| {
                BackupError::new(format!(
                    "'{}' has a name that cannot be backed up; rename or remove it",
                    entry.path().display()
                ))
            })?;
        let child = format!("{path}/{name}");
        if kind.is_dir() {
            copy_tree(&entry.path(), &child, work, files, progress)?;
        } else if kind.is_file() && !scope::is_temporary_file(name) {
            if scope::classify(&child).is_none() {
                return Err(BackupError::new(format!(
                    "'{}' cannot be backed up: unexpected location",
                    entry.path().display()
                )));
            }
            files.push(copy_file(&entry.path(), work, &child)?);
        }
    }
    Ok(())
}

fn create_parent(path: &Path) -> Result<(), BackupError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| BackupError::io("create", parent, &error))?;
    }
    Ok(())
}

fn write_new(path: &Path, content: &[u8]) -> Result<(), BackupError> {
    create_parent(path)?;
    fs::write(path, content).map_err(|error| BackupError::io("write", path, &error))
}

/// Whether the interface preferences sent by the frontend are acceptable.
pub fn check_interface_preferences(value: &serde_json::Value) -> Result<(), BackupError> {
    let size = serde_json::to_vec(value).map_or(usize::MAX, |content| content.len());
    if !value.is_object() || size > MAX_INTERFACE_PREFERENCES {
        return Err(BackupError::new("the interface preferences are invalid"));
    }
    Ok(())
}
