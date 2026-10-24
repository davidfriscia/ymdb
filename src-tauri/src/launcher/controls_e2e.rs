//! End-to-end test of controller profiles with a real MAME executable,
//! through the same path as a launch: `prepare`, `controls::materialize`,
//! `SessionManager::start`. Ignored by default.
//!
//! Nothing of the real installation is written: the test builds a temporary
//! installation (a hard link to the executable, or a copy, with its own
//! `mame.ini`, `cfg` and `ctrlr` directories) that reads the real ROM and
//! hash directories. A Lua script set in that `mame.ini` prints the effective
//! sequence of `P1_BUTTON1` and exits MAME normally, so MAME saves its
//! configuration as after a user session.
//!
//! Run with:
//! `YMDB_E2E_MAME=<path to mame> cargo test controller_profiles_end_to_end -- --ignored --nocapture`

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant, SystemTime};

use super::config::LaunchConfig;
use super::controls::{self, ControlsContext};
use super::resolve::MediaAssignment;
use super::search_paths::RequiredSearchPaths;
use super::session::{SessionManager, SessionState};
use super::{LaunchRequest, PreparedLaunch, SoftwareSelection, prepare};
use crate::catalog::import::{ImportProgress, import_catalog, import_software_lists};
use crate::catalog::store::CatalogStore;
use crate::controllers::model::{Assignment, AssignmentLevel, AssignmentTarget, ControllersConfig};
use crate::controllers::store::ControllersStore;

const INSTALLATION_ID: &str = "0123456789abcdef0123456789abcdef";

const PROBE: &str = r#"
local ioport = manager.machine.ioport
local t, p = ioport:token_to_input_type("P1_BUTTON1")
print("PROBE general=" .. manager.machine.input:seq_to_tokens(ioport:type_seq(t, p)))
manager.machine:exit()
"#;

fn controller_file(code: &str) -> String {
    format!(
        r#"<?xml version="1.0"?>
<mameconfig version="10">
    <system name="default">
        <input>
            <port type="P1_BUTTON1">
                <newseq type="standard">{code}</newseq>
            </port>
        </input>
    </system>
</mameconfig>
"#
    )
}

/// Files under `dir` with their size and modification time.
fn snapshot(dir: &Path) -> BTreeMap<PathBuf, (u64, Option<SystemTime>)> {
    let mut files = BTreeMap::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return files;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if metadata.is_dir() {
            files.extend(snapshot(&path));
        } else {
            files.insert(path, (metadata.len(), metadata.modified().ok()));
        }
    }
    files
}

struct Bench {
    executable: PathBuf,
    store: CatalogStore,
    controllers: ControllersStore,
    sessions: SessionManager,
}

impl Bench {
    /// Prepares like `preview_launch`: nothing may be created.
    fn prepare(&self, config: &ControllersConfig, request: &LaunchRequest) -> PreparedLaunch {
        let controls = ControlsContext {
            config,
            paths: self.controllers.paths(),
        };
        self.store
            .with_reader(|connection| {
                Ok(prepare(
                    connection,
                    &self.executable,
                    &LaunchConfig::default(),
                    &RequiredSearchPaths::default(),
                    controls,
                    request,
                ))
            })
            .expect("reader")
            .expect("catalog")
            .expect("prepare")
    }

    /// Starts like `start_launch` and waits for MAME to exit; returns the
    /// end of its output.
    fn run(&self, config: &ControllersConfig, request: &LaunchRequest) -> (PreparedLaunch, String) {
        let prepared = self.prepare(config, request);
        controls::materialize(&prepared.plan.controls).expect("materialize");
        let session = self
            .sessions
            .start(
                prepared.target.clone(),
                prepared.title.clone(),
                "MAME e2e".to_owned(),
                &prepared.plan,
            )
            .expect("start");
        let deadline = Instant::now() + Duration::from_secs(120);
        loop {
            let info = self
                .sessions
                .list()
                .into_iter()
                .find(|info| info.id == session.id)
                .expect("session");
            match info.state {
                SessionState::Running if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(100));
                }
                SessionState::Exited {
                    success: true,
                    log_tail,
                    ..
                } => {
                    println!("{:?}\n{log_tail}", prepared.plan.command_line());
                    return (prepared, log_tail);
                }
                other => panic!("{:?}: {other:?}", prepared.plan.command_line()),
            }
        }
    }
}

fn request(machine: &str, profile: Option<&str>) -> LaunchRequest {
    LaunchRequest {
        installation_id: INSTALLATION_ID.to_owned(),
        machine: machine.to_owned(),
        software: None,
        profile_id: None,
        controls: profile.map(|profile| Assignment {
            profile_id: Some(profile.to_owned()),
            players: None,
        }),
    }
}

fn probe(log: &str) -> &str {
    log.lines()
        .find_map(|line| line.trim().strip_prefix("PROBE general="))
        .unwrap_or_else(|| panic!("no probe output in:\n{log}"))
}

