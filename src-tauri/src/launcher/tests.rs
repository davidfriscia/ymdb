//! Launch preparation tests on a catalog imported from fixtures. Nothing here
//! starts MAME.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use rusqlite::Connection;
use tempfile::TempDir;

use super::command::ArgumentRole;
use super::config::{LaunchConfig, OptionSource, Profile, software_key};
use super::controls::ControlsContext;
use super::options::{OptionSet, OptionValue};
use super::resolve::{ListRelation, MediaAssignment, SoftwareLaunchInfo, software_launch_info};
use super::search_paths::RequiredSearchPaths;
use super::{LaunchError, LaunchRequest, PreparedLaunch, SoftwareSelection, prepare};
use crate::catalog::import::{ImportProgress, import_from_reader, import_software_from_reader};
use crate::catalog::store::{CatalogSource, CatalogStore};
use crate::controllers::ControllerPaths;
use crate::controllers::model::{
    Assignment, AssignmentLevel, AssignmentTarget, ControllerInput, ControllerKind,
    ControllersConfig, DeviceClass,
};

const MACHINES: &str = include_str!("../catalog/testdata/launcher-machines.xml");
const SOFTWARE: &str = include_str!("../catalog/testdata/listsoftware-sample.xml");

struct Catalog {
    _dir: TempDir,
    store: CatalogStore,
}

impl Catalog {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = CatalogStore::in_dir(dir.path());
        let source = CatalogSource {
            mame_release: "0.280".to_owned(),
            mame_revision: "mame0280".to_owned(),
            executable_path: "/opt/mame/mame".into(),
            executable_size: Some(1),
            executable_modified_at: Some(1),
        };
        let cancel = AtomicBool::new(false);
        import_from_reader(
            MACHINES.as_bytes(),
            &store,
            source.clone(),
            &cancel,
            &ImportProgress::default(),
            || Ok(()),
        )
        .expect("machines");
        import_software_from_reader(
            SOFTWARE.as_bytes(),
            &store,
            source,
            &cancel,
            &ImportProgress::default(),
            || Ok(()),
        )
        .expect("software");
        Self { _dir: dir, store }
    }

    fn read<T>(&self, operation: impl FnOnce(&Connection) -> T) -> T {
        self.store
            .with_reader(|connection| Ok(operation(connection)))
            .expect("store")
            .expect("catalog")
    }

    fn info(&self, list: &str, name: &str) -> SoftwareLaunchInfo {
        self.read(|connection| software_launch_info(connection, list, name).expect("query"))
            .expect("software")
    }

    fn prepare(
        &self,
        config: &LaunchConfig,
        request: &LaunchRequest,
    ) -> Result<PreparedLaunch, LaunchError> {
        self.prepare_with(config, &RequiredSearchPaths::default(), request)
    }

    fn prepare_with(
        &self,
        config: &LaunchConfig,
        search_paths: &RequiredSearchPaths,
        request: &LaunchRequest,
    ) -> Result<PreparedLaunch, LaunchError> {
        let controllers = ControllersConfig::new();
        let paths = ControllerPaths::new(Path::new("unused"));
        let controls = ControlsContext {
            config: &controllers,
            paths: &paths,
        };
        self.prepare_controls(config, search_paths, controls, request)
    }

    fn prepare_controls(
        &self,
        config: &LaunchConfig,
        search_paths: &RequiredSearchPaths,
        controls: ControlsContext<'_>,
        request: &LaunchRequest,
    ) -> Result<PreparedLaunch, LaunchError> {
        self.prepare_at(&executable(), config, search_paths, controls, request)
    }

    /// Prepares with the MAME executable `executable` (whose directory may
    /// hold INI files).
    fn prepare_at(
        &self,
        executable: &Path,
        config: &LaunchConfig,
        search_paths: &RequiredSearchPaths,
        controls: ControlsContext<'_>,
        request: &LaunchRequest,
    ) -> Result<PreparedLaunch, LaunchError> {
        self.read(|connection| {
            prepare(
                connection,
                executable,
                config,
                search_paths,
                controls,
                request,
            )
        })
    }
}

const INSTALLATION_ID: &str = "0123456789abcdef0123456789abcdef";

fn executable() -> PathBuf {
    PathBuf::from("/home/user/Émulateurs MAME ✓/mame")
}

fn machine_request(machine: &str) -> LaunchRequest {
    LaunchRequest {
        installation_id: INSTALLATION_ID.to_owned(),
        machine: machine.to_owned(),
        software: None,
        profile_id: None,
        controls: None,
    }
}

fn software_request(
    machine: &str,
    list: &str,
    name: &str,
    media: &[(&str, &str)],
) -> LaunchRequest {
    LaunchRequest {
        installation_id: INSTALLATION_ID.to_owned(),
        machine: machine.to_owned(),
        software: Some(SoftwareSelection {
            list: list.to_owned(),
            name: name.to_owned(),
            media: media
                .iter()
                .map(|(device, part)| MediaAssignment {
                    device: (*device).to_owned(),
                    part: (*part).to_owned(),
                })
                .collect(),
        }),
        profile_id: None,
        controls: None,
    }
}

fn options(entries: &[(&str, OptionValue)]) -> OptionSet {
    entries
        .iter()
        .map(|(name, value)| ((*name).to_owned(), value.clone()))
        .collect()
}

#[test]
fn launches_a_machine_with_native_configuration_only() {
    let catalog = Catalog::new();
    let prepared = catalog
        .prepare(&LaunchConfig::default(), &machine_request("pong"))
        .expect("prepare");
    assert_eq!(prepared.plan.command_line(), ["pong"]);
    assert_eq!(prepared.plan.executable, executable());
    assert_eq!(
        prepared.plan.working_directory,
        Path::new("/home/user/Émulateurs MAME ✓")
    );
    assert_eq!(prepared.title, "Pong (Rev E)");
    assert_eq!(prepared.target.machine, "pong");
    assert!(prepared.target.software.is_none());
    assert!(prepared.warnings.is_empty());
}

