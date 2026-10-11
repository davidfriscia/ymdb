//! Remote paths.
//!
//! Remote paths follow the POSIX semantics of the remote side (`/` separator,
//! case-sensitive names), whatever the local operating system: they are never
//! `std::path::PathBuf`. Two types keep them apart:
//!
//! - [`RemotePath`]: an absolute, normalized path, such as the base directory
//!   of a source (`/volume1/mame/roms`).
//! - [`RelativePath`]: a path below a base, such as the file the ROM manager
//!   looks for (`pacman.zip`, `neogeo/kof98.chd`). It cannot contain `..`, so
//!   [`RemotePath::join`] never leaves the base directory.
//!
//! Symbolic links on the server are resolved by the server; YMDB cannot
//! prevent a link inside the base directory from pointing elsewhere.

use std::fmt;

/// Longest accepted path, in bytes.
pub const MAX_PATH_LENGTH: usize = 1024;

/// An absolute, normalized remote path: it starts with `/`, has no empty,
/// `.` or `..` segments and no trailing `/` (except the root itself).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RemotePath(String);

/// A path below a [`RemotePath`]: one or more plain segments.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RelativePath(Vec<String>);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathError(pub String);

impl fmt::Display for PathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for PathError {}

fn error(message: impl Into<String>) -> PathError {
    PathError(message.into())
}

/// Checks one path segment (a file or directory name).
pub fn check_segment(segment: &str) -> Result<(), PathError> {
    if segment.is_empty() {
        return Err(error("a remote path cannot contain empty names"));
    }
    if segment == "." || segment == ".." {
        return Err(error(
            "a remote path cannot contain '.' or '..': use the full path instead",
        ));
    }
    if segment.contains('/') {
        return Err(error(format!("'{segment}' is not a single name")));
    }
    if segment.contains('\\') {
        return Err(error(
            "remote paths use '/' as separator, not '\\' (for example /volume1/mame/roms)",
        ));
    }
    if segment.chars().any(char::is_control) {
        return Err(error("a remote path cannot contain control characters"));
    }
    Ok(())
}

impl RemotePath {
    /// The root directory, `/`.
    pub fn root() -> Self {
        Self("/".to_owned())
    }

    /// Parses an absolute path typed by the user. Repeated and trailing `/`
    /// and `.` segments are removed; `..` is refused rather than resolved, so
    /// a configured path always means what it says.
    pub fn parse(text: &str) -> Result<Self, PathError> {
        let text = text.trim();
        if text.is_empty() {
            return Err(error("the remote path is empty"));
        }
        if text.len() > MAX_PATH_LENGTH {
            return Err(error(format!(
                "a remote path has at most {MAX_PATH_LENGTH} characters"
            )));
        }
        if !text.starts_with('/') {
            return Err(error(
                "the remote path must be absolute and start with '/' (for example /volume1/mame/roms)",
            ));
        }
        let mut normalized = String::with_capacity(text.len());
        for segment in text.split('/').filter(|s| !s.is_empty() && *s != ".") {
            check_segment(segment)?;
            normalized.push('/');
            normalized.push_str(segment);
        }
        if normalized.is_empty() {
            normalized.push('/');
        }
        Ok(Self(normalized))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_root(&self) -> bool {
        self.0 == "/"
    }

    /// The path of `relative` inside this directory. Never outside of it.
    #[must_use]
    pub fn join(&self, relative: &RelativePath) -> Self {
        let mut path = self.0.clone();
        for segment in &relative.0 {
            if !path.ends_with('/') {
                path.push('/');
            }
            path.push_str(segment);
        }
        Self(path)
    }

    /// The parent directory; `None` for the root.
    pub fn parent(&self) -> Option<Self> {
        if self.is_root() {
            return None;
        }
        let index = self.0.rfind('/').unwrap_or(0);
        Some(if index == 0 {
            Self::root()
        } else {
            Self(self.0[..index].to_owned())
        })
    }

    /// Whether `self` is `base` or inside it.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "part of the contract, not used by the ROM manager yet"
        )
    )]
    pub fn starts_with(&self, base: &RemotePath) -> bool {
        base.is_root()
            || self.0 == base.0
            || (self.0.starts_with(&base.0) && self.0.as_bytes().get(base.0.len()) == Some(&b'/'))
    }
}

