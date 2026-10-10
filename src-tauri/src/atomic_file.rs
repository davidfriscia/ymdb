//! Atomic replacement of small files such as `settings.json` and `launch.json`.
//!
//! The new content is written to a temporary file in the same directory,
//! flushed to disk, then renamed over the target, so readers never see a
//! partially written file.

use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Writes `content` to `path` atomically, creating the directory if needed.
pub fn write(path: &Path, content: &[u8]) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let temp_file = temp_path(path);
    let result = write_synced(&temp_file, content).and_then(|()| fs::rename(&temp_file, path));
    if result.is_err() {
        // Best effort: never leave a partial temporary file behind.
        let _ = fs::remove_file(&temp_file);
    }
    result
}

/// `<file name>.tmp` in the same directory.
fn temp_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().map(OsString::from).unwrap_or_default();
    name.push(".tmp");
    path.with_file_name(name)
}

fn write_synced(path: &Path, content: &[u8]) -> io::Result<()> {
    let mut file = File::create(path)?;
    file.write_all(content)?;
    file.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_content_and_leaves_no_temporary_file() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("nested dir ✓").join("file.json");

        write(&path, b"first").expect("first write");
        write(&path, b"second").expect("second write");

        assert_eq!(fs::read(&path).expect("read"), b"second");
        let entries: Vec<_> = fs::read_dir(path.parent().expect("parent"))
            .expect("read dir")
            .map(|entry| entry.expect("entry").file_name())
            .collect();
        assert_eq!(entries, vec!["file.json"]);
    }

    #[test]
    fn temporary_file_sits_next_to_the_target() {
        assert_eq!(
            temp_path(Path::new("/config/settings.json")),
            PathBuf::from("/config/settings.json.tmp")
        );
    }
}
