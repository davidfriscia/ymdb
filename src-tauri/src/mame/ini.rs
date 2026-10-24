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
    for (name, value) in entries(content) {
        let target = match name {
            "rompath" => &mut paths.rompath,
            "samplepath" => &mut paths.samplepath,
            _ => continue,
        };
        *target = split_path_list(value);
    }
    paths
}

/// The `<name> <value>` lines of an INI file, values unquoted. Lines without
/// a value (an empty option) are left out.
fn entries(content: &str) -> impl Iterator<Item = (&str, &str)> {
    content.lines().filter_map(|line| {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('<') {
            return None;
        }
        let (name, value) = line.split_once(char::is_whitespace)?;
        let value = value.trim();
        let value = value
            .strip_prefix('"')
            .and_then(|value| value.strip_suffix('"'))
            .unwrap_or(value);
        Some((name, value))
    })
}

/// MAME's default `ctrlrpath`, relative to its working directory.
pub const DEFAULT_CTRLRPATH: &str = "ctrlr";
/// INI files scanned at most for input settings, per directory.
const MAX_INI_FILES: usize = 10_000;

/// MAME's own controller file and joystick provider settings, which a
/// launch that maps devices must take into account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeInputSettings {
    /// The `mame.ini` read, if one was found.
    pub ini_file: Option<PathBuf>,
    /// The `ctrlr` of `mame.ini`; `None` when empty or not set.
    pub ctrlr: Option<String>,
    /// The `ctrlrpath` of `mame.ini`, else MAME's default.
    pub ctrlrpath: Vec<String>,
    /// The `joystickprovider` of `mame.ini`, if set.
    pub joystickprovider: Option<String>,
    /// Other INI files of MAME's inipath that set a `ctrlr`. MAME applies
    /// them by machine, parent, source file, screen type... after `mame.ini`.
    pub other_ctrlr: Vec<PathBuf>,
}

impl Default for NativeInputSettings {
    fn default() -> Self {
        Self {
            ini_file: None,
            ctrlr: None,
            ctrlrpath: vec![DEFAULT_CTRLRPATH.to_owned()],
            joystickprovider: None,
            other_ctrlr: Vec::new(),
        }
    }
}

/// Reads the input settings of the MAME installation in `mame_dir`: those of
/// the first `mame.ini` of MAME's inipath, and the other INI files of the
/// inipath (and their `source` subdirectories) that set a controller file.
pub fn read_input_settings(mame_dir: &Path) -> NativeInputSettings {
    let mut settings = NativeInputSettings::default();
    for dir in platform::ini_search_dirs(mame_dir) {
        for file in ini_files(&dir)
            .into_iter()
            .chain(ini_files(&dir.join("source")))
        {
            let is_mame_ini = file
                .file_name()
                .is_some_and(|name| name.eq_ignore_ascii_case("mame.ini"));
            if is_mame_ini && file.parent() != Some(dir.as_path()) {
                continue;
            }
            // MAME reads only the first `mame.ini` of its inipath.
            if is_mame_ini && settings.ini_file.is_some() {
                continue;
            }
            let Some(content) = read_small(&file) else {
                continue;
            };
            if is_mame_ini {
                for (name, value) in entries(&content) {
                    match name {
                        "ctrlr" => settings.ctrlr = Some(value.to_owned()),
                        "ctrlrpath" => settings.ctrlrpath = split_path_list(value),
                        "joystickprovider" => {
                            settings.joystickprovider = Some(value.to_owned());
                        }
                        _ => {}
                    }
                }
                settings.ctrlr = settings.ctrlr.filter(|ctrlr| !ctrlr.is_empty());
                settings.ini_file = Some(file);
            } else if entries(&content).any(|(name, value)| name == "ctrlr" && !value.is_empty()) {
                settings.other_ctrlr.push(file);
            }
        }
    }
    settings.other_ctrlr.sort();
    settings
}

/// The `.ini` files directly in `dir`, sorted.
fn ini_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .take(MAX_INI_FILES)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("ini"))
                && path.is_file()
        })
        .collect();
    files.sort();
    files
}

fn read_small(file: &Path) -> Option<String> {
    let metadata = fs::metadata(file).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_INI_SIZE {
        return None;
    }
    fs::read(file)
        .ok()
        .map(|content| String::from_utf8_lossy(&content).into_owned())
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
    fn reads_input_settings_of_mame_ini_and_other_ini_files() {
        let dir = tempfile::tempdir().expect("temp dir");
        let mame_dir = dir.path().join("MAME ✓");
        let ini_dir = mame_dir.join("ini");
        fs::create_dir_all(ini_dir.join("source")).expect("dirs");
        let settings = read_input_settings(&mame_dir);
        // A home `.mame` directory could exist on Unix test machines.
        if settings.ini_file.is_none() {
            assert_eq!(settings.ctrlr, None);
            assert_eq!(settings.ctrlrpath, [DEFAULT_CTRLRPATH]);
        }

        fs::write(
            mame_dir.join("mame.ini"),
            "ctrlrpath  \"ctrlr;D:\\My Controllers\"\nctrlr  xarcade\njoystickprovider dinput\n",
        )
        .expect("write");
        // Only the first mame.ini of the inipath is read.
        fs::write(ini_dir.join("mame.ini"), "ctrlr ignored\n").expect("write");
        fs::write(ini_dir.join("vertical.ini"), "ctrlr\nrotate 1\n").expect("write");
        fs::write(ini_dir.join("sf2.ini"), "ctrlr fighting\n").expect("write");
        fs::write(
            ini_dir.join("source").join("cps1.ini"),
            "# x\nctrlr hotrod\n",
        )
        .expect("write");
        let settings = read_input_settings(&mame_dir);
        if settings.ini_file.as_deref() == Some(&mame_dir.join("mame.ini")) {
            assert_eq!(settings.ctrlr.as_deref(), Some("xarcade"));
            assert_eq!(settings.ctrlrpath, ["ctrlr", "D:\\My Controllers"]);
            assert_eq!(settings.joystickprovider.as_deref(), Some("dinput"));
        }
        // An empty ctrlr does not count.
        assert_eq!(
            settings.other_ctrlr,
            [
                ini_dir.join("sf2.ini"),
                ini_dir.join("source").join("cps1.ini")
            ]
        );

        // An empty ctrlr in mame.ini is no controller file.
        fs::write(mame_dir.join("mame.ini"), "ctrlr \"\"\nctrlr\n").expect("write");
        let settings = read_input_settings(&mame_dir);
        if settings.ini_file.as_deref() == Some(&mame_dir.join("mame.ini")) {
            assert_eq!(settings.ctrlr, None);
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
