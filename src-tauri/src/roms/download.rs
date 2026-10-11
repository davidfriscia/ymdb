//! Downloads of a plan: one file at a time, through [`RemoteRepository`].
//!
//! Each file is streamed into a temporary file next to its destination
//! (`<name>.ymdb-download`, never a name MAME looks for), synced, checked
//! against the size announced by the repository and only then given its
//! final name. The final name is created with a hard link, which fails when
//! a file already exists, so an existing file is never replaced; on file
//! systems without hard links the temporary file is renamed after checking
//! that the destination is still free. Cancelled or failed transfers remove
//! their temporary file; completed files are kept.
//!
//! A temporary file left by an interrupted run (for example when YMDB was
//! closed) has a fixed name and is overwritten by the next attempt.

use std::fs;
use std::future::Future;
use std::io;
use std::path::{Path, PathBuf};
use std::pin::pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use serde::Serialize;
use tokio::io::AsyncWriteExt;

use super::Connector;
use super::availability::breaks_connection;
use crate::remote::connections::RemoteConnection;
use crate::remote::path::RemotePath;
use crate::remote::provider::{RemoteError, RemoteRepository};
use crate::remote::transfer::copy_download;

const TEMP_SUFFIX: &str = ".ymdb-download";
/// How often a waiting operation checks for cancellation.
const CANCEL_POLL: Duration = Duration::from_millis(100);

/// One file to download.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadTask {
    pub artifact_id: String,
    pub connection: RemoteConnection,
    pub remote_path: RemotePath,
    pub destination: PathBuf,
    /// The size seen when the plan was built.
    pub expected_size: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TransferStatus {
    Pending,
    Downloading,
    /// Transferred, size checked and finalized. Says nothing about whether
    /// MAME can use the content.
    Downloaded,
    /// A file appeared at the destination meanwhile; it was kept.
    Skipped,
    Failed,
    Cancelled,
}

impl TransferStatus {
    /// Whether a later attempt has nothing left to do for the file.
    pub fn is_done(self) -> bool {
        matches!(self, Self::Downloaded | Self::Skipped)
    }
}

/// Why a transfer failed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum TransferError {
    /// The repository or the stream failed (network, authentication,
    /// permissions, or a local write error while streaming).
    Remote { error: RemoteError },
    /// The bytes received differ from the size announced.
    SizeMismatch { expected: u64, received: u64 },
    /// A local directory or file could not be created, synced or finalized.
    Local { message: String },
}