impl fmt::Display for RemotePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "part of the contract, not used by the ROM manager yet"
    )
)]
impl RelativePath {
    /// Parses a relative path such as `neogeo/kof98.chd`. Absolute paths,
    /// empty segments, `.` and `..` are refused.
    pub fn parse(text: &str) -> Result<Self, PathError> {
        if text.is_empty() {
            return Err(error("the relative path is empty"));
        }
        if text.len() > MAX_PATH_LENGTH {
            return Err(error(format!(
                "a remote path has at most {MAX_PATH_LENGTH} characters"
            )));
        }
        if text.starts_with('/') {
            return Err(error(format!("'{text}' is not a relative path")));
        }
        let segments = text
            .split('/')
            .map(|segment| check_segment(segment).map(|()| segment.to_owned()))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self(segments))
    }

    /// A relative path made of segments that are each checked.
    pub fn from_segments<S: AsRef<str>>(segments: &[S]) -> Result<Self, PathError> {
        if segments.is_empty() {
            return Err(error("the relative path is empty"));
        }
        segments
            .iter()
            .map(|segment| check_segment(segment.as_ref()).map(|()| segment.as_ref().to_owned()))
            .collect::<Result<Vec<_>, _>>()
            .map(Self)
    }
}

impl fmt::Display for RelativePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0.join("/"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(text: &str) -> RemotePath {
        RemotePath::parse(text).expect(text)
    }

    #[test]
    fn normalizes_absolute_paths() {
        assert_eq!(path("/volume1/mame/roms").as_str(), "/volume1/mame/roms");
        assert_eq!(
            path("  //volume1//mame/./roms/ ").as_str(),
            "/volume1/mame/roms"
        );
        assert_eq!(path("/").as_str(), "/");
        assert_eq!(path("//./").as_str(), "/");
        assert_eq!(path("/MAME 0.280/Rôms ✓").as_str(), "/MAME 0.280/Rôms ✓");
    }

    #[test]
    fn rejects_relative_traversing_and_windows_paths() {
        for text in [
            "",
            "   ",
            "volume1/mame",
            "~/mame",
            "/volume1/../etc",
            "/volume1/mame/..",
            "C:\\MAME\\roms",
            "/volume1\\mame",
            "/vol\u{0}ume",
            "/a\nb",
        ] {
            assert!(RemotePath::parse(text).is_err(), "{text:?}");
        }
        assert!(RemotePath::parse(&format!("/{}", "a".repeat(MAX_PATH_LENGTH))).is_err());
    }

    #[test]
    fn joins_relative_paths_inside_the_base() {
        let base = path("/volume1/mame/chds");
        let relative = RelativePath::parse("kof98/kof98.chd").expect("relative");
        let joined = base.join(&relative);
        assert_eq!(joined.as_str(), "/volume1/mame/chds/kof98/kof98.chd");
        assert!(joined.starts_with(&base));
        assert_eq!(
            RemotePath::root()
                .join(&RelativePath::parse("pacman.zip").expect("relative"))
                .as_str(),
            "/pacman.zip"
        );
    }

    #[test]
    fn relative_paths_cannot_escape_the_base() {
        for text in [
            "",
            "/etc/passwd",
            "../roms/pacman.zip",
            "roms/../../etc",
            "./pacman.zip",
            "roms//pacman.zip",
            "roms/",
            "roms\\pacman.zip",
            "pac\u{7}man.zip",
        ] {
            assert!(RelativePath::parse(text).is_err(), "{text:?}");
        }
        assert!(RelativePath::from_segments(&["..", "x"]).is_err());
        assert!(RelativePath::from_segments(&["a/b"]).is_err());
        assert!(RelativePath::from_segments::<&str>(&[]).is_err());
        assert_eq!(
            RelativePath::from_segments(&["neogeo", "kof98.chd"])
                .expect("segments")
                .to_string(),
            "neogeo/kof98.chd"
        );
    }

    #[test]
    fn navigates_to_parents() {
        let base = path("/volume1/mame");
        assert_eq!(base.parent(), Some(path("/volume1")));
        assert_eq!(path("/volume1").parent(), Some(RemotePath::root()));
        assert_eq!(RemotePath::root().parent(), None);
        assert!(RemotePath::root().is_root());
        assert!(!base.is_root());
    }

    #[test]
    fn prefix_checks_respect_segment_boundaries() {
        let base = path("/volume1/mame");
        assert!(path("/volume1/mame").starts_with(&base));
        assert!(path("/volume1/mame/roms").starts_with(&base));
        assert!(!path("/volume1/mame2/roms").starts_with(&base));
        assert!(!path("/volume1").starts_with(&base));
        assert!(path("/anything").starts_with(&RemotePath::root()));
    }
}