#[test]
#[ignore = "requires a MAME executable in YMDB_E2E_MAME"]
#[allow(clippy::too_many_lines)]
fn controller_profiles_end_to_end() {
    let real = PathBuf::from(std::env::var("YMDB_E2E_MAME").expect("YMDB_E2E_MAME must be set"));
    let real_dir = real.parent().expect("MAME directory").to_path_buf();
    let real_before = snapshot(&real_dir);
    let work = tempfile::tempdir().expect("temp dir");

    // A temporary installation that only reads the real ROM and hash files.
    let mame_dir = work.path().join("MAME ✓");
    fs::create_dir_all(mame_dir.join("ctrlr")).expect("dir");
    let executable = mame_dir.join(real.file_name().expect("file name"));
    if fs::hard_link(&real, &executable).is_err() {
        fs::copy(&real, &executable).expect("copy MAME");
    }
    fs::write(
        mame_dir.join("ctrlr").join("testx.cfg"),
        controller_file("KEYCODE_X"),
    )
    .expect("ctrlr");
    fs::write(mame_dir.join("probe.lua"), PROBE).expect("probe");
    fs::write(
        mame_dir.join("mame.ini"),
        format!(
            "rompath {}\nhashpath {}\nctrlr testx\nvideo none\nsound none\nskip_gameinfo 1\nautoboot_script probe.lua\n",
            real_dir.join("roms").display(),
            real_dir.join("hash").display()
        ),
    )
    .expect("mame.ini");

    let store = CatalogStore::in_dir(&work.path().join("catalog"));
    let cancel = Arc::new(AtomicBool::new(false));
    import_catalog(&executable, &store, &cancel, &ImportProgress::default()).expect("machines");
    import_software_lists(&executable, &store, &cancel, &ImportProgress::default())
        .expect("software lists");

    let controllers = ControllersStore::in_dir(&work.path().join("config ✓"));
    let paths = controllers.paths().clone();
    let mut config = ControllersConfig::new();
    let fighting = config
        .create_profile("Fighting", false, |_| false)
        .expect("fighting");
    let six = config
        .create_profile("Six Buttons", true, |_| false)
        .expect("six");
    let base = controller_file("KEYCODE_V");
    controllers.write_base(&six, base.as_bytes()).expect("base");
    controllers.save(&config).expect("save");
    let bench = Bench {
        executable: executable.clone(),
        store,
        controllers,
        sessions: SessionManager::new(Some(work.path().join("logs"))),
    };
    let native_cfg = mame_dir.join("cfg");
    let fighting_cfg = paths.context_dir(INSTALLATION_ID, &fighting);
    let six_cfg = paths.context_dir(INSTALLATION_ID, &six);
    let ini_before = fs::read(mame_dir.join("mame.ini")).expect("ini");

    // 1. The `mame` profile: MAME's own configuration, ctrlr from mame.ini.
    let (prepared, log) = bench.run(&config, &request("turfmast", None));
    assert_eq!(prepared.plan.command_line(), ["turfmast"]);
    assert_eq!(probe(&log), "KEYCODE_X");
    assert!(native_cfg.join("turfmast.cfg").is_file());
    assert!(!fighting_cfg.exists() && !six_cfg.exists());
    let native_after_first = snapshot(&native_cfg);

    // 2. A profile without base: the preview creates nothing; the launch
    // creates its directory, where MAME saves; mame.ini's ctrlr is ignored.
    let default = AssignmentTarget {
        level: AssignmentLevel::Default,
        key: None,
    };
    config
        .set_assignment(
            &default,
            Assignment {
                profile_id: Some(fighting.clone()),
                players: None,
            },
        )
        .expect("assign");
    let before_preview = snapshot(work.path());
    let preview = bench.prepare(&config, &request("turfmast", None));
    assert_eq!(
        preview.plan.command_line().last().map(String::as_str),
        Some("")
    );
    assert_eq!(snapshot(work.path()), before_preview);
    assert!(!fighting_cfg.exists());
    let (_, log) = bench.run(&config, &request("turfmast", None));
    // MAME's default; it names the first joystick only when one is connected.
    assert_eq!(
        probe(&log).trim_end_matches(" OR JOYCODE_1_BUTTON1"),
        "KEYCODE_LCONTROL OR MOUSECODE_1_BUTTON1 OR GUNCODE_1_BUTTON1"
    );
    assert!(fighting_cfg.join("default.cfg").is_file());
    assert!(fighting_cfg.join("turfmast.cfg").is_file());
    assert_eq!(snapshot(&native_cfg), native_after_first);

    // 3. A change saved by MAME in the profile (written here in MAME's
    // format, as the Input Assignments menu would) is used by the next launch
    // with the profile and kept when MAME saves again.
    let general = fs::read_to_string(fighting_cfg.join("default.cfg")).expect("default.cfg");
    let customized = general.replace(
        "<system name=\"default\">",
        "<system name=\"default\">\n        <input>\n            <port type=\"P1_BUTTON1\">\n                <newseq type=\"standard\">KEYCODE_Z</newseq>\n            </port>\n        </input>",
    );
    assert_ne!(customized, general, "{general}");
    fs::write(fighting_cfg.join("default.cfg"), customized).expect("customize");
    let (_, log) = bench.run(&config, &request("turfmast", None));
    assert_eq!(probe(&log), "KEYCODE_Z");

    // 4. Another profile, with a base file: its own directory and baseline.
    let (prepared, log) = bench.run(&config, &request("turfmast", Some(&six)));
    assert_eq!(probe(&log), "KEYCODE_V");
    assert_eq!(
        prepared.plan.command_line()[prepared.plan.command_line().len() - 2..],
        ["-ctrlr", "base"]
    );
    assert!(six_cfg.join("turfmast.cfg").is_file());

    // 5. Back to the first profile: its change is still there.
    let (_, log) = bench.run(&config, &request("turfmast", None));
    assert_eq!(probe(&log), "KEYCODE_Z");
    assert!(
        fs::read_to_string(fighting_cfg.join("default.cfg"))
            .expect("default.cfg")
            .contains("KEYCODE_Z")
    );

    // 6. A software list launch, with a software assignment.
    config
        .set_assignment(
            &AssignmentTarget {
                level: AssignmentLevel::Software,
                key: Some("nes:smb".to_owned()),
            },
            Assignment {
                profile_id: Some(six.clone()),
                players: None,
            },
        )
        .expect("assign");
    let mut nes = request("nes", None);
    nes.software = Some(SoftwareSelection {
        list: "nes".to_owned(),
        name: "smb".to_owned(),
        media: vec![MediaAssignment {
            device: "cart".to_owned(),
            part: "cart".to_owned(),
        }],
    });
    let (prepared, log) = bench.run(&config, &nes);
    assert_eq!(prepared.plan.controls.profile_id, six);
    assert_eq!(probe(&log), "KEYCODE_V");
    assert!(six_cfg.join("nes.cfg").is_file());
    assert!(!fighting_cfg.join("nes.cfg").exists());

    // YMDB wrote none of MAME's files: the base file and the temporary
    // installation's own files are unchanged, the real installation too.
    assert_eq!(
        fs::read_to_string(paths.base_file(&six)).expect("base"),
        base
    );
    assert_eq!(
        fs::read(mame_dir.join("mame.ini")).expect("ini"),
        ini_before
    );
    assert_eq!(snapshot(&native_cfg), native_after_first);
    assert_eq!(snapshot(&real_dir), real_before);
}

