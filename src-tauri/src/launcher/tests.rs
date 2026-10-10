//! Launch preparation tests on a catalog imported from fixtures. Nothing here
//! starts MAME.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use rusqlite::Connection;
use tempfile::TempDir;

use super::command::ArgumentRole;
use super::config::{LaunchConfig, OptionSource, Profile, software_key};
use super::options::{OptionSet, OptionValue};
use super::resolve::{ListRelation, MediaAssignment, SoftwareLaunchInfo, software_launch_info};
use super::{LaunchError, LaunchRequest, PreparedLaunch, SoftwareSelection, prepare};
use crate::catalog::import::{ImportProgress, import_from_reader, import_software_from_reader};
use crate::catalog::store::{CatalogSource, CatalogStore};

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
        self.read(|connection| prepare(connection, &executable(), config, request))
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
