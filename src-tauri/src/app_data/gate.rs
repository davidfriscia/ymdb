//! The data gate: a process-wide reader-writer lock that makes an export a
//! consistent snapshot of every file at once.
//!
//! Every command that changes application data (configuration files,
//! controller profile files, catalogs, the support files repository) holds
//! the gate shared for the whole change, and so do the commands that start a
//! catalog import, a download or a MAME session. An export holds it
//! exclusively while it checks that none of those is running and copies the
//! data: a change in progress is finished first, and changes requested
//! meanwhile are refused (`CommandError::DataBusy`) instead of blocking a
//! thread for the length of the copy. A restore holds it exclusively from
//! its final checks until the application restarts.
//!
//! The gate is not reentrant: code holding it must never take it again.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard, TryLockError};

static GATE: RwLock<()> = RwLock::new(());
/// Exclusive requests waiting: new changes are refused meanwhile, so that a
/// stream of changes cannot delay an export indefinitely.
static EXCLUSIVE_WAITING: AtomicUsize = AtomicUsize::new(0);

/// Held while a command changes application data; `None` while an export
/// or a restore holds the gate.
pub fn shared() -> Option<RwLockReadGuard<'static, ()>> {
    if EXCLUSIVE_WAITING.load(Ordering::SeqCst) > 0 {
        return None;
    }
    match GATE.try_read() {
        Ok(guard) => Some(guard),
        Err(TryLockError::Poisoned(error)) => Some(error.into_inner()),
        Err(TryLockError::WouldBlock) => None,
    }
}

/// Held while application data is copied or about to be replaced. Waits
/// for the changes in progress.
pub fn exclusive() -> RwLockWriteGuard<'static, ()> {
    EXCLUSIVE_WAITING.fetch_add(1, Ordering::SeqCst);
    let guard = GATE.write().unwrap_or_else(PoisonError::into_inner);
    EXCLUSIVE_WAITING.fetch_sub(1, Ordering::SeqCst);
    guard
}
