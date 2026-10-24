//! Application data management: the inventory of the files YMDB uses, and a
//! full backup (export) and restore (import) of the data it owns.
//!
//! YMDB keeps its data in two roots: the configuration directory (small
//! files entered by the user) and the local data directory (catalogs and
//! other large or machine-specific files). A backup is one ZIP archive
//! (`.ymdb-backup.zip`) with a versioned manifest ([`manifest`]); its entries
//! are logical paths below these roots, never absolute paths.
//!
//! - [`scope`]: which files a backup contains and which a restore replaces.
//! - [`gate`]: the shared lock that commands changing application data hold,
//!   so that an export takes a consistent snapshot of every file at once.
//! - [`export`]: snapshot (JSON copies, SQLite backup API) and archive.
//! - [`archive`]: reading and checking an archive (paths, sizes, checksums,
//!   symlinks, decompression limits).
//! - [`restore`]: staging, verification and preview of an import, then the
//!   replacement at the next start with a rollback copy and a transaction file.
//! - [`inventory`]: what the Application Data settings section shows.
//!
//! Files outside YMDB's roots (MAME installations, ROMs, CHDs, media
//! directories, private keys) are only referenced: they are never copied into
//! a backup nor written by a restore.

pub mod archive;
pub mod export;
pub mod gate;
pub mod inventory;
pub mod manifest;
pub mod restore;
pub mod scope;
mod sqlite;

pub use sqlite::backup as sqlite_backup;

#[cfg(test)]
mod tests;

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

/// The two directories holding YMDB's own data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppDirs {
    pub config: PathBuf,
    pub data: PathBuf,
}

impl AppDirs {
    pub fn new(config: &Path, data: &Path) -> Self {
        Self {
            config: config.to_path_buf(),
            data: data.to_path_buf(),
        }
    }

    pub fn root(&self, root: Root) -> &Path {
        match root {
            Root::Config => &self.config,
            Root::Data => &self.data,
        }
    }
}

/// One of the two data roots; also the first component of logical paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Root {
    Config,
    Data,
}

impl Root {
    pub const ALL: [Self; 2] = [Self::Config, Self::Data];

    pub fn name(self) -> &'static str {
        match self {
            Self::Config => "config",
            Self::Data => "data",
        }
    }
}

/// A failed backup operation. The message is shown to the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupError(pub String);

impl BackupError {
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }

    pub fn io(action: &str, path: &Path, error: &std::io::Error) -> Self {
        Self(format!("failed to {action} '{}': {error}", path.display()))
    }

    pub fn cancelled() -> Self {
        Self::new(CANCELLED)
    }

    pub fn is_cancelled(&self) -> bool {
        self.0 == CANCELLED
    }
}

const CANCELLED: &str = "the operation was cancelled";

impl fmt::Display for BackupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for BackupError {}

/// Phase of a running export or import check, polled by the frontend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Phase {
    /// Waiting for running changes to finish, then copying the data.
    Snapshot,
    Writing,
    Verifying,
    Extracting,
    Checking,
}

/// Cheap progress shared with the thread of an export or import check.
#[derive(Debug)]
pub struct Progress {
    phase: Mutex<Phase>,
    done: AtomicU64,
    total: AtomicU64,
    cancel: AtomicBool,
}

impl Progress {
    pub fn new(phase: Phase) -> Self {
        Self {
            phase: Mutex::new(phase),
            done: AtomicU64::new(0),
            total: AtomicU64::new(0),
            cancel: AtomicBool::new(false),
        }
    }

    pub fn start(&self, phase: Phase, total: u64) {
        *self
            .phase
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = phase;
        self.done.store(0, Ordering::Relaxed);
        self.total.store(total, Ordering::Relaxed);
    }

    pub fn advance(&self, bytes: u64) {
        self.done.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn phase(&self) -> Phase {
        *self
            .phase
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub fn done(&self) -> u64 {
        self.done.load(Ordering::Relaxed)
    }

    pub fn total(&self) -> u64 {
        self.total.load(Ordering::Relaxed)
    }

    pub fn request_cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    pub fn is_cancel_requested(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    pub fn check_cancel(&self) -> Result<(), BackupError> {
        if self.is_cancel_requested() {
            Err(BackupError::cancelled())
        } else {
            Ok(())
        }
    }
}

/// Milliseconds since the Unix epoch.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| {
            u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
        })
}

/// Copies a stream in chunks, hashing it and stopping as soon as more than
/// `limit` bytes arrive or the operation is cancelled. Returns the number
/// of bytes and the SHA-256 digest (lowercase hexadecimal).
pub fn copy_hashed(
    reader: &mut impl std::io::Read,
    writer: &mut impl std::io::Write,
    limit: u64,
    progress: &Progress,
) -> Result<(u64, String), CopyError> {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 256 * 1024];
    let mut copied: u64 = 0;
    loop {
        if progress.is_cancel_requested() {
            return Err(CopyError::Cancelled);
        }
        let read = match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => read,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(CopyError::Read(error)),
        };
        copied += read as u64;
        if copied > limit {
            return Err(CopyError::TooLarge);
        }
        hasher.update(&buffer[..read]);
        writer
            .write_all(&buffer[..read])
            .map_err(CopyError::Write)?;
        progress.advance(read as u64);
    }
    Ok((copied, hex(&hasher.finalize())))
}

#[derive(Debug)]
pub enum CopyError {
    Read(std::io::Error),
    Write(std::io::Error),
    /// More bytes than declared.
    TooLarge,
    Cancelled,
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::new(), |mut text, byte| {
        let _ = write!(text, "{byte:02x}");
        text
    })
}