#[test]
fn applies_options_by_precedence_with_their_origin() {
    let catalog = Catalog::new();
    let mut config = LaunchConfig {
        global_options: options(&[
            ("window", OptionValue::Boolean(true)),
            ("volume", OptionValue::Number(-10.0)),
        ]),
        ..LaunchConfig::default()
    };
    config.profiles.push(Profile {
        id: "test".to_owned(),
        name: "Test".to_owned(),
        options: options(&[
            ("volume", OptionValue::Number(-20.0)),
            ("seconds_to_run", OptionValue::Number(5.0)),
        ]),
    });
    config.machine_overrides.insert(
        "nes".to_owned(),
        options(&[("seconds_to_run", OptionValue::Number(10.0))]),
    );
    config.software_overrides.insert(
        software_key("nes", "smb"),
        options(&[("window", OptionValue::Boolean(false))]),
    );
    config.validate().expect("valid");

    let mut request = software_request("nes", "nes", "smb", &[("cart", "cart")]);
    request.profile_id = Some("test".to_owned());
    let prepared = catalog.prepare(&config, &request).expect("prepare");
    assert_eq!(
        prepared.plan.command_line(),
        [
            "nes",
            "-cart",
            "nes:smb:cart",
            "-nowindow",
            "-volume",
            "-20",
            "-seconds_to_run",
            "10"
        ]
    );
    let sources: Vec<_> = prepared
        .plan
        .arguments
        .iter()
        .filter_map(|argument| match &argument.role {
            ArgumentRole::Option { name, source } => Some((name.as_str(), *source)),
            _ => None,
        })
        .collect();
    assert_eq!(
        sources,
        [
            ("window", OptionSource::Software),
            ("volume", OptionSource::Profile),
            ("seconds_to_run", OptionSource::Machine)
        ]
    );

    // The same machine without the software uses the machine override and global options.
    let prepared = catalog
        .prepare(&config, &machine_request("nes"))
        .expect("prepare");
    assert_eq!(
        prepared.plan.command_line(),
        ["nes", "-window", "-volume", "-10", "-seconds_to_run", "10"]
    );
}

#[test]
fn rejects_missing_or_non_runnable_machines_and_unknown_profiles() {
    let catalog = Catalog::new();
    let config = LaunchConfig::default();
    assert!(matches!(
        catalog.prepare(&config, &machine_request("missing")),
        Err(LaunchError::NotFound(_))
    ));
    assert!(matches!(
        catalog.prepare(&config, &machine_request("z80")),
        Err(LaunchError::Invalid(_))
    ));
    for name in ["", "-help", "pong; rm -rf /", "Pong", "../pong"] {
        assert!(
            matches!(
                catalog.prepare(&config, &machine_request(name)),
                Err(LaunchError::Invalid(_))
            ),
            "{name}"
        );
    }
    let mut request = machine_request("pong");
    request.profile_id = Some("missing".to_owned());
    assert!(matches!(
        catalog.prepare(&config, &request),
        Err(LaunchError::Invalid(_))
    ));
}

#[test]
fn lists_candidate_machines_with_their_compatibility() {
    let catalog = Catalog::new();
    let info = catalog.info("apple2_flop_orig", "ultima4");
    assert_eq!(info.parts.len(), 2);
    assert_eq!(
        info.parts[0].label.as_deref(),
        Some("Side A - Program Disk")
    );
    assert_eq!(
        info.parts[0].compatibility.as_deref(),
        Some("A2P,A2E,A2EE,A2C,A2GS")
    );

    let summary: Vec<_> = info
        .candidates
        .iter()
        .map(|c| (c.name.as_str(), c.relation, c.filter_match, c.launchable))
        .collect();
    assert_eq!(
        summary,
        [
            // Original and no filter, but no floppy drive: cannot mount the software.
            ("apple2tape", ListRelation::Original, true, false),
            // Original, but its filter (A2) is not in the software's compatibility values.
            ("apple2", ListRelation::Original, false, true),
            // Matching filter, but declared as a compatible list.
            ("apple2e", ListRelation::Compatible, true, true),
        ]
    );
    // No single valid choice: the user must choose.
    assert_eq!(info.recommended, None);
    let apple2e = &info.candidates[2];
    assert_eq!(
        apple2e
            .devices
            .iter()
            .map(|d| d.brief_name.as_str())
            .collect::<Vec<_>>(),
        ["flop1", "flop2"]
    );
    assert_eq!(
        apple2e.default_media,
        [
            MediaAssignment {
                device: "flop1".to_owned(),
                part: "flop1".to_owned()
            },
            MediaAssignment {
                device: "flop2".to_owned(),
                part: "flop2".to_owned()
            },
        ]
    );
}

#[test]
fn recommends_a_machine_only_when_it_is_the_only_valid_choice() {
    let catalog = Catalog::new();
    // Only c64 declares c64_cart, as original, and its NTSC filter matches.
    let info = catalog.info("c64_cart", "gameb2");
    assert_eq!(info.recommended.as_deref(), Some("c64"));
    // Two original machines accept smb (no compatibility value): no preference.
    let info = catalog.info("nes", "smb");
    assert_eq!(info.candidates.len(), 2);
    assert!(
        info.candidates
            .iter()
            .all(|c| c.filter_match && c.launchable)
    );
    assert_eq!(info.recommended, None);
    // A list that no fixture machine declares has no candidates.
    let info = catalog.info("pcecd", "ys12");
    assert!(info.candidates.is_empty());
    assert_eq!(info.recommended, None);
}

#[test]
fn launches_multi_part_software_with_explicit_media() {
    let catalog = Catalog::new();
    let config = LaunchConfig::default();
    let prepared = catalog
        .prepare(
            &config,
            &software_request(
                "apple2e",
                "apple2_flop_orig",
                "ultima4",
                &[("flop1", "flop1"), ("flop2", "flop2")],
            ),
        )
        .expect("prepare");
    assert_eq!(
        prepared.plan.command_line(),
        [
            "apple2e",
            "-flop1",
            "apple2_flop_orig:ultima4:flop1",
            "-flop2",
            "apple2_flop_orig:ultima4:flop2"
        ]
    );
    assert_eq!(prepared.title, "Ultima IV: Quest of the Avatar (Apple //e)");
    assert_eq!(prepared.warnings.len(), 1, "{:?}", prepared.warnings);
    assert!(prepared.warnings[0].contains("compatible list"));

    // One part only, in the second drive.
    let prepared = catalog
        .prepare(
            &config,
            &software_request(
                "apple2e",
                "apple2_flop_orig",
                "ultima4",
                &[("flop2", "flop2")],
            ),
        )
        .expect("prepare");
    assert_eq!(
        prepared.plan.command_line(),
        ["apple2e", "-flop2", "apple2_flop_orig:ultima4:flop2"]
    );

    // The filter mismatch is reported but the user may still launch.
    let prepared = catalog
        .prepare(
            &config,
            &software_request(
                "apple2",
                "apple2_flop_orig",
                "ultima4",
                &[("flop1", "flop1")],
            ),
        )
        .expect("prepare");
    assert!(
        prepared
            .warnings
            .iter()
            .any(|warning| warning.contains("filter")),
        "{:?}",
        prepared.warnings
    );
}

