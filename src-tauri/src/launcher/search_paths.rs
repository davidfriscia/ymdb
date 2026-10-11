//! Search path composition for the ROM manager's local destinations.
//!
//! `-rompath` and `-samplepath` replace MAME's value; they do not add to it.
//! When a remote source downloads into a directory MAME would not search,
//! the launcher passes the complete list: the value the launch already
//! resolved (from the global options, the profile or an override, by the
//! usual precedence) or, when no level sets it, MAME's own value from
//! `mame.ini`, followed by the missing destinations. Nothing is passed when
//! every destination is already searched, so MAME's configuration stays in
//! charge whenever possible.

use std::path::Path;

use super::config::{OptionSource, ResolvedOption};
use super::options::{DEFINITIONS, OptionValue};
use crate::mame::ini::{NativeSearchPaths, same_directory};

/// Directories that must be searched by a launch.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RequiredSearchPaths {
    pub rompath: Vec<String>,
    pub samplepath: Vec<String>,
    /// MAME's own values, used when no level sets the option.
    pub native: NativeSearchPaths,
}

/// Adds the missing required directories to the resolved options.
/// `mame_dir` is MAME's working directory, against which relative entries
/// are compared.
pub fn compose(options: &mut Vec<ResolvedOption>, required: &RequiredSearchPaths, mame_dir: &Path) {
    let targets = [
        ("rompath", &required.rompath, &required.native.rompath),
        (
            "samplepath",
            &required.samplepath,
            &required.native.samplepath,
        ),
    ];
    for (name, directories, native) in targets {
        if directories.is_empty() {
            continue;
        }
        let index = options.iter().position(|option| option.name == name);
        let mut current = match index.map(|index| &options[index].value) {
            Some(OptionValue::List(entries)) => entries.clone(),
            Some(_) => continue,
            None => native.clone(),
        };
        let missing: Vec<String> = directories
            .iter()
            .filter(|directory| {
                !current
                    .iter()
                    .any(|entry| same_directory(mame_dir, entry, directory))
            })
            .cloned()
            .collect();
        if missing.is_empty() {
            continue;
        }
        current.extend(missing);
        let composed = ResolvedOption {
            name: name.to_owned(),
            value: OptionValue::List(current),
            source: OptionSource::RomManager,
        };
        match index {
            Some(index) => options[index] = composed,
            None => {
                // Keep the definition order of the command line.
                let order = |option: &str| DEFINITIONS.iter().position(|d| d.name == option);
                let position = options
                    .iter()
                    .position(|option| order(&option.name) > order(name))
                    .unwrap_or(options.len());
                options.insert(position, composed);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn option(name: &str, value: OptionValue, source: OptionSource) -> ResolvedOption {
        ResolvedOption {
            name: name.to_owned(),
            value,
            source,
        }
    }

    fn list(entries: &[&str]) -> OptionValue {
        OptionValue::List(entries.iter().map(|&entry| entry.to_owned()).collect())
    }

    fn required(rompath: &[&str], samplepath: &[&str]) -> RequiredSearchPaths {
        RequiredSearchPaths {
            rompath: rompath.iter().map(|&entry| entry.to_owned()).collect(),
            samplepath: samplepath.iter().map(|&entry| entry.to_owned()).collect(),
            native: NativeSearchPaths {
                rompath: vec!["roms".to_owned(), "D:/Shared ROMs".to_owned()],
                samplepath: vec!["samples".to_owned()],
                ini_file: None,
            },
        }
    }

    #[test]
    fn nothing_changes_when_destinations_are_already_searched() {
        let dir = tempfile::tempdir().expect("temp dir");
        fs::create_dir(dir.path().join("roms")).expect("dir");
        let absolute_roms = dir.path().join("roms").to_string_lossy().into_owned();
        let mut options = vec![option(
            "window",
            OptionValue::Boolean(true),
            OptionSource::Global,
        )];
        let before = options.clone();
        compose(
            &mut options,
            &required(&["roms", &absolute_roms], &["samples"]),
            dir.path(),
        );
        assert_eq!(options, before);
        compose(&mut options, &RequiredSearchPaths::default(), dir.path());
        assert_eq!(options, before);
    }

    #[test]
    fn extends_mame_native_paths_in_definition_order() {
        let dir = tempfile::tempdir().expect("temp dir");
        let mut options = vec![
            option("window", OptionValue::Boolean(true), OptionSource::Global),
            option(
                "skip_gameinfo",
                OptionValue::Boolean(true),
                OptionSource::Global,
            ),
        ];
        compose(
            &mut options,
            &required(
                &["E:/Downloads/ROMs ✓", "E:/Downloads/CHDs"],
                &["E:/Samples"],
            ),
            dir.path(),
        );
        let names: Vec<_> = options.iter().map(|option| option.name.as_str()).collect();
        let window = names
            .iter()
            .position(|&name| name == "window")
            .expect("window");
        let rompath = names
            .iter()
            .position(|&name| name == "rompath")
            .expect("rompath");
        let samplepath = names
            .iter()
            .position(|&name| name == "samplepath")
            .expect("samplepath");
        assert!(window < rompath && rompath < samplepath);
        assert_eq!(
            options[rompath],
            option(
                "rompath",
                list(&[
                    "roms",
                    "D:/Shared ROMs",
                    "E:/Downloads/ROMs ✓",
                    "E:/Downloads/CHDs"
                ]),
                OptionSource::RomManager
            )
        );
        assert_eq!(options[samplepath].value, list(&["samples", "E:/Samples"]));
    }

    #[test]
    fn extends_the_value_resolved_by_precedence() {
        let dir = tempfile::tempdir().expect("temp dir");
        // A machine override replaced MAME's rompath: it is kept and extended,
        // MAME's own value is not brought back.
        let mut options = vec![option(
            "rompath",
            list(&["F:/Special"]),
            OptionSource::Machine,
        )];
        compose(&mut options, &required(&["E:/Downloads"], &[]), dir.path());
        assert_eq!(
            options,
            vec![option(
                "rompath",
                list(&["F:/Special", "E:/Downloads"]),
                OptionSource::RomManager
            )]
        );
        // Already present in the override: unchanged, still from the override.
        let mut options = vec![option(
            "rompath",
            list(&["E:/Downloads"]),
            OptionSource::Profile,
        )];
        compose(&mut options, &required(&["E:/Downloads"], &[]), dir.path());
        assert_eq!(options[0].source, OptionSource::Profile);
    }
}