/// Changes `P1_BUTTON1` like MAME's Input Assignments menus would: the general
/// assignment (`general`) and the one of the running machine (`system`),
/// then exits normally so that MAME saves them. With `None`, only prints the
/// effective sequences.
fn menu_script(change: Option<(&str, &str)>) -> String {
    let change = change.map_or_else(String::new, |(general, system)| {
        format!(
            r#"ioport:set_type_seq(t, p, "standard", input:seq_from_tokens("{general}"))
field:set_input_seq("standard", input:seq_from_tokens("{system}"))
"#
        )
    });
    format!(
        r#"
local ioport = manager.machine.ioport
local input = manager.machine.input
local t, p = ioport:token_to_input_type("P1_BUTTON1")
local field
for _, port in pairs(ioport.ports) do
    for _, candidate in pairs(port.fields) do
        if candidate.type == t and candidate.player == p then field = candidate end
    end
end
{change}print("PROBE general=" .. input:seq_to_tokens(ioport:type_seq(t, p)))
print("PROBE system=" .. input:seq_to_tokens(field:input_seq("standard")))
manager.machine:exit()
"#
    )
}

/// The effective general and machine sequences printed by [`menu_script`].
fn probes(log: &str) -> (String, String) {
    let find = |prefix: &str| {
        log.lines()
            .find_map(|line| line.trim().strip_prefix(prefix).map(str::to_owned))
            .unwrap_or_else(|| panic!("no {prefix} in:\n{log}"))
    };
    (find("PROBE general="), find("PROBE system="))
}