#[test]
fn rejects_invalid_software_launches() {
    let catalog = Catalog::new();
    let config = LaunchConfig::default();
    let invalid = [
        // The machine does not declare the list.
        software_request("nes", "c64_cart", "gamebase", &[("cart", "cart")]),
        // No device for the part.
        software_request(
            "apple2tape",
            "apple2_flop_orig",
            "ultima4",
            &[("cass", "flop1")],
        ),
        // Unknown device, unknown part, nothing mounted, device used twice.
        software_request("nes", "nes", "smb", &[("flop1", "cart")]),
        software_request("nes", "nes", "smb", &[("cart", "disk")]),
        software_request("nes", "nes", "smb", &[]),
        software_request(
            "apple2e",
            "apple2_flop_orig",
            "ultima4",
            &[("flop1", "flop1"), ("flop1", "flop2")],
        ),
        // Names that are not MAME short names.
        software_request("nes", "nes", "smb -window", &[("cart", "cart")]),
    ];
    for request in invalid {
        assert!(
            matches!(
                catalog.prepare(&config, &request),
                Err(LaunchError::Invalid(_))
            ),
            "{request:?}"
        );
    }
    assert!(matches!(
        catalog.prepare(
            &config,
            &software_request("nes", "nes", "missing", &[("cart", "cart")])
        ),
        Err(LaunchError::NotFound(_))
    ));
}

#[test]
fn preview_and_execution_share_the_same_plan() {
    let catalog = Catalog::new();
    let prepared = catalog
        .prepare(
            &LaunchConfig::default(),
            &software_request("c64", "c64_cart", "gameb2", &[("cart", "cart")]),
        )
        .expect("prepare");
    let command = prepared.plan.command();
    let executed: Vec<String> = command
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    let preview = super::LaunchPreview::from(prepared);
    assert_eq!(executed, preview.command_line);
    assert_eq!(
        preview.command_line,
        ["c64", "-cart", "c64_cart:gameb2:cart"]
    );
}

#[test]
fn adds_rom_manager_destinations_to_the_search_paths() {
    let catalog = Catalog::new();
    let native = crate::mame::ini::NativeSearchPaths::default();
    let required = RequiredSearchPaths {
        rompath: vec!["/srv/YMDB downloads ✓/roms".to_owned()],
        samplepath: vec!["samples".to_owned()],
        native,
    };
    // MAME's own rompath is kept and the destination added; the sample
    // destination is already MAME's default, so samplepath is not passed.
    let prepared = catalog
        .prepare_with(
            &LaunchConfig::default(),
            &required,
            &machine_request("pong"),
        )
        .expect("prepare");
    assert_eq!(
        prepared.plan.command_line(),
        ["pong", "-rompath", "roms;/srv/YMDB downloads ✓/roms"]
    );
    assert!(prepared.plan.arguments.iter().any(|argument| matches!(
        &argument.role,
        ArgumentRole::Option { name, source: OptionSource::RomManager } if name == "rompath"
    )));

    // A rompath from the profile wins over mame.ini and is extended.
    let mut config = LaunchConfig::default();
    config.profiles[0].options =
        options(&[("rompath", OptionValue::List(vec!["/mnt/roms".to_owned()]))]);
    let prepared = catalog
        .prepare_with(&config, &required, &machine_request("pong"))
        .expect("prepare");
    assert_eq!(
        prepared.plan.command_line(),
        ["pong", "-rompath", "/mnt/roms;/srv/YMDB downloads ✓/roms"]
    );
}

/// Controller profiles for the launch tests, in a temporary configuration
/// directory: `fighting` (no base file) and `six-buttons` (with one).
struct Controls {
    dir: TempDir,
    config: ControllersConfig,
    paths: ControllerPaths,
}

const BASE_FILE: &str = r#"<mameconfig version="10"><system name="default"><input><port type="P1_BUTTON1"><newseq type="standard">KEYCODE_X</newseq></port></input></system></mameconfig>"#;

impl Controls {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        let paths = ControllerPaths::new(&dir.path().join("config ✓"));
        let mut config = ControllersConfig::new();
        config
            .create_profile("Fighting", false, |_| false)
            .expect("fighting");
        config
            .create_profile("Six Buttons", true, |_| false)
            .expect("six");
        config
            .create_controller(ControllerInput {
                name: "Stick".to_owned(),
                kind: ControllerKind::ArcadeStick,
                device_class: DeviceClass::Joystick,
                device_id: None,
                provider: None,
                binding: None,
            })
            .expect("stick");
        fs::create_dir_all(paths.profile_dir("six-buttons")).expect("dir");
        fs::write(paths.base_file("six-buttons"), BASE_FILE).expect("base");
        Self { dir, config, paths }
    }

    fn assign(&mut self, level: AssignmentLevel, key: Option<&str>, profile: Option<&str>) {
        self.config
            .set_assignment(
                &AssignmentTarget {
                    level,
                    key: key.map(str::to_owned),
                },
                Assignment {
                    profile_id: profile.map(str::to_owned),
                    players: None,
                },
            )
            .expect("assign");
    }

    fn context(&self) -> ControlsContext<'_> {
        ControlsContext {
            config: &self.config,
            paths: &self.paths,
        }
    }

    fn cfg_directory(&self, profile: &str) -> String {
        self.paths
            .context_dir(INSTALLATION_ID, profile)
            .to_string_lossy()
            .into_owned()
    }

    /// Every file and directory under the temporary directory.
    fn files(&self) -> Vec<PathBuf> {
        fn walk(dir: &Path, entries: &mut Vec<PathBuf>) {
            for entry in fs::read_dir(dir).expect("read").flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, entries);
                }
                entries.push(path);
            }
        }
        let mut entries = Vec::new();
        walk(self.dir.path(), &mut entries);
        entries.sort();
        entries
    }
}

fn option_sources(prepared: &PreparedLaunch) -> Vec<(String, OptionSource)> {
    prepared
        .plan
        .arguments
        .iter()
        .filter_map(|argument| match &argument.role {
            ArgumentRole::Option { name, source } => Some((name.clone(), *source)),
            _ => None,
        })
        .collect()
}

