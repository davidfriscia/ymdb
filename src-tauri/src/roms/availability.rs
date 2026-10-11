//! Remote availability of missing files.
//!
//! Each file is looked up with one `stat` on its exact path: repositories are
//! never listed, so providers without directory listings work the same way.
//! Only "not found" means that a file is missing; any other failure leaves
//! its availability unresolved, because a timeout or a refused login proves
//! nothing about the file.

use crate::remote::path::RemotePath;
use crate::remote::provider::{EntryKind, RemoteError, RemoteRepository};

/// The outcome of one remote lookup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteCheck {
    Available { size: Option<u64> },
    Missing,
    Unresolved(RemoteError),
}

/// Looks up `paths` in order. After a failure of the connection itself, the
/// remaining paths are unresolved with the same error instead of waiting for
/// more time limits.
pub async fn check_paths<R: RemoteRepository>(
    repository: &R,
    paths: &[RemotePath],
) -> Vec<RemoteCheck> {
    let mut results = Vec::with_capacity(paths.len());
    let mut broken: Option<RemoteError> = None;
    for path in paths {
        if let Some(error) = &broken {
            results.push(RemoteCheck::Unresolved(error.clone()));
            continue;
        }
        let result = match repository.stat(path).await {
            Ok(metadata) if matches!(metadata.kind, EntryKind::File | EntryKind::Symlink) => {
                RemoteCheck::Available {
                    size: metadata.size,
                }
            }
            Ok(_) => RemoteCheck::Unresolved(RemoteError::Protocol {
                message: format!("'{path}' is not a file"),
            }),
            Err(RemoteError::NotFound { .. }) => RemoteCheck::Missing,
            Err(error) => {
                if breaks_connection(&error) {
                    broken = Some(error.clone());
                }
                RemoteCheck::Unresolved(error)
            }
        };
        results.push(result);
    }
    results
}

/// Errors after which the connection cannot be used any more.
pub fn breaks_connection(error: &RemoteError) -> bool {
    matches!(
        error,
        RemoteError::ConnectionLost | RemoteError::TimedOut | RemoteError::HostUnreachable { .. }
    )
}