/// The workflow of the Controllers settings and of Configure Controls in
/// MAME, through the commands' state (as the UI uses it) and a real MAME:
/// input changes made in MAME persist per controller profile, a temporary
/// selection never changes the saved preferences, and removing a baseline
/// file keeps what MAME saved.
///
/// The changes are made by a Lua script calling the same `ioport` functions
/// as MAME's menus, then MAME exits normally and saves them itself; pressing
/// keys in MAME's menus by hand remains a manual check.
#[test]
#[ignore = "requires a MAME executable in YMDB_E2E_MAME"]
#[allow(clippy::too_many_lines)]
fn controller_profile_changes_made_in_mame_persist_per_profile() {
    use crate::commands::controllers::{ControllerFile, ControllersState};
    use crate::controllers::model::{AssignmentUpdate, FieldUpdate};

    let real = PathBuf::from(std::env::var("YMDB_E2E_MAME").expect("YMDB_E2E_MAME must be set"));
    let real_dir = real.parent().expect("MAME directory").to_path_buf();
    let real_before = snapshot(&real_dir);
    let work = tempfile::tempdir().expect("temp dir");

    let mame_dir = work.path().join("MAME ✓");
    fs::create_dir_all(&mame_dir).expect("dir");
    let executable = mame_dir.join(real.file_name().expect("file name"));
    if fs::hard_link(&real, &executable).is_err() {
        fs::copy(&real, &executable).expect("copy MAME");
    }
    let script = mame_dir.join("probe.lua");
    fs::write(
        mame_dir.join("mame.ini"),
        format!(
            "rompath {}\nvideo none\nsound none\nskip_gameinfo 1\nautoboot_script probe.lua\n",
            real_dir.join("roms").display(),
        ),
    )
    .expect("mame.ini");
    let store = CatalogStore::in_dir(&work.path().join("catalog"));
    let cancel = Arc::new(AtomicBool::new(false));
    import_catalog(&executable, &store, &cancel, &ImportProgress::default()).expect("machines");

    // 1. Profiles created as Settings → Controllers does.
    let config_dir = work.path().join("config ✓");
    let state = ControllersState::new(Some(&config_dir));
    let arcade = state.create_profile("Arcade", None).expect("arcade");
    let pad = state
        .create_profile(
            "Pad",
            Some(&ControllerFile {
                content: controller_file("KEYCODE_V").into_bytes(),
                file_name: Some("pad.cfg".to_owned()),
            }),
        )
        .expect("pad");
    // 2. Arcade becomes the preference of the machine.
    let turfmast = AssignmentTarget {
        level: AssignmentLevel::Machine,
        key: Some("turfmast".to_owned()),
    };
    state
        .update(|config| {
            Ok(config.update_assignment(
                &turfmast,
                AssignmentUpdate {
                    profile_id: FieldUpdate::Set {
                        value: arcade.clone(),
                    },
                    players: FieldUpdate::Keep,
                },
            )?)
        })
        .expect("machine preference");
    let bench = Bench {
        executable,
        store,
        controllers: ControllersStore::in_dir(&config_dir),
        sessions: SessionManager::new(Some(work.path().join("logs"))),
    };
    let paths = bench.controllers.paths().clone();
    let run = |change: Option<(&str, &str)>, temporary: Option<&str>| {
        fs::write(&script, menu_script(change)).expect("script");
        let (config, _) = state.snapshot().expect("snapshot");
        let (prepared, log) = bench.run(&config, &request("turfmast", temporary));
        (prepared, probes(&log))
    };
    let saved = || state.snapshot().expect("snapshot").0;
    let preferences = saved();

    // 3-5. Launch, change a button in MAME, exit normally.
    let (prepared, _) = run(Some(("KEYCODE_Q", "KEYCODE_Z")), None);
    assert_eq!(prepared.plan.controls.profile_id, arcade);
    let arcade_cfg = paths.context_dir(INSTALLATION_ID, &arcade);
    assert!(
        fs::read_to_string(arcade_cfg.join("turfmast.cfg"))
            .expect("turfmast.cfg")
            .contains("KEYCODE_Z")
    );
    // 6-7. Launch again with the same profile: the changes are there.
    let (_, effective) = run(None, None);
    assert_eq!(effective, ("KEYCODE_Q".to_owned(), "KEYCODE_Z".to_owned()));

    // 8-9. A second profile, chosen temporarily: its own baseline and
    // directory; then change its configuration.
    let (prepared, effective) = run(None, Some(&pad));
    assert_eq!(prepared.plan.controls.profile_id, pad);
    assert_eq!(effective, ("KEYCODE_V".to_owned(), "KEYCODE_V".to_owned()));
    run(Some(("KEYCODE_W", "KEYCODE_E")), Some(&pad));
    // The temporary selection saved nothing.
    assert_eq!(saved(), preferences);

    // 10-11. Back to the first profile: its configuration is unchanged.
    let (_, effective) = run(None, None);
    assert_eq!(effective, ("KEYCODE_Q".to_owned(), "KEYCODE_Z".to_owned()));
    let (_, effective) = run(None, Some(&pad));
    assert_eq!(effective, ("KEYCODE_W".to_owned(), "KEYCODE_E".to_owned()));

    // Removing Pad's baseline keeps what MAME saved in its directory.
    let pad_cfg = paths.context_dir(INSTALLATION_ID, &pad);
    let pad_saved = snapshot(&pad_cfg);
    state.remove_base(&pad).expect("remove base");
    assert!(!paths.base_file(&pad).exists());
    assert_eq!(snapshot(&pad_cfg), pad_saved);
    let (prepared, effective) = run(None, Some(&pad));
    assert!(
        !prepared
            .plan
            .command_line()
            .contains(&"-ctrlrpath".to_owned())
    );
    assert_eq!(effective, ("KEYCODE_W".to_owned(), "KEYCODE_E".to_owned()));

    // MAME's own configuration directory was never used, and nothing of the
    // real installation changed.
    assert!(!mame_dir.join("cfg").exists());
    assert_eq!(snapshot(&real_dir), real_before);
}