#[test]
fn the_mame_profile_keeps_the_previous_command_line() {
    let catalog = Catalog::new();
    let controls = Controls::new();
    let mut config = LaunchConfig {
        global_options: options(&[
            ("window", OptionValue::Boolean(true)),
            ("cfg_directory", OptionValue::Text("D:/My cfg".to_owned())),
        ]),
        ..LaunchConfig::default()
    };
    config.profiles[0].options = options(&[("speed", OptionValue::Number(2.0))]);
    let required = RequiredSearchPaths {
        rompath: vec!["/srv/downloads".to_owned()],
        ..RequiredSearchPaths::default()
    };
    let requests = [
        machine_request("pong"),
        software_request("nes", "nes", "smb", &[("cart", "cart")]),
        software_request(
            "apple2e",
            "apple2_flop_orig",
            "ultima4",
            &[("flop1", "flop1"), ("flop2", "flop2")],
        ),
    ];
    for request in requests {
        // The command line built before controller management existed.
        let without = catalog
            .prepare_with(&config, &required, &request)
            .expect("prepare");
        let with = catalog
            .prepare_controls(&config, &required, controls.context(), &request)
            .expect("prepare");
        assert_eq!(with.plan.command_line(), without.plan.command_line());
        assert_eq!(with.warnings, without.warnings);
        assert_eq!(with.plan.controls.profile_id, "mame");
        assert!(!with.plan.controls.is_managed());
        assert!(
            option_sources(&with)
                .iter()
                .all(|(_, source)| *source != OptionSource::Controllers)
        );
    }
    let prepared = catalog
        .prepare_controls(
            &config,
            &required,
            controls.context(),
            &machine_request("pong"),
        )
        .expect("prepare");
    assert_eq!(
        prepared.plan.command_line(),
        [
            "pong",
            "-window",
            "-speed",
            "2",
            "-rompath",
            "roms;/srv/downloads",
            "-cfg_directory",
            "D:/My cfg"
        ]
    );
}

#[test]
fn a_managed_profile_without_base_uses_its_directory_and_no_controller_file() {
    let catalog = Catalog::new();
    let mut controls = Controls::new();
    controls.assign(AssignmentLevel::Default, None, Some("fighting"));
    let files = controls.files();
    let prepared = catalog
        .prepare_controls(
            &LaunchConfig::default(),
            &RequiredSearchPaths::default(),
            controls.context(),
            &machine_request("pong"),
        )
        .expect("prepare");
    let cfg = controls.cfg_directory("fighting");
    assert_eq!(
        prepared.plan.command_line(),
        ["pong", "-cfg_directory", cfg.as_str(), "-ctrlr", ""]
    );
    // The empty value is a real, separate argument of the process.
    let args: Vec<_> = prepared
        .plan
        .command()
        .get_args()
        .map(|arg| arg.to_owned())
        .collect();
    assert_eq!(args.len(), 5);
    assert!(args[4].is_empty());
    assert_eq!(
        option_sources(&prepared),
        [
            ("cfg_directory".to_owned(), OptionSource::Controllers),
            ("ctrlr".to_owned(), OptionSource::Controllers)
        ]
    );
    let plan = &prepared.plan.controls;
    assert_eq!(plan.profile_id, "fighting");
    assert_eq!(plan.profile_name, "Fighting");
    assert_eq!(plan.cfg_directory.as_deref(), Some(Path::new(&cfg)));
    assert_eq!(plan.base_file, None);
    assert!(prepared.warnings.is_empty(), "{:?}", prepared.warnings);
    // Preparing (and so previewing) creates nothing.
    assert_eq!(controls.files(), files);
    assert!(!Path::new(&cfg).exists());

    // Starting creates the directory, and nothing else.
    super::controls::materialize(plan).expect("materialize");
    assert!(Path::new(&cfg).is_dir());
    assert!(fs::read_dir(&cfg).expect("read").next().is_none());
    super::controls::materialize(plan).expect("again");
}

#[test]
fn a_managed_profile_with_base_passes_its_controller_file() {
    let catalog = Catalog::new();
    let mut controls = Controls::new();
    controls.assign(AssignmentLevel::Machine, Some("nes"), Some("six-buttons"));
    let request = software_request("nes", "nes", "smb", &[("cart", "cart")]);
    let prepared = catalog
        .prepare_controls(
            &LaunchConfig::default(),
            &RequiredSearchPaths::default(),
            controls.context(),
            &request,
        )
        .expect("prepare");
    let cfg = controls.cfg_directory("six-buttons");
    let profile_dir = controls
        .paths
        .profile_dir("six-buttons")
        .to_string_lossy()
        .into_owned();
    assert_eq!(
        prepared.plan.command_line(),
        [
            "nes",
            "-cart",
            "nes:smb:cart",
            "-cfg_directory",
            cfg.as_str(),
            "-ctrlrpath",
            profile_dir.as_str(),
            "-ctrlr",
            "base"
        ]
    );
    assert_eq!(
        prepared.plan.controls.base_file.as_deref(),
        Some(controls.paths.base_file("six-buttons").as_path())
    );
    // The base file is never written by a launch.
    assert_eq!(
        fs::read_to_string(controls.paths.base_file("six-buttons")).expect("read"),
        BASE_FILE
    );

    // A missing base file is reported before MAME runs.
    fs::remove_file(controls.paths.base_file("six-buttons")).expect("remove");
    assert!(matches!(
        catalog.prepare_controls(
            &LaunchConfig::default(),
            &RequiredSearchPaths::default(),
            controls.context(),
            &request
        ),
        Err(LaunchError::Invalid(message)) if message.contains("missing")
    ));
    assert!(super::controls::materialize(&prepared.plan.controls).is_err());
}

#[test]
fn a_managed_profile_replaces_the_cfg_directory_launch_option() {
    let catalog = Catalog::new();
    let mut controls = Controls::new();
    controls.assign(AssignmentLevel::Default, None, Some("fighting"));
    let mut config = LaunchConfig::default();
    config.machine_overrides.insert(
        "pong".to_owned(),
        options(&[
            ("cfg_directory", OptionValue::Text("D:/Pong cfg".to_owned())),
            ("window", OptionValue::Boolean(true)),
        ]),
    );
    let prepared = catalog
        .prepare_controls(
            &config,
            &RequiredSearchPaths::default(),
            controls.context(),
            &machine_request("pong"),
        )
        .expect("prepare");
    let cfg = controls.cfg_directory("fighting");
    assert_eq!(
        prepared.plan.command_line(),
        [
            "pong",
            "-window",
            "-cfg_directory",
            cfg.as_str(),
            "-ctrlr",
            ""
        ]
    );
    assert_eq!(prepared.warnings.len(), 1, "{:?}", prepared.warnings);
    assert!(
        prepared.warnings[0].contains("cfg_directory (from the machine options)"),
        "{}",
        prepared.warnings[0]
    );
    // The launch configuration keeps the option for the `mame` profile.
    assert!(config.machine_overrides["pong"].contains_key("cfg_directory"));
}