impl TransferError {
    fn local(context: &str, error: &io::Error) -> Self {
        Self::Local {
            message: format!("{context}: {error}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferState {
    pub artifact_id: String,
    pub status: TransferStatus,
    pub bytes_transferred: u64,
    pub size: Option<u64>,
    pub error: Option<TransferError>,
}

/// Transfer states shared with status readers.
pub struct Progress {
    transfers: Mutex<Vec<TransferState>>,
}

impl Progress {
    /// States for `tasks`; files completed by `previous` (an earlier attempt
    /// of the same plan) keep their state and are not downloaded again.
    pub fn new(tasks: &[DownloadTask], previous: &[TransferState]) -> Self {
        let transfers = tasks
            .iter()
            .map(|task| {
                previous
                    .iter()
                    .find(|state| state.artifact_id == task.artifact_id && state.status.is_done())
                    .cloned()
                    .unwrap_or_else(|| TransferState {
                        artifact_id: task.artifact_id.clone(),
                        status: TransferStatus::Pending,
                        bytes_transferred: 0,
                        size: task.expected_size,
                        error: None,
                    })
            })
            .collect();
        Self {
            transfers: Mutex::new(transfers),
        }
    }

    pub fn snapshot(&self) -> Vec<TransferState> {
        self.lock().clone()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<TransferState>> {
        self.transfers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    fn status(&self, index: usize) -> TransferStatus {
        self.lock()[index].status
    }

    fn update(&self, index: usize, change: impl FnOnce(&mut TransferState)) {
        change(&mut self.lock()[index]);
    }

    fn set(&self, index: usize, status: TransferStatus, error: Option<TransferError>) {
        self.update(index, |state| {
            state.status = status;
            state.error = error;
        });
    }

    /// Marks every unfinished file as failed (the downloads could not run).
    pub fn fail_unfinished(&self, error: &TransferError) {
        for state in self.lock().iter_mut() {
            if !state.status.is_done() {
                state.status = TransferStatus::Failed;
                state.error = Some(error.clone());
            }
        }
    }

    /// Marks every file not started yet as cancelled.
    fn cancel_pending(&self) {
        for state in self.lock().iter_mut() {
            if matches!(
                state.status,
                TransferStatus::Pending | TransferStatus::Downloading
            ) {
                state.status = TransferStatus::Cancelled;
            }
        }
    }
}

/// Runs `future` until it completes or `cancel` is set (`None`).
async fn unless_cancelled<F: Future>(cancel: &AtomicBool, future: F) -> Option<F::Output> {
    let mut future = pin!(future);
    loop {
        if cancel.load(Ordering::Relaxed) {
            return None;
        }
        if let Ok(output) = tokio::time::timeout(CANCEL_POLL, &mut future).await {
            return Some(output);
        }
    }
}

/// Downloads the tasks in order, reusing one connection per repository.
/// A connection that cannot be opened fails its remaining files; a broken
/// connection is reopened for the next file.
pub async fn run<C: Connector>(
    tasks: &[DownloadTask],
    connector: &C,
    progress: &Progress,
    cancel: &AtomicBool,
) {
    let mut current: Option<(String, C::Repository)> = None;
    let mut refused: Vec<(String, RemoteError)> = Vec::new();
    for (index, task) in tasks.iter().enumerate() {
        if progress.status(index).is_done() {
            continue;
        }
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let connection_id = &task.connection.id;
        if let Some((_, error)) = refused.iter().find(|(id, _)| id == connection_id) {
            let error = TransferError::Remote {
                error: error.clone(),
            };
            progress.set(index, TransferStatus::Failed, Some(error));
            continue;
        }
        if destination_exists(&task.destination) {
            progress.set(index, TransferStatus::Skipped, None);
            continue;
        }
        if current.as_ref().is_none_or(|(id, _)| id != connection_id) {
            if let Some((_, repository)) = current.take() {
                repository.close().await;
            }
            progress.set(index, TransferStatus::Downloading, None);
            match unless_cancelled(cancel, connector.connect(&task.connection)).await {
                None => break,
                Some(Ok(repository)) => current = Some((connection_id.clone(), repository)),
                Some(Err(error)) => {
                    refused.push((connection_id.clone(), error.clone()));
                    let error = TransferError::Remote { error };
                    progress.set(index, TransferStatus::Failed, Some(error));
                    continue;
                }
            }
        }
        let Some((_, repository)) = &current else {
            continue;
        };
        progress.set(index, TransferStatus::Downloading, None);
        let result = transfer(repository, task, cancel, |bytes, size| {
            progress.update(index, |state| {
                state.bytes_transferred = bytes;
                if size.is_some() {
                    state.size = size;
                }
            });
        })
        .await;
        match result {
            Ok(Finalized::Created) => progress.set(index, TransferStatus::Downloaded, None),
            Ok(Finalized::Kept) => progress.set(index, TransferStatus::Skipped, None),
            Err(Failure::Cancelled) => break,
            Err(Failure::Error(error)) => {
                let broken =
                    matches!(&error, TransferError::Remote { error } if breaks_connection(error));
                progress.set(index, TransferStatus::Failed, Some(error));
                if broken && let Some((_, repository)) = current.take() {
                    repository.close().await;
                }
            }
        }
    }
    if cancel.load(Ordering::Relaxed) {
        progress.cancel_pending();
    }
    if let Some((_, repository)) = current {
        repository.close().await;
    }
}

fn destination_exists(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

enum Finalized {
    /// The destination was created.
    Created,
    /// A file already existed at the destination and was kept.
    Kept,
}

enum Failure {
    Cancelled,
    Error(TransferError),
}

/// The temporary file of a destination: same directory, same file system.
pub fn temp_path(destination: &Path) -> PathBuf {
    let mut name = destination
        .file_name()
        .map(std::ffi::OsString::from)
        .unwrap_or_default();
    name.push(TEMP_SUFFIX);
    destination.with_file_name(name)
}

/// Downloads one file. `on_progress` receives the bytes written and the
/// size announced by the repository.
async fn transfer<R: RemoteRepository>(
    repository: &R,
    task: &DownloadTask,
    cancel: &AtomicBool,
    mut on_progress: impl FnMut(u64, Option<u64>),
) -> Result<Finalized, Failure> {
    let Some(dir) = task.destination.parent() else {
        return Err(Failure::Error(TransferError::Local {
            message: "the destination has no directory".to_owned(),
        }));
    };
    let created = create_dirs(dir).map_err(|error| {
        Failure::Error(TransferError::local(
            &format!("cannot create {}", dir.display()),
            &error,
        ))
    })?;
    let temp = temp_path(&task.destination);
    let result = write_and_finalize(repository, task, &temp, cancel, &mut on_progress).await;
    if !matches!(result, Ok(Finalized::Created)) {
        let _ = fs::remove_file(&temp);
        for dir in created.iter().rev() {
            // Only empty directories are removed.
            let _ = fs::remove_dir(dir);
        }
    }
    result
}

async fn write_and_finalize<R: RemoteRepository>(
    repository: &R,
    task: &DownloadTask,
    temp: &Path,
    cancel: &AtomicBool,
    on_progress: &mut impl FnMut(u64, Option<u64>),
) -> Result<Finalized, Failure> {
    let remote = |error: RemoteError| match error {
        RemoteError::Cancelled => Failure::Cancelled,
        error => Failure::Error(TransferError::Remote { error }),
    };
    let download = unless_cancelled(cancel, repository.open_download(&task.remote_path, 0))
        .await
        .ok_or(Failure::Cancelled)?
        .map_err(remote)?;
    let announced = download.size;
    on_progress(0, announced);
    let mut file = tokio::fs::File::create(temp).await.map_err(|error| {
        Failure::Error(TransferError::local(
            &format!("cannot create {}", temp.display()),
            &error,
        ))
    })?;
    let copied = unless_cancelled(
        cancel,
        copy_download(download, &mut file, cancel, |bytes| {
            on_progress(bytes, announced);
        }),
    )
    .await
    .ok_or(Failure::Cancelled)?
    .map_err(remote)?;
    let synced = async {
        file.flush().await?;
        file.sync_all().await
    }
    .await;
    drop(file);
    synced
        .map_err(|error| Failure::Error(TransferError::local("cannot write the file", &error)))?;

    let expected = announced.or(task.expected_size);
    let written = fs::metadata(temp).map_or(0, |metadata| metadata.len());
    if let Some(expected) = expected
        && (copied != expected || written != expected)
    {
        return Err(Failure::Error(TransferError::SizeMismatch {
            expected,
            received: written,
        }));
    }
    finalize(temp, &task.destination).map_err(|error| {
        Failure::Error(TransferError::local(
            &format!("cannot create {}", task.destination.display()),
            &error,
        ))
    })
}

/// Gives the temporary file its final name without replacing a file.
fn finalize(temp: &Path, destination: &Path) -> io::Result<Finalized> {
    match fs::hard_link(temp, destination) {
        Ok(()) => {
            let _ = fs::remove_file(temp);
            Ok(Finalized::Created)
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(Finalized::Kept),
        Err(_) if destination_exists(destination) => Ok(Finalized::Kept),
        // Hard links are not supported by this file system.
        Err(_) => fs::rename(temp, destination).map(|()| Finalized::Created),
    }
}

/// Creates `dir` and its missing parents; returns those created, outermost first.
fn create_dirs(dir: &Path) -> io::Result<Vec<PathBuf>> {
    let mut missing = Vec::new();
    let mut current = Some(dir);
    while let Some(path) = current {
        if fs::symlink_metadata(path).is_ok() {
            break;
        }
        missing.push(path.to_path_buf());
        current = path.parent();
    }
    missing.reverse();
    fs::create_dir_all(dir)?;
    Ok(missing)
}
