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
    assert_eq!(
        probe(&log),
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