#[test]
fn the_request_overrides_controls_independently_of_the_launch_profile() {
    let catalog = Catalog::new();
    let mut controls = Controls::new();
    controls.assign(AssignmentLevel::Default, None, Some("fighting"));
    let mut config = LaunchConfig::default();
    config.profiles.push(Profile {
        id: "fast".to_owned(),
        name: "Fast".to_owned(),
        options: options(&[("speed", OptionValue::Number(4.0))]),
    });
    let before = controls.config.clone();
    let prepare = |request: &LaunchRequest| {
        catalog.prepare_controls(
            &config,
            &RequiredSearchPaths::default(),
            controls.context(),
            request,
        )
    };

    // A launch profile does not change the controller profile.
    let mut request = machine_request("pong");
    request.profile_id = Some("fast".to_owned());
    let prepared = prepare(&request).expect("prepare");
    assert_eq!(prepared.plan.profile_id, "fast");
    assert_eq!(prepared.plan.controls.profile_id, "fighting");
    assert_eq!(prepared.plan.command_line()[1..3], ["-speed", "4"]);

    // A temporary controller profile does not change the launch profile.
    request.controls = Some(Assignment {
        profile_id: Some("mame".to_owned()),
        players: None,
    });
    let prepared = prepare(&request).expect("prepare");
    assert_eq!(prepared.plan.profile_id, "fast");
    assert_eq!(prepared.plan.controls.profile_id, "mame");
    assert_eq!(
        prepared.plan.controls.profile_source,
        crate::controllers::resolve::ControlsSource::Request
    );
    assert_eq!(prepared.plan.command_line(), ["pong", "-speed", "4"]);

    // A controller without a configured device cannot be mapped.
    request.controls = Some(Assignment {
        profile_id: None,
        players: Some(vec![Some("stick".to_owned())]),
    });
    assert!(matches!(
        prepare(&request),
        Err(LaunchError::Invalid(message)) if message.contains("Player 1") && message.contains("not configured")
    ));
    // Assigning nobody keeps the profile's command line.
    request.controls = Some(Assignment {
        profile_id: None,
        players: Some(vec![None, None]),
    });
    let prepared = prepare(&request).expect("prepare");
    assert!(prepared.plan.controls.players.is_empty());
    assert_eq!(prepared.plan.controls.profile_id, "fighting");
    let cfg = controls.cfg_directory("fighting");
    assert_eq!(
        prepared.plan.command_line(),
        [
            "pong",
            "-speed",
            "4",
            "-cfg_directory",
            cfg.as_str(),
            "-ctrlr",
            ""
        ]
    );
    assert!(prepared.warnings.is_empty(), "{:?}", prepared.warnings);

    // Unknown references in a request are refused.
    request.controls = Some(Assignment {
        profile_id: Some("missing".to_owned()),
        players: None,
    });
    assert!(matches!(prepare(&request), Err(LaunchError::Invalid(_))));
    // The request never changes the saved assignments.
    assert_eq!(controls.config, before);
}

#[test]
fn launch_requests_accept_controls() {
    let request: LaunchRequest = serde_json::from_value(serde_json::json!({
        "installationId": INSTALLATION_ID,
        "machine": "pong",
        "controls": { "profileId": "fighting", "players": [null, "stick"] }
    }))
    .expect("parse");
    assert_eq!(
        request.controls,
        Some(Assignment {
            profile_id: Some("fighting".to_owned()),
            players: Some(vec![None, Some("stick".to_owned())]),
        })
    );
    let request: LaunchRequest = serde_json::from_value(serde_json::json!({
        "installationId": INSTALLATION_ID,
        "machine": "pong"
    }))
    .expect("parse");
    assert_eq!(request.controls, None);
    assert!(
        serde_json::from_value::<LaunchRequest>(serde_json::json!({
            "installationId": INSTALLATION_ID,
            "machine": "pong",
            "controls": { "ctrlr": "x" }
        }))
        .is_err()
    );
}

// --- Player mappings -------------------------------------------------------

use crate::controllers::devices::JoystickProvider;
use crate::controllers::model::ControllerBinding;

const PAD_A: &str = "USB gamepad            product_e401081f-0000-0000-0000-504944564944 instance_1a2b3c4d-1111-11f0-8001-444553540000";
const PAD_B: &str = "USB gamepad            product_e401081f-0000-0000-0000-504944564944 instance_5e6f7a8b-2222-11f1-8001-444553540000";
const F310: &str = "Controller (Gamepad F310) product_c21d046d-0000-0000-0000-504944564944 instance_9c0d1e2f-3333-11f0-8001-444553540000";

/// The provider `auto` selects on this operating system.
fn provider() -> JoystickProvider {
    JoystickProvider::available()[0]
}

impl Controls {
    /// Registers a mapped joystick; returns its id.
    fn add_device(&mut self, name: &str, device_id: &str, binding: ControllerBinding) -> String {
        self.config
            .create_controller(ControllerInput {
                name: name.to_owned(),
                kind: ControllerKind::Gamepad,
                device_class: DeviceClass::Joystick,
                device_id: Some(device_id.to_owned()),
                provider: Some(provider().name().to_owned()),
                binding: Some(binding),
            })
            .expect("controller")
    }

    fn assign_players(
        &mut self,
        level: AssignmentLevel,
        key: Option<&str>,
        players: &[Option<&str>],
    ) {
        let target = AssignmentTarget {
            level,
            key: key.map(str::to_owned),
        };
        self.config
            .update_assignment(
                &target,
                crate::controllers::model::AssignmentUpdate {
                    players: crate::controllers::model::FieldUpdate::Set {
                        value: players.iter().map(|id| id.map(str::to_owned)).collect(),
                    },
                    ..Default::default()
                },
            )
            .expect("assign players");
    }

    /// Three gamepads, as in the hardware tests: F310 (device binding) and
    /// two identical GP100 (port order).
    fn with_three_pads() -> Self {
        let mut controls = Self::new();
        controls.add_device("F310", F310, ControllerBinding::Device);
        controls.add_device("GP100 A", PAD_A, ControllerBinding::PortOrder);
        controls.add_device("GP100 B", PAD_B, ControllerBinding::PortOrder);
        controls
    }
}