// --- Player mappings with real devices --------------------------------------
//
// `player_mappings_end_to_end` detects the joysticks connected to this
// computer with the real MAME (`discovery`), registers them in a temporary
// configuration and checks every launch path with `pong` (no ROM file): the
// command lines, the generated controller file, MAME's own report of the
// final joystick order (a Lua probe reading `devindex`), the verification
// from the verbose output and the files written. It needs at least two
// joysticks. It does not press buttons: that check is done by hand.
//
// Run with:
// `YMDB_E2E_MAME=<path to mame> cargo test player_mappings_end_to_end -- --ignored --nocapture`

use crate::controllers::devices::JoystickProvider;
use crate::controllers::discovery;
use crate::controllers::model::{ControllerBinding, ControllerInput, ControllerKind, DeviceClass};
use crate::launcher::mapping_check::MappingStatus;
use crate::launcher::session::SessionInfo;

/// Prints the final joystick order and the general `P1_BUTTON1`, then exits.
const ORDER_PROBE: &str = r#"
local class = manager.machine.input.device_classes["joystick"]
for _, device in pairs(class.devices) do
  print("PROBE joystick " .. (device.devindex + 1) .. "=" .. device.id)
end
local ioport = manager.machine.ioport
local t, p = ioport:token_to_input_type("P1_BUTTON1")
print("PROBE general=" .. manager.machine.input:seq_to_tokens(ioport:type_seq(t, p)))
manager.machine:exit()
"#;

/// The device id MAME reports at joystick `index` (1-based).
fn joystick_at(log: &str, index: usize) -> Option<String> {
    let prefix = format!("PROBE joystick {index}=");
    log.lines()
        .find_map(|line| line.trim().strip_prefix(&prefix).map(str::to_owned))
}

impl Bench {
    /// Starts like `start_launch`, waits for MAME to exit and for the
    /// verification of the mappings; returns the session and its output.
    fn run_session(
        &self,
        config: &ControllersConfig,
        request: &LaunchRequest,
    ) -> (PreparedLaunch, SessionInfo, String) {
        let prepared = self.prepare(config, request);
        controls::materialize(&prepared.plan.controls).expect("materialize");
        let started = self
            .sessions
            .start(
                prepared.target.clone(),
                prepared.title.clone(),
                "MAME e2e".to_owned(),
                &prepared.plan,
            )
            .expect("start");
        let deadline = Instant::now() + Duration::from_secs(120);
        loop {
            let info = self
                .sessions
                .list()
                .into_iter()
                .find(|info| info.id == started.id)
                .expect("session");
            match &info.state {
                SessionState::Running if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(100));
                }
                SessionState::Exited {
                    success: true,
                    log_tail,
                    ..
                } => {
                    println!("{:?}\n{log_tail}", prepared.plan.command_line());
                    let log = log_tail.clone();
                    return (prepared, info, log);
                }
                other => panic!("{:?}: {other:?}", prepared.plan.command_line()),
            }
        }
    }
}

