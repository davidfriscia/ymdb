//! Reading MAME's own search paths from `mame.ini`.
//!
//! YMDB never writes `mame.ini`. It only reads the `rompath` and
//! `samplepath` values, without running MAME, to know which directories MAME
//! searches by default: they are offered as download destinations and kept
//! when YMDB has to add a directory to a search path at launch.

use std::fs;
use std::path::{Path, PathBuf};

use super::platform;

/// MAME's built-in defaults, relative to its working directory.
pub const DEFAULT_ROMPATH: &str = "roms";
pub const DEFAULT_SAMPLEPATH: &str = "samples";

/// `mame.ini` files larger than this are ignored (the real file is ~20 KB).
const MAX_INI_SIZE: u64 = 1024 * 1024;

/// The search paths MAME uses without command line options.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeSearchPaths {
    /// Entries as written (relative ones are relative to the MAME directory).
    pub rompath: Vec<String>,
    pub samplepath: Vec<String>,
    /// The `mame.ini` read, if one was found.
    pub ini_file: Option<PathBuf>,
}

impl Default for NativeSearchPaths {
    fn default() -> Self {
        Self {
            rompath: vec![DEFAULT_ROMPATH.to_owned()],
            samplepath: vec![DEFAULT_SAMPLEPATH.to_owned()],
            ini_file: None,
        }
    }
}

/// Reads the search paths of the MAME installation in `mame_dir`: the first
/// `mame.ini` found in MAME's `inipath`, else MAME's defaults.
pub fn read_search_paths(mame_dir: &Path) -> NativeSearchPaths {
    for dir in platform::ini_search_dirs(mame_dir) {
        let file = dir.join("mame.ini");
        let Ok(metadata) = fs::metadata(&file) else {
            continue;
        };
        if !metadata.is_file() || metadata.len() > MAX_INI_SIZE {
            continue;
        }
        let Ok(content) = fs::read(&file) else {
            continue;
        };
        let mut paths = parse(&String::from_utf8_lossy(&content));
        paths.ini_file = Some(file);
        return paths;
    }
    NativeSearchPaths::default()
}

/// Parses the `rompath` and `samplepath` lines of an INI file. MAME's format
/// is `<name> <value>` per line, `#` starts a comment and values containing
/// spaces may be quoted. The last occurrence wins, as in MAME.
fn parse(content: &str) -> NativeSearchPaths {
    let mut paths = NativeSearchPaths::default();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('<') {
            continue;
        }
        let Some((name, value)) = line.split_once(char::is_whitespace) else {
            continue;
        };
        let target = match name {
            "rompath" => &mut paths.rompath,
            "samplepath" => &mut paths.samplepath,
            _ => continue,
        };
        let value = value.trim();
        let value = value
            .strip_prefix('"')
            .and_then(|value| value.strip_suffix('"'))
            .unwrap_or(value);
        *target = split_path_list(value);
    }
    paths
}

/// Splits a `;`-separated search path, dropping empty entries.
pub fn split_path_list(value: &str) -> Vec<String> {
    value
        .split(';')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(str::to_owned)
        .collect()
}

/// The directory an entry of a search path names: relative entries are
/// relative to the MAME directory, where YMDB runs MAME.
pub fn resolve_entry(mame_dir: &Path, entry: &str) -> PathBuf {
    let path = Path::new(entry);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        mame_dir.join(path)
    }
}

/// Whether two search path entries name the same directory.
pub fn same_directory(mame_dir: &Path, a: &str, b: &str) -> bool {
    a == b || platform::same_path(&resolve_entry(mame_dir, a), &resolve_entry(mame_dir, b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_search_paths_like_mame() {
        let paths = parse(
            "#\n# CORE SEARCH PATH OPTIONS\n#\nhomepath                  .\n\
             rompath                   roms;D:\\MAME ROMs ✓;;\n\
             hashpath                  hash\n\
             samplepath                \"D:\\Sample Sets\"\n\
             rompathextra              ignored\n",
        );
        assert_eq!(paths.rompath, ["roms", "D:\\MAME ROMs ✓"]);
        assert_eq!(paths.samplepath, ["D:\\Sample Sets"]);
        // Missing lines keep MAME's defaults; the last occurrence wins.
        let paths = parse("rompath a\nrompath b;c\n");
        assert_eq!(paths.rompath, ["b", "c"]);
        assert_eq!(paths.samplepath, [DEFAULT_SAMPLEPATH]);
    }

    #[test]
    fn reads_the_ini_of_the_mame_directory_or_defaults() {
        let dir = tempfile::tempdir().expect("temp dir");
        let mame_dir = dir.path().join("MAME 0.280 ✓");
        fs::create_dir_all(mame_dir.join("ini")).expect("dirs");
        // A home `.mame` directory could exist on Unix test machines; only
        // check what is under the temporary MAME directory.
        let paths = read_search_paths(&mame_dir);
        if paths.ini_file.is_none() {
            assert_eq!(paths, NativeSearchPaths::default());
        }

        fs::write(mame_dir.join("ini").join("mame.ini"), "rompath from_ini\n").expect("write");
        fs::write(mame_dir.join("mame.ini"), "rompath roms;bios\n").expect("write");
        let paths = read_search_paths(&mame_dir);
        if paths.ini_file.as_deref() == Some(&mame_dir.join("mame.ini")) {
            // `.` comes before `ini` in MAME's inipath.
            assert_eq!(paths.rompath, ["roms", "bios"]);
            assert_eq!(paths.samplepath, [DEFAULT_SAMPLEPATH]);
        }
    }

    #[test]
    fn compares_entries_relative_to_the_mame_directory() {
        let dir = tempfile::tempdir().expect("temp dir");
        let mame_dir = dir.path();
        fs::create_dir(mame_dir.join("roms")).expect("dir");
        let absolute = mame_dir.join("roms").to_string_lossy().into_owned();
        assert!(same_directory(mame_dir, "roms", &absolute));
        assert!(same_directory(mame_dir, "roms", "./roms"));
        assert!(!same_directory(mame_dir, "roms", "samples"));
        assert_eq!(resolve_entry(mame_dir, "roms"), mame_dir.join("roms"));
        assert_eq!(split_path_list(" a ; ;b"), ["a", "b"]);
    }
}