fn generated_args(controls: &Controls, prepared: &PreparedLaunch) -> Vec<String> {
    let file = prepared
        .plan
        .controls
        .controller_file
        .as_ref()
        .expect("file");
    vec![
        "-ctrlrpath".to_owned(),
        controls
            .paths
            .generated_dir()
            .to_string_lossy()
            .into_owned(),
        "-ctrlr".to_owned(),
        file.name.clone(),
        "-joystickprovider".to_owned(),
        provider().name().to_owned(),
        "-verbose".to_owned(),
    ]
}

#[test]
fn the_mame_profile_maps_players_and_keeps_its_configuration_directory() {
    let catalog = Catalog::new();
    let mut controls = Controls::with_three_pads();
    controls.assign_players(
        AssignmentLevel::Default,
        None,
        &[Some("f310"), Some("gp100-a"), Some("gp100-b")],
    );
    let config = LaunchConfig {
        global_options: options(&[("window", OptionValue::Boolean(true))]),
        ..LaunchConfig::default()
    };
    let files = controls.files();
    let prepared = catalog
        .prepare_controls(
            &config,
            &RequiredSearchPaths::default(),
            controls.context(),
            &machine_request("pong"),
        )
        .expect("prepare");
    let plan = &prepared.plan.controls;
    assert_eq!(plan.profile_id, "mame");
    // MAME's own configuration directory: no -cfg_directory.
    assert!(!plan.is_managed());
    let mut expected = vec!["pong".to_owned(), "-window".to_owned()];
    expected.extend(generated_args(&controls, &prepared));
    assert_eq!(prepared.plan.command_line(), expected);
    assert!(
        option_sources(&prepared)
            .iter()
            .skip(1)
            .all(|(_, source)| *source == OptionSource::Controllers)
    );
    assert_eq!(
        plan.mappings
            .iter()
            .map(|mapping| (
                mapping.player,
                mapping.controller_id.as_str(),
                mapping.binding
            ))
            .collect::<Vec<_>>(),
        [
            (1, "f310", ControllerBinding::Device),
            (2, "gp100-a", ControllerBinding::PortOrder),
            (3, "gp100-b", ControllerBinding::PortOrder)
        ]
    );
    assert_eq!(plan.joystick_provider, Some(provider()));
    let file = plan.controller_file.as_ref().expect("file");
    assert_eq!(file.baseline, None);
    let info = crate::controllers::base_file::validate(file.content.as_bytes()).expect("valid");
    assert_eq!(info.mappings.len(), 3);
    assert_eq!(info.mappings[2].device, PAD_B);
    assert_eq!(info.mappings[2].controller.to_string(), "JOYCODE_3");
    // Port order controllers are named in a warning.
    assert!(
        prepared
            .warnings
            .iter()
            .any(|warning| warning.contains("GP100 A, GP100 B")
                && warning.contains("Swapping ports")),
        "{:?}",
        prepared.warnings
    );

    // The preview writes nothing; starting writes the file once.
    assert_eq!(controls.files(), files);
    assert!(!file.path.exists());
    super::controls::materialize(plan).expect("materialize");
    assert_eq!(fs::read_to_string(&file.path).expect("read"), file.content);
    assert_eq!(file.path.parent(), Some(controls.paths.generated_dir()));
    let modified = fs::metadata(&file.path)
        .and_then(|m| m.modified())
        .expect("mtime");
    super::controls::materialize(plan).expect("again");
    assert_eq!(
        fs::metadata(&file.path)
            .and_then(|m| m.modified())
            .expect("mtime"),
        modified
    );
    // A file with that name but other content is never reused.
    fs::write(&file.path, "corrupted").expect("write");
    super::controls::materialize(plan).expect("repaired");
    assert_eq!(fs::read_to_string(&file.path).expect("read"), file.content);
    // No temporary file is left.
    assert_eq!(
        fs::read_dir(controls.paths.generated_dir())
            .expect("dir")
            .count(),
        1
    );

    // Preparing again gives the same plan; other assignments another file.
    let again = catalog
        .prepare_controls(
            &config,
            &RequiredSearchPaths::default(),
            controls.context(),
            &machine_request("pong"),
        )
        .expect("prepare");
    assert_eq!(again.plan, prepared.plan);
    controls.assign_players(
        AssignmentLevel::Machine,
        Some("pong"),
        &[Some("gp100-b"), Some("gp100-a")],
    );
    let swapped = catalog
        .prepare_controls(
            &config,
            &RequiredSearchPaths::default(),
            controls.context(),
            &machine_request("pong"),
        )
        .expect("prepare");
    let swapped_file = swapped
        .plan
        .controls
        .controller_file
        .as_ref()
        .expect("file");
    assert_ne!(swapped_file.name, file.name);
    assert_eq!(swapped.plan.controls.mappings.len(), 2);
}

#[test]
fn players_and_profile_are_resolved_independently() {
    let catalog = Catalog::new();
    let mut controls = Controls::with_three_pads();
    controls.assign(AssignmentLevel::Default, None, Some("fighting"));
    controls.assign_players(AssignmentLevel::Machine, Some("pong"), &[Some("gp100-a")]);
    let prepare = |controls: &Controls, request: &LaunchRequest| {
        catalog.prepare_controls(
            &LaunchConfig::default(),
            &RequiredSearchPaths::default(),
            controls.context(),
            request,
        )
    };
    let prepared = prepare(&controls, &machine_request("pong")).expect("prepare");
    let plan = &prepared.plan.controls;
    assert_eq!(plan.profile_id, "fighting");
    assert_eq!(
        plan.profile_source,
        crate::controllers::resolve::ControlsSource::Default
    );
    assert_eq!(
        plan.players_source,
        Some(crate::controllers::resolve::ControlsSource::Machine)
    );
    // A managed profile without base: its directory, and the generated
    // file instead of `-ctrlr ""`.
    let cfg = controls.cfg_directory("fighting");
    let mut expected = vec!["pong".to_owned(), "-cfg_directory".to_owned(), cfg];
    expected.extend(generated_args(&controls, &prepared));
    assert_eq!(prepared.plan.command_line(), expected);

    // Changing the profile of the machine keeps its players, and the
    // reverse.
    controls.assign(AssignmentLevel::Machine, Some("pong"), Some("mame"));
    controls.assign_players(AssignmentLevel::Machine, Some("pong"), &[Some("gp100-a")]);
    let machine = controls
        .config
        .assignment(&AssignmentTarget {
            level: AssignmentLevel::Machine,
            key: Some("pong".to_owned()),
        })
        .cloned()
        .expect("machine");
    assert_eq!(machine.profile_id.as_deref(), Some("mame"));
    assert_eq!(machine.players, Some(vec![Some("gp100-a".to_owned())]));

    // A temporary selection overrides the players for this launch only.
    let before = controls.config.clone();
    let mut request = machine_request("pong");
    request.controls = Some(Assignment {
        profile_id: None,
        players: Some(vec![None, Some("f310".to_owned()), None]),
    });
    let prepared = prepare(&controls, &request).expect("prepare");
    let plan = &prepared.plan.controls;
    assert_eq!(plan.profile_id, "mame");
    assert_eq!(
        plan.players_source,
        Some(crate::controllers::resolve::ControlsSource::Request)
    );
    assert_eq!(plan.players, [None, Some("f310".to_owned())]);
    assert_eq!(plan.mappings.len(), 1);
    assert_eq!(plan.mappings[0].player, 2);
    // A partial assignment is allowed with a warning.
    assert!(
        prepared
            .warnings
            .iter()
            .any(|warning| warning.starts_with("Player 1 has no assigned controller"))
    );
    assert_eq!(controls.config, before);
    // Explicitly no controllers: no mapping, the previous command line.
    request.controls = Some(Assignment {
        profile_id: None,
        players: Some(Vec::new()),
    });
    let prepared = prepare(&controls, &request).expect("prepare");
    assert_eq!(prepared.plan.command_line(), ["pong"]);
    assert!(prepared.plan.controls.controller_file.is_none());

    // Software and machine + software levels.
    controls.assign_players(
        AssignmentLevel::Software,
        Some("nes:smb"),
        &[Some("gp100-b")],
    );
    controls.assign_players(
        AssignmentLevel::MachineSoftware,
        Some("nes/nes:smb"),
        &[Some("f310"), Some("gp100-b")],
    );
    let request = software_request("nes", "nes", "smb", &[("cart", "cart")]);
    let prepared = prepare(&controls, &request).expect("prepare");
    assert_eq!(
        prepared.plan.controls.players_source,
        Some(crate::controllers::resolve::ControlsSource::MachineSoftware)
    );
    assert_eq!(prepared.plan.controls.mappings.len(), 2);
}