#[test]
#[ignore = "requires a MAME executable in YMDB_E2E_MAME and two connected joysticks"]
#[allow(clippy::too_many_lines)]
fn player_mappings_end_to_end() {
    let real = PathBuf::from(std::env::var("YMDB_E2E_MAME").expect("YMDB_E2E_MAME must be set"));
    let real_dir = real.parent().expect("MAME directory").to_path_buf();
    let real_before = snapshot(&real_dir);
    let work = tempfile::tempdir().expect("temp dir");
    let mame_dir = work.path().join("MAME ✓");
    fs::create_dir_all(mame_dir.join("ctrlr")).expect("dir");
    let executable = mame_dir.join(real.file_name().expect("file name"));
    if fs::hard_link(&real, &executable).is_err() {
        fs::copy(&real, &executable).expect("copy MAME");
    }
    // A native controller file in mame.ini, to compose with the `mame` profile.
    fs::write(
        mame_dir.join("ctrlr").join("testx.cfg"),
        controller_file("KEYCODE_X"),
    )
    .expect("ctrlr");
    fs::write(mame_dir.join("probe.lua"), ORDER_PROBE).expect("probe");
    fs::write(
        mame_dir.join("mame.ini"),
        "ctrlr testx\nvideo none\nsound none\nskip_gameinfo 1\nautoboot_script probe.lua\n",
    )
    .expect("mame.ini");
    let ini_before = fs::read(mame_dir.join("mame.ini")).expect("ini");

    // 0. Detection: on request, in its own temporary directory.
    let provider = JoystickProvider::available()[0];
    let detect_root = work.path().join("detect");
    fs::create_dir_all(&detect_root).expect("dir");
    let output = discovery::run(&executable, provider, &detect_root).expect("detection");
    assert_eq!(fs::read_dir(&detect_root).expect("read").count(), 0);
    let (detected, _) =
        discovery::interpret(&output, provider, &ControllersConfig::new()).expect("interpreted");
    for device in &detected {
        println!(
            "detected #{} {} [{:?}, distinguishable: {}] {}",
            device.index, device.name, device.identity, device.distinguishable, device.device_id
        );
    }
    let usable: Vec<_> = detected
        .iter()
        .filter(|device| device.distinguishable)
        .collect();
    assert!(
        usable.len() >= 2,
        "connect at least two joysticks: {detected:?}"
    );

    let store = CatalogStore::in_dir(&work.path().join("catalog"));
    let cancel = Arc::new(AtomicBool::new(false));
    import_catalog(&executable, &store, &cancel, &ImportProgress::default()).expect("machines");
    let controllers = ControllersStore::in_dir(&work.path().join("config ✓"))
        .with_data_dir(&work.path().join("data ✓"));
    let paths = controllers.paths().clone();
    let mut config = ControllersConfig::new();
    let ids: Vec<String> = usable
        .iter()
        .enumerate()
        .map(|(index, device)| {
            config
                .create_controller(ControllerInput {
                    name: format!("Pad {} {}", index + 1, device.name),
                    kind: ControllerKind::Gamepad,
                    device_class: DeviceClass::Joystick,
                    device_id: Some(device.device_id.clone()),
                    provider: Some(provider.name().to_owned()),
                    binding: device.suggested_binding,
                })
                .expect("controller")
        })
        .collect();
    let profile = config
        .create_profile("Arcade", true, |_| false)
        .expect("profile");
    controllers
        .write_base(&profile, controller_file("KEYCODE_V").as_bytes())
        .expect("base");
    controllers.save(&config).expect("save");
    let bench = Bench {
        executable: executable.clone(),
        store,
        controllers,
        sessions: SessionManager::new(Some(work.path().join("logs"))),
    };
    let default = AssignmentTarget {
        level: AssignmentLevel::Default,
        key: None,
    };
    let assign = |config: &mut ControllersConfig, players: Vec<Option<String>>| {
        config
            .update_assignment(
                &default,
                crate::controllers::model::AssignmentUpdate {
                    players: crate::controllers::model::FieldUpdate::Set { value: players },
                    ..Default::default()
                },
            )
            .expect("players");
    };
    let pong = request("pong", None);

    // 1. The `mame` profile without mappings: the previous command line.
    let (prepared, info, log) = bench.run_session(&config, &pong);
    assert_eq!(prepared.plan.command_line(), ["pong"]);
    assert_eq!(info.mapping, None);
    assert_eq!(probe(&log), "KEYCODE_X");
    let native_cfg = mame_dir.join("cfg");
    assert!(native_cfg.join("pong.cfg").is_file());

    // 2. The `mame` profile with mappings, in reverse detection order: the
    // native controller file is kept, MAME's directory stays in charge.
    let reversed: Vec<Option<String>> = ids.iter().rev().cloned().map(Some).collect();
    assign(&mut config, reversed.clone());
    let before_preview = snapshot(work.path());
    let preview = bench.prepare(&config, &pong);
    assert_eq!(
        snapshot(work.path()),
        before_preview,
        "the preview wrote files"
    );
    let file = preview.plan.controls.controller_file.clone().expect("file");
    assert_eq!(
        file.baseline,
        Some(mame_dir.join("ctrlr").join("testx.cfg"))
    );
    let line = preview.plan.command_line();
    assert_eq!(line[0], "pong");
    assert!(!line.contains(&"-cfg_directory".to_owned()));
    let provider_at = line
        .iter()
        .position(|arg| arg == "-joystickprovider")
        .expect("provider");
    assert_eq!(line[provider_at + 1], provider.name());
    let (prepared, info, log) = bench.run_session(&config, &pong);
    assert_eq!(prepared.plan, preview.plan);
    assert_eq!(
        fs::read_to_string(&file.path).expect("generated"),
        file.content
    );
    assert_eq!(probe(&log), "KEYCODE_X");
    let verification = info.mapping.expect("verification");
    assert!(verification.complete, "{verification:?}");
    for (index, id) in ids.iter().rev().enumerate() {
        let device = &config.controller(id).expect("controller").device_id;
        assert_eq!(
            joystick_at(&log, index + 1).as_deref(),
            device.as_deref(),
            "MAME's joystick {}",
            index + 1
        );
        let player = &verification.players[index];
        assert!(
            matches!(
                player.status,
                MappingStatus::Verified | MappingStatus::VerifiedPortOrder
            ),
            "{player:?}"
        );
    }

    // 3. A managed profile with its base and the same mappings.
    let mut arcade = pong.clone();
    arcade.controls = Some(Assignment {
        profile_id: Some(profile.clone()),
        players: None,
    });
    let (prepared, info, log) = bench.run_session(&config, &arcade);
    assert_eq!(probe(&log), "KEYCODE_V");
    assert_eq!(
        prepared
            .plan
            .controls
            .controller_file
            .as_ref()
            .and_then(|f| f.baseline.clone()),
        Some(paths.base_file(&profile))
    );
    assert!(
        info.mapping
            .expect("verification")
            .players
            .iter()
            .all(|player| matches!(
                player.status,
                MappingStatus::Verified | MappingStatus::VerifiedPortOrder
            ))
    );
    let arcade_cfg = paths.context_dir(INSTALLATION_ID, &profile);
    // 11. MAME saved its configuration in the profile's directory.
    assert!(arcade_cfg.join("default.cfg").is_file());
    assert!(arcade_cfg.join("pong.cfg").is_file());
    assert_eq!(
        joystick_at(&log, 1).as_deref(),
        config
            .controller(ids.last().expect("id"))
            .and_then(|c| c.device_id.as_deref())
    );

    // 12. A temporary override for one launch only: detection order, Player 2 first.
    let mut temporary = pong.clone();
    temporary.controls = Some(Assignment {
        profile_id: None,
        players: Some(vec![None, Some(ids[0].clone())]),
    });
    let (prepared, info, log) = bench.run_session(&config, &temporary);
    assert_eq!(prepared.plan.controls.mappings.len(), 1);
    assert_eq!(
        joystick_at(&log, 2).as_deref(),
        config
            .controller(&ids[0])
            .and_then(|c| c.device_id.as_deref())
    );
    assert_eq!(info.mapping.expect("verification").players.len(), 1);
    assert_eq!(
        config.assignment(&default).and_then(|a| a.players.clone()),
        Some(reversed.clone())
    );

    // 9. A registered device that is not connected: reported missing.
    let missing = config
        .create_controller(ControllerInput {
            name: "Disconnected pad".to_owned(),
            kind: ControllerKind::Gamepad,
            device_class: DeviceClass::Joystick,
            device_id: Some("YMDB disconnected pad product_00000000 instance_00000000".to_owned()),
            provider: Some(provider.name().to_owned()),
            binding: Some(ControllerBinding::Device),
        })
        .expect("missing");
    let mut with_missing = pong.clone();
    with_missing.controls = Some(Assignment {
        profile_id: None,
        players: Some(vec![Some(missing), Some(ids[0].clone())]),
    });
    let (_, info, _) = bench.run_session(&config, &with_missing);
    let statuses: Vec<_> = info
        .mapping
        .expect("verification")
        .players
        .iter()
        .map(|player| player.status)
        .collect();
    assert_eq!(statuses[0], MappingStatus::Missing);
    assert!(matches!(
        statuses[1],
        MappingStatus::Verified | MappingStatus::VerifiedPortOrder
    ));

    // 10. A base file with sections for one machine only is refused before
    // MAME runs, and kept as it is.
    let machine_only =
        r#"<mameconfig version="10"><system name="sf2"><input/></system></mameconfig>"#;
    fs::write(paths.base_file(&profile), machine_only).expect("write");
    let refused = bench.store.with_reader(|connection| {
        Ok(prepare(
            connection,
            &executable,
            &LaunchConfig::default(),
            &RequiredSearchPaths::default(),
            ControlsContext {
                config: &config,
                paths: &paths,
            },
            &arcade,
        ))
    });
    let error = refused
        .expect("reader")
        .expect("catalog")
        .unwrap_err()
        .to_string();
    assert!(error.contains("default"), "{error}");
    assert_eq!(
        fs::read_to_string(paths.base_file(&profile)).expect("base"),
        machine_only
    );

    // 13. Back to no players: the first command line again, nothing generated.
    assign(&mut config, Vec::new());
    let (prepared, info, log) = bench.run_session(&config, &pong);
    assert_eq!(prepared.plan.command_line(), ["pong"]);
    assert_eq!(info.mapping, None);
    assert_eq!(probe(&log), "KEYCODE_X");

    // YMDB wrote none of MAME's files and nothing of the real installation.
    assert_eq!(
        fs::read(mame_dir.join("mame.ini")).expect("ini"),
        ini_before
    );
    assert_eq!(
        fs::read_to_string(mame_dir.join("ctrlr").join("testx.cfg")).expect("ctrlr"),
        controller_file("KEYCODE_X")
    );
    assert_eq!(snapshot(&real_dir), real_before);
}

