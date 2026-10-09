//! Detection of candidate MAME executables.
//!
//! Detection only looks for files with known MAME executable names directly
//! inside a small set of directories: the configured path, the directories
//! listed in `PATH` and conventional installation directories. It never scans
//! directories recursively. Detection never changes the saved configuration.

use std::collections::HashSet;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;

use serde::Serialize;

use super::platform;
use super::validation::{ValidationReport, validate_executable};

/// Upper bound on the number of candidates validated in one detection run.
const MAX_CANDIDATES: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CandidateSource {
    /// The executable currently saved in the settings.
    Configured,
    /// Found in a directory listed in the `PATH` environment variable.
    Path,
    /// Found in a conventional installation directory.
    Conventional,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub path: PathBuf,
    pub source: CandidateSource,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedInstallation {
    pub source: CandidateSource,
    pub report: ValidationReport,
}

/// Where to look for MAME executables.
#[derive(Debug, Clone)]
pub struct SearchLocations {
    pub configured: Option<PathBuf>,
    /// Value of the `PATH` environment variable.
    pub path_var: Option<OsString>,
    pub conventional_dirs: Vec<PathBuf>,
    pub executable_names: Vec<String>,
}

impl SearchLocations {
    pub fn for_current_platform(configured: Option<PathBuf>) -> Self {
        Self {
            configured,
            path_var: std::env::var_os("PATH"),
            conventional_dirs: platform::conventional_dirs(),
            executable_names: platform::EXECUTABLE_NAMES
                .iter()
                .map(|name| (*name).to_owned())
                .collect(),
        }
    }
}

/// Finds existing files that may be MAME executables, without running them.
///
/// Candidates are ordered by source (configured, `PATH`, conventional) and
/// deduplicated by their canonical path.
pub fn find_candidates(locations: &SearchLocations) -> Vec<Candidate> {
    let mut finder = CandidateFinder::default();

    if let Some(configured) = &locations.configured {
        finder.add(configured.clone(), CandidateSource::Configured);
    }
    if let Some(path_var) = &locations.path_var {
        // Relative PATH entries are ignored: they depend on the working directory.
        for dir in std::env::split_paths(path_var).filter(|dir| dir.is_absolute()) {
            finder.add_dir(&dir, &locations.executable_names, CandidateSource::Path);
        }
    }
    for dir in &locations.conventional_dirs {
        finder.add_dir(
            dir,
            &locations.executable_names,
            CandidateSource::Conventional,
        );
    }

    finder.candidates.truncate(MAX_CANDIDATES);
    finder.candidates
}

/// Finds candidates and validates each of them concurrently.
pub fn detect_installations(locations: &SearchLocations) -> Vec<DetectedInstallation> {
    let candidates = find_candidates(locations);
    thread::scope(|scope| {
        let handles: Vec<_> = candidates
            .iter()
            .map(|candidate| scope.spawn(|| validate_executable(&candidate.path)))
            .collect();
        candidates
            .iter()
            .zip(handles)
            .map(|(candidate, handle)| DetectedInstallation {
                source: candidate.source,
                report: handle
                    .join()
                    .unwrap_or_else(|panic| std::panic::resume_unwind(panic)),
            })
            .collect()
    })
}

#[derive(Default)]
struct CandidateFinder {
    candidates: Vec<Candidate>,
    seen: HashSet<PathBuf>,
}

impl CandidateFinder {
    fn add_dir(&mut self, dir: &Path, names: &[String], source: CandidateSource) {
        for name in names {
            self.add(dir.join(name), source);
        }
    }

    fn add(&mut self, path: PathBuf, source: CandidateSource) {
        if !path.is_file() {
            return;
        }
        let identity = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        if self.seen.insert(identity) {
            self.candidates.push(Candidate { path, source });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NAME: &str = "mame-test-binary";

    fn locations(
        configured: Option<PathBuf>,
        path_dirs: &[&Path],
        conventional_dirs: &[&Path],
    ) -> SearchLocations {
        SearchLocations {
            configured,
            path_var: Some(std::env::join_paths(path_dirs).expect("join paths")),
            conventional_dirs: conventional_dirs.iter().map(|d| d.to_path_buf()).collect(),
            executable_names: vec![NAME.to_owned()],
        }
    }

    fn create_file(dir: &Path) -> PathBuf {
        fs::create_dir_all(dir).expect("create dir");
        let path = dir.join(NAME);
        fs::write(&path, "").expect("write file");
        path
    }

    #[test]
    fn finds_candidates_in_path_and_conventional_dirs() {
        let root = tempfile::tempdir().expect("temp dir");
        let in_path = create_file(&root.path().join("bin"));
        let conventional = create_file(&root.path().join("Program Files").join("MAME"));
        let empty_dir = root.path().join("empty");
        fs::create_dir(&empty_dir).expect("create dir");

        let candidates = find_candidates(&locations(
            None,
            &[&empty_dir, in_path.parent().unwrap()],
            &[conventional.parent().unwrap()],
        ));

        assert_eq!(
            candidates,
            vec![
                Candidate {
                    path: in_path,
                    source: CandidateSource::Path
                },
                Candidate {
                    path: conventional,
                    source: CandidateSource::Conventional
                },
            ]
        );
    }

    #[test]
    fn lists_configured_executable_first_and_deduplicates() {
        let root = tempfile::tempdir().expect("temp dir");
        let configured = create_file(&root.path().join("portable mame"));
        let dir = configured.parent().unwrap();

        let candidates = find_candidates(&locations(Some(configured.clone()), &[dir], &[dir]));

        assert_eq!(
            candidates,
            vec![Candidate {
                path: configured,
                source: CandidateSource::Configured
            }]
        );
    }

    #[test]
    fn ignores_missing_files_directories_and_relative_path_entries() {
        let root = tempfile::tempdir().expect("temp dir");
        let dir_named_like_executable = root.path().join("dirs");
        fs::create_dir_all(dir_named_like_executable.join(NAME)).expect("create dir");

        let candidates = find_candidates(&locations(
            Some(root.path().join("missing").join(NAME)),
            &[Path::new("relative"), &dir_named_like_executable],
            &[&root.path().join("missing")],
        ));

        assert!(candidates.is_empty(), "{candidates:?}");
    }

    #[test]
    fn validates_each_detected_candidate() {
        let root = tempfile::tempdir().expect("temp dir");
        let candidate = create_file(&root.path().join("bin"));

        let detected = detect_installations(&locations(None, &[candidate.parent().unwrap()], &[]));

        assert_eq!(detected.len(), 1);
        assert_eq!(detected[0].source, CandidateSource::Path);
        assert_eq!(detected[0].report.executable_path, candidate);
        // An empty file without an executable extension or permission.
        assert!(!detected[0].report.is_valid());
    }
}