#[test]
fn a_managed_profile_keeps_its_base_sections_with_mappings() {
    let catalog = Catalog::new();
    let mut controls = Controls::with_three_pads();
    controls.assign(AssignmentLevel::Default, None, Some("six-buttons"));
    controls.assign_players(
        AssignmentLevel::Default,
        None,
        &[Some("gp100-b"), Some("gp100-a")],
    );
    let prepared = catalog
        .prepare_controls(
            &LaunchConfig::default(),
            &RequiredSearchPaths::default(),
            controls.context(),
            &machine_request("pong"),
        )
        .expect("prepare");
    let plan = &prepared.plan.controls;
    let base = controls.paths.base_file("six-buttons");
    assert_eq!(plan.base_file.as_deref(), Some(base.as_path()));
    let file = plan.controller_file.as_ref().expect("file");
    assert_eq!(file.baseline.as_deref(), Some(base.as_path()));
    // The base sections follow the generated one, verbatim.
    let start = BASE_FILE.find("<system").expect("system");
    assert!(file.content.ends_with(&BASE_FILE[start..]));
    let info = crate::controllers::base_file::validate(file.content.as_bytes()).expect("valid");
    assert_eq!(info.systems, ["default", "default"]);
    // `-ctrlr base` is replaced by the generated file.
    let line = prepared.plan.command_line();
    assert!(!line.contains(&"base".to_owned()));
    let cfg = controls.cfg_directory("six-buttons");
    let mut expected = vec!["pong".to_owned(), "-cfg_directory".to_owned(), cfg];
    expected.extend(generated_args(&controls, &prepared));
    assert_eq!(line, expected);
    super::controls::materialize(plan).expect("materialize");
    assert_eq!(fs::read_to_string(&base).expect("base"), BASE_FILE);
}

#[test]
fn invalid_or_contradictory_assignments_are_refused() {
    let catalog = Catalog::new();
    let prepare = |controls: &Controls| {
        catalog.prepare_controls(
            &LaunchConfig::default(),
            &RequiredSearchPaths::default(),
            controls.context(),
            &machine_request("pong"),
        )
    };
    let refused = |controls: &Controls, expected: &str| match prepare(controls) {
        Err(LaunchError::Invalid(message)) => assert!(message.contains(expected), "{message}"),
        other => panic!("expected an error containing '{expected}', got {other:?}"),
    };

    // A controller of another class.
    let mut controls = Controls::new();
    controls
        .config
        .create_controller(ControllerInput {
            name: "Light Gun".to_owned(),
            kind: ControllerKind::Lightgun,
            device_class: DeviceClass::Lightgun,
            device_id: None,
            provider: None,
            binding: None,
        })
        .expect("gun");
    controls.assign_players(AssignmentLevel::Default, None, &[Some("light-gun")]);
    refused(&controls, "not a joystick");

    // Controllers of different providers.
    if JoystickProvider::available().len() > 1 {
        let mut controls = Controls::with_three_pads();
        let other = JoystickProvider::available()[1];
        controls
            .config
            .create_controller(ControllerInput {
                name: "Other provider".to_owned(),
                kind: ControllerKind::Gamepad,
                device_class: DeviceClass::Joystick,
                device_id: Some("Other pad".to_owned()),
                provider: Some(other.name().to_owned()),
                binding: Some(ControllerBinding::PortOrder),
            })
            .expect("other");
        controls.assign_players(
            AssignmentLevel::Default,
            None,
            &[Some("f310"), Some("other-provider")],
        );
        refused(&controls, "different joystick providers");
    }

    // A provider of another operating system, stored in controllers.json.
    let mut controls = Controls::with_three_pads();
    let foreign = JoystickProvider::ALL
        .into_iter()
        .find(|provider| !provider.is_available());
    if let Some(foreign) = foreign {
        controls.config.controllers[0].provider = Some(foreign.name().to_owned());
        controls.assign_players(AssignmentLevel::Default, None, &[Some("f310")]);
        refused(&controls, "not available on this operating system");
    }

    // A base file without default section (imported before the check) is
    // reported and kept as it is.
    let mut controls = Controls::new();
    controls.assign(AssignmentLevel::Default, None, Some("six-buttons"));
    let base = controls.paths.base_file("six-buttons");
    let machine_only =
        r#"<mameconfig version="10"><system name="sf2"><input/></system></mameconfig>"#;
    fs::write(&base, machine_only).expect("write");
    refused(&controls, "no <system name=\"default\">");
    assert_eq!(fs::read_to_string(&base).expect("base"), machine_only);

    // A base file mapping a device to an assigned joystick index.
    let mut controls = Controls::with_three_pads();
    controls.assign(AssignmentLevel::Default, None, Some("six-buttons"));
    controls.assign_players(AssignmentLevel::Default, None, &[Some("f310")]);
    fs::write(
        controls.paths.base_file("six-buttons"),
        r#"<mameconfig version="10"><system name="default"><input><mapdevice device="XInput Player 1" controller="JOYCODE_1"/></input></system></mameconfig>"#,
    )
    .expect("write");
    refused(&controls, "JOYCODE_1");
}