// --- Manual hardware check ---------------------------------------------------
//
// `player_mappings_hardware_check` launches `pong` in a window through the
// same path as Play (detection, registration, assignment, `prepare`,
// `materialize`, session) twice: the detected joysticks assigned in reverse
// detection order, then with Players 2 and 3 swapped. During each launch a
// Lua script prints `PRESS JOYCODE_<n>_BUTTON1` for every press of button 1,
// so pressing the button of each physical controller in turn shows the
// player it got. It needs a person at the controllers.
//
// Run with:
// `YMDB_E2E_MAME=<path to mame> cargo test player_mappings_hardware_check -- --ignored --nocapture`

/// Logs the first button of joysticks 1 to 4 and exits after `seconds`.
fn press_logger(seconds: u32) -> String {
    format!(
        r#"
local input = manager.machine.input
local codes = {{}}
for n = 1, 4 do codes[n] = input:code_from_token("JOYCODE_" .. n .. "_BUTTON1") end
local held = {{}}
local frames = 0
print("PRESS ready")
subscription = emu.add_machine_frame_notifier(function()
  frames = frames + 1
  for n = 1, 4 do
    local pressed = input:code_pressed(codes[n])
    if pressed and not held[n] then print("PRESS JOYCODE_" .. n .. "_BUTTON1") end
    held[n] = pressed
  end
  if frames > 60 * {seconds} then manager.machine:exit() end
end)
"#
    )
}