#[test]
fn registered_devices_are_validated() {
    let mut config = ControllersConfig::new();
    let input = |name: &str, id: &str, binding| ControllerInput {
        name: name.to_owned(),
        kind: ControllerKind::Gamepad,
        device_class: DeviceClass::Joystick,
        device_id: Some(id.to_owned()),
        provider: Some(provider().name().to_owned()),
        binding: Some(binding),
    };
    config
        .create_controller(input("A", PAD_A, ControllerBinding::PortOrder))
        .expect("a");
    // The same device id (without case) twice.
    assert!(
        config
            .create_controller(input(
                "A again",
                &PAD_A.to_uppercase(),
                ControllerBinding::PortOrder
            ))
            .unwrap_err()
            .to_string()
            .contains("same")
    );
    // An id contained in another: MAME could match either.
    assert!(
        config
            .create_controller(input("Prefix", "USB gamepad", ControllerBinding::PortOrder))
            .is_err()
    );
    // A binding needs a provider and a device id.
    let mut incomplete = input("Incomplete", PAD_B, ControllerBinding::PortOrder);
    incomplete.provider = None;
    assert!(
        config
            .create_controller(incomplete)
            .unwrap_err()
            .to_string()
            .contains("not configured")
    );
    // An unknown provider.
    let mut unknown = input("Unknown", PAD_B, ControllerBinding::PortOrder);
    unknown.provider = Some("rawinput".to_owned());
    assert!(config.create_controller(unknown).is_err());
    // An XInput slot never identifies a device.
    if provider() == JoystickProvider::Winhybrid {
        assert!(
            config
                .create_controller(input("Slot", "XInput Player 1", ControllerBinding::Device))
                .unwrap_err()
                .to_string()
                .contains("port order")
        );
        config
            .create_controller(input(
                "Slot",
                "XInput Player 1",
                ControllerBinding::PortOrder,
            ))
            .expect("slot");
    }
    // Controllers registered before device mapping keep loading.
    let mut legacy: ControllersConfig = serde_json::from_value(serde_json::json!({
        "schemaVersion": 1,
        "controllers": [
            { "id": "stick", "name": "Stick", "kind": "arcadeStick", "deviceClass": "joystick", "deviceId": "Stick", "provider": "dinput" },
            { "id": "stick-2", "name": "Stick 2", "kind": "arcadeStick", "deviceClass": "joystick", "deviceId": "Stick", "provider": "dinput" }
        ],
        "assignments": { "default": { "profileId": "mame", "players": ["stick", "stick-2"] } }
    }))
    .expect("legacy");
    legacy.ensure_system_profiles();
    legacy.validate().expect("valid");
    assert_eq!(legacy.controllers[0].binding, None);
    assert!(
        legacy.controllers[0]
            .device_binding()
            .unwrap_err()
            .contains("not configured")
    );
}

#[test]
fn the_mame_profile_composes_the_controller_file_of_mame_ini() {
    let catalog = Catalog::new();
    let mut controls = Controls::with_three_pads();
    controls.assign_players(AssignmentLevel::Default, None, &[Some("f310")]);
    let mame_dir = controls.dir.path().join("MAME ✓");
    fs::create_dir_all(mame_dir.join("ctrlr")).expect("dir");
    let executable = mame_dir.join("mame");
    let native = r#"<?xml version="1.0"?>
<mameconfig version="10">
    <system name="default"><input><port type="UI_PAUSE"><newseq type="standard">KEYCODE_P</newseq></port></input></system>
</mameconfig>
"#;
    fs::write(mame_dir.join("ctrlr").join("xarcade.cfg"), native).expect("ctrlr");
    let prepare = |controls: &Controls| {
        catalog.prepare_at(
            &executable,
            &LaunchConfig::default(),
            &RequiredSearchPaths::default(),
            controls.context(),
            &machine_request("pong"),
        )
    };

    // Without mame.ini: only the mappings.
    let prepared = prepare(&controls).expect("prepare");
    let file = prepared
        .plan
        .controls
        .controller_file
        .clone()
        .expect("file");
    if file.baseline.is_some() {
        // A home `.mame/mame.ini` of the test machine is read first.
        return;
    }

    fs::write(
        mame_dir.join("mame.ini"),
        "ctrlr xarcade\njoystickprovider dinput\n",
    )
    .expect("ini");
    let prepared = prepare(&controls).expect("prepare");
    let file = prepared
        .plan
        .controls
        .controller_file
        .clone()
        .expect("file");
    assert_eq!(
        file.baseline,
        Some(mame_dir.join("ctrlr").join("xarcade.cfg"))
    );
    assert!(file.content.contains("KEYCODE_P"));
    assert!(file.content.contains("<mapdevice"));
    // The configuration directory stays MAME's own.
    assert!(
        !prepared
            .plan
            .command_line()
            .contains(&"-cfg_directory".to_owned())
    );
    if provider().name() != "dinput" {
        assert!(
            prepared
                .warnings
                .iter()
                .any(|warning| warning.contains("mame.ini sets the joystick provider 'dinput'")),
            "{:?}",
            prepared.warnings
        );
    }

    // A controller file named by mame.ini but missing.
    fs::write(mame_dir.join("mame.ini"), "ctrlr missing\n").expect("ini");
    assert!(
        matches!(prepare(&controls), Err(LaunchError::Invalid(message)) if message.contains("'missing'"))
    );

    // A controller file set by another INI file cannot be composed safely.
    fs::write(mame_dir.join("mame.ini"), "ctrlr xarcade\n").expect("ini");
    fs::create_dir_all(mame_dir.join("ini")).expect("dir");
    fs::write(mame_dir.join("ini").join("sf2.ini"), "ctrlr hotrod\n").expect("ini");
    assert!(
        matches!(prepare(&controls), Err(LaunchError::Invalid(message)) if message.contains("sf2.ini"))
    );
    // Without mapped players, those INI files are MAME's business.
    controls.assign_players(AssignmentLevel::Default, None, &[]);
    assert_eq!(
        prepare(&controls).expect("prepare").plan.command_line(),
        ["pong"]
    );
}