#[test]
#[ignore = "requires YMDB_E2E_MAME, connected joysticks and a person pressing their buttons"]
fn player_mappings_hardware_check() {
    let real = PathBuf::from(std::env::var("YMDB_E2E_MAME").expect("YMDB_E2E_MAME must be set"));
    let seconds: u32 = std::env::var("YMDB_E2E_SECONDS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(30);
    let work = tempfile::tempdir().expect("temp dir");
    let mame_dir = work.path().join("MAME");
    fs::create_dir_all(&mame_dir).expect("dir");
    let executable = mame_dir.join(real.file_name().expect("file name"));
    if fs::hard_link(&real, &executable).is_err() {
        fs::copy(&real, &executable).expect("copy MAME");
    }
    fs::write(mame_dir.join("press.lua"), press_logger(seconds)).expect("script");
    fs::write(
        mame_dir.join("mame.ini"),
        "window 1\nsound none\nskip_gameinfo 1\nautoboot_script press.lua\n",
    )
    .expect("mame.ini");

    let provider = JoystickProvider::available()[0];
    let detect_root = work.path().join("detect");
    fs::create_dir_all(&detect_root).expect("dir");
    let output = discovery::run(&executable, provider, &detect_root).expect("detection");
    let (detected, _) =
        discovery::interpret(&output, provider, &ControllersConfig::new()).expect("interpreted");
    let usable: Vec<_> = detected
        .iter()
        .filter(|device| device.distinguishable)
        .collect();
    assert!(usable.len() >= 2, "connect at least two joysticks");

    let store = CatalogStore::in_dir(&work.path().join("catalog"));
    let cancel = Arc::new(AtomicBool::new(false));
    import_catalog(&executable, &store, &cancel, &ImportProgress::default()).expect("machines");
    let controllers = ControllersStore::in_dir(&work.path().join("config"))
        .with_data_dir(&work.path().join("data"));
    let mut config = ControllersConfig::new();
    let ids: Vec<String> = usable
        .iter()
        .map(|device| {
            config
                .create_controller(ControllerInput {
                    name: format!("Joystick {} {}", device.index, device.name),
                    kind: ControllerKind::Gamepad,
                    device_class: DeviceClass::Joystick,
                    device_id: Some(device.device_id.clone()),
                    provider: Some(provider.name().to_owned()),
                    binding: device.suggested_binding,
                })
                .expect("controller")
        })
        .collect();
    let bench = Bench {
        executable,
        store,
        controllers,
        sessions: SessionManager::new(Some(work.path().join("logs"))),
    };
    let reversed: Vec<Option<String>> = ids.iter().rev().cloned().map(Some).collect();
    let mut swapped = reversed.clone();
    let last = swapped.len() - 1;
    swapped.swap(1.min(last), 2.min(last));
    for (round, players) in [("first", &reversed), ("second", &swapped)] {
        let mut pong = request("pong", None);
        pong.controls = Some(Assignment {
            profile_id: None,
            players: Some(players.clone()),
        });
        println!("\n=== {round} launch: press button 1 on each controller, one at a time ===");
        for (index, id) in players.iter().enumerate() {
            let device = config
                .controller(id.as_deref().unwrap_or_default())
                .and_then(|controller| controller.device_id.clone());
            println!("Player {} = {}", index + 1, device.unwrap_or_default());
        }
        let (_, info, log) = bench.run_session(&config, &pong);
        for line in log.lines().filter(|line| line.starts_with("PRESS")) {
            println!("{line}");
        }
        println!("{:?}", info.mapping);
    }
}
