//! Installation state tests without Tauri: independent catalogs, imports,
//! searches, launches and removal across installations. MAME is never run:
//! executables are plain files and catalogs come from XML fixtures.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use tempfile::TempDir;

use super::*;
use crate::catalog::detail::machine::machine_detail;
use crate::catalog::import::{ImportError, ImportProgress, import_from_reader};
use crate::catalog::search::machines::search_machines;
use crate::catalog::search::request::MachineSearchRequest;
use crate::catalog::store::{CatalogInfo, CatalogSource};
use crate::commands::catalog::{CatalogState, ImportStatus};
use crate::commands::launcher::{LaunchSettings, prepare_resolved};
use crate::commands::search::search;
use crate::controllers::ControllerPaths;
use crate::controllers::model::{Assignment, AssignmentLevel, AssignmentTarget, ControllersConfig};
use crate::launcher::LaunchRequest;
use crate::launcher::config::{LaunchConfig, Profile};
use crate::launcher::options::{OptionSet, OptionValue};
use crate::mame::validation::ValidationStatus;
use crate::media::config::{MediaConfig, MediaConfigStore};
use crate::media::resolve::{MediaKey, resolve as resolve_media};

const ARCADE: &str = include_str!("../../catalog/testdata/listxml-sample.xml");
const SEARCH: &str = include_str!("../../catalog/testdata/search-machines.xml");
const LAUNCHER: &str = include_str!("../../catalog/testdata/launcher-machines.xml");

struct App {
    dir: TempDir,
    state: InstallationsState,
    catalog: CatalogState,
}

impl App {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        let layout = AppLayout::new(&dir.path().join("config"), &dir.path().join("data ✓"));
        Self {
            state: InstallationsState::new(Some(layout)),
            catalog: CatalogState::default(),
            dir,
        }
    }

    fn layout(&self) -> &AppLayout {
        self.state.layout().expect("layout")
    }

    /// Registers a fake executable as if `-version` had reported `release`.
    fn add(&self, folder: &str, release: &str) -> String {
        let path = self.dir.path().join("MAME ✓").join(folder).join("mame.exe");
        fs::create_dir_all(path.parent().expect("parent")).expect("dir");
        fs::write(&path, format!("binary {folder}")).expect("write");
        self.state
            .register(&ValidationReport {
                executable_path: path,
                status: ValidationStatus::Valid,
                version: Some(MameVersion {
                    release: release.to_owned(),
                    revision: format!("mame{}", release.replace('.', "")),
                }),
                output: None,
                checked_at: 1,
            })
            .expect("register")
    }

    /// Imports `xml` into an installation's catalog through the import job.
    fn import(&self, id: &str, xml: &'static str) -> ImportStatus {
        let resolved = self.state.resolve(id).expect("resolve");
        let source = source(&resolved.installation.executable_path);
        self.catalog
            .start_with(
                ImportKind::Machines,
                resolved,
                &self.state,
                move |store, cancel, progress| import_xml(store, xml, source, cancel, progress),
            )
            .expect("start");
        self.wait_for_import()
    }

    fn wait_for_import(&self) -> ImportStatus {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let status = self.catalog.import_status();
            if !matches!(status, ImportStatus::Running { .. }) || Instant::now() > deadline {
                return status;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn machines(&self, id: &str) -> (u64, Vec<String>) {
        let store = self.state.resolve(id).expect("resolve").catalog;
        let response = search(&store, |connection| {
            search_machines(
                connection,
                &MachineSearchRequest {
                    page_size: Some(100),
                    ..MachineSearchRequest::default()
                },
                None,
            )
        })
        .expect("search");
        (
            response.total,
            response.items.into_iter().map(|item| item.name).collect(),
        )
    }
}

use crate::commands::catalog::ImportKind;

fn source(executable: &Path) -> CatalogSource {
    let identity = ExecutableIdentity::read(executable).expect("identity");
    CatalogSource {
        mame_release: "0.280".to_owned(),
        mame_revision: "mame0280".to_owned(),
        executable_path: executable.to_path_buf(),
        executable_size: Some(identity.size),
        executable_modified_at: identity.modified_at,
    }
}

fn import_xml(
    store: &CatalogStore,
    xml: &str,
    source: CatalogSource,
    cancel: &AtomicBool,
    progress: &ImportProgress,
) -> Result<CatalogInfo, ImportError> {
    import_from_reader(xml.as_bytes(), store, source, cancel, progress, || Ok(()))
}

fn machine_request(installation_id: &str, machine: &str) -> LaunchRequest {
    LaunchRequest {
        installation_id: installation_id.to_owned(),
        machine: machine.to_owned(),
        software: None,
        profile_id: None,
        controls: None,
    }
}

#[test]
fn every_installation_has_its_own_catalog() {
    let app = App::new();
    let old = app.add("0.279", "0.279");
    let new = app.add("0.280", "0.280");
    assert!(
        matches!(app.import(&old, ARCADE), ImportStatus::Completed { ref installation_id, .. } if *installation_id == old)
    );
    assert!(
        matches!(app.import(&new, SEARCH), ImportStatus::Completed { ref installation_id, .. } if *installation_id == new)
    );

    let (old_total, old_names) = app.machines(&old);
    let (new_total, new_names) = app.machines(&new);
    assert_eq!(old_total, 8);
    assert_eq!(new_total, 13);
    assert!(old_names.contains(&"kof98".to_owned()));
    assert!(!new_names.contains(&"kof98".to_owned()));
    assert!(new_names.contains(&"sfiii".to_owned()));
    // The files are separate.
    let old_file = app.layout().catalog_dir(&old).join(CATALOG_FILE_NAME);
    let new_file = app.layout().catalog_dir(&new).join(CATALOG_FILE_NAME);
    assert!(old_file.is_file() && new_file.is_file());
    assert_ne!(old_file, new_file);

    // Re-importing one installation never replaces the other's catalog.
    app.import(&new, LAUNCHER);
    assert_eq!(app.machines(&old).0, 8);
    assert_eq!(app.machines(&new).0, 8);
    assert!(app.machines(&new).1.contains(&"apple2e".to_owned()));
    let view = app.state.view().expect("view");
    assert!(
        view.installations
            .iter()
            .all(|installation| installation.catalog_imported)
    );
}

#[test]
fn queries_name_their_installation_whatever_the_active_one_becomes() {
    let app = App::new();
    let old = app.add("0.279", "0.279");
    let new = app.add("0.280", "0.280");
    app.import(&old, ARCADE);
    app.import(&new, SEARCH);
    assert_eq!(
        app.state
            .view()
            .expect("view")
            .active_installation_id
            .as_deref(),
        Some(old.as_str())
    );

    // A request resolved before the switch keeps reading its own catalog.
    let resolved = app.state.resolve(&old).expect("resolve");
    app.state.set_active(&new).expect("switch");
    let detail = resolved
        .catalog
        .with_reader(|connection| Ok(machine_detail(connection, "kof98")))
        .expect("read")
        .expect("catalog")
        .expect("query");
    assert!(detail.is_some());
    // The same name in the other catalog does not exist.
    let other = app
        .state
        .resolve(&new)
        .expect("resolve")
        .catalog
        .with_reader(|connection| Ok(machine_detail(connection, "kof98")))
        .expect("read")
        .expect("catalog")
        .expect("query");
    assert!(other.is_none());
    // The selection persists in the registry file.
    let reloaded = InstallationsState::new(Some(app.layout().clone()));
    assert_eq!(
        reloaded
            .view()
            .expect("view")
            .active_installation_id
            .as_deref(),
        Some(new.as_str())
    );
}

#[test]
fn only_the_requested_and_active_catalogs_stay_open() {
    let app = App::new();
    let ids: Vec<String> = ["a", "b", "c"]
        .iter()
        .map(|folder| app.add(folder, "0.280"))
        .collect();
    for id in &ids {
        app.import(id, ARCADE);
    }
    for id in &ids {
        app.machines(id);
    }
    // `ids[0]` is active and `ids[2]` was the last one requested.
    assert_eq!(app.state.open_readers(), 2);
    app.state.set_active(&ids[1]).expect("switch");
    assert_eq!(app.state.open_readers(), 0);
    app.machines(&ids[1]);
    assert_eq!(app.state.open_readers(), 1);
}

#[test]
fn an_import_stays_with_its_installation_while_the_active_one_changes() {
    let app = App::new();
    let first = app.add("0.279", "0.279");
    let second = app.add("0.280", "0.280");
    let (release, released) = mpsc::channel::<()>();
    let resolved = app.state.resolve(&first).expect("resolve");
    let source = source(&resolved.installation.executable_path);
    app.catalog
        .start_with(
            ImportKind::Machines,
            resolved,
            &app.state,
            move |store, cancel, progress| {
                released.recv().expect("release");
                import_xml(store, ARCADE, source, cancel, progress)
            },
        )
        .expect("start");

    // While it runs: switch the active installation, try another import and removals.
    app.state.set_active(&second).expect("switch");
    let other = app.state.resolve(&second).expect("resolve");
    assert_eq!(
        app.catalog
            .start_with(ImportKind::Machines, other, &app.state, |_, _, _| Err(
                ImportError::Cancelled
            ))
            .err(),
        Some(CommandError::ImportInProgress)
    );
    assert!(matches!(
        app.catalog.while_not_importing(&first, || Ok(())),
        Err(CommandError::RemovalNotAllowed { .. })
    ));
    assert!(
        matches!(app.catalog.import_status(), ImportStatus::Running { ref installation_id, .. } if *installation_id == first)
    );

    release.send(()).expect("release");
    assert!(
        matches!(app.wait_for_import(), ImportStatus::Completed { ref installation_id, .. } if *installation_id == first)
    );
    assert_eq!(app.machines(&first).0, 8);
    assert_eq!(
        app.state
            .resolve(&second)
            .expect("resolve")
            .catalog
            .read_info()
            .expect("read"),
        None
    );
}

#[test]
fn a_cancelled_import_keeps_the_installation_catalog() {
    let app = App::new();
    let id = app.add("0.280", "0.280");
    app.import(&id, ARCADE);
    let (release, released) = mpsc::channel::<()>();
    let resolved = app.state.resolve(&id).expect("resolve");
    let source = source(&resolved.installation.executable_path);
    app.catalog
        .start_with(
            ImportKind::Machines,
            resolved,
            &app.state,
            move |store, cancel, progress| {
                released.recv().expect("release");
                import_xml(store, SEARCH, source, cancel, progress)
            },
        )
        .expect("start");
    app.catalog.cancel();
    release.send(()).expect("release");

    assert!(
        matches!(app.wait_for_import(), ImportStatus::Cancelled { ref installation_id, .. } if *installation_id == id)
    );
    assert_eq!(app.machines(&id).0, 8);
}

#[test]
fn an_import_for_a_removed_installation_does_not_start() {
    let app = App::new();
    let id = app.add("0.280", "0.280");
    let resolved = app.state.resolve(&id).expect("resolve");
    app.state.remove(&resolved, true, true).expect("remove");
    assert_eq!(
        app.catalog
            .start_with(ImportKind::Machines, resolved, &app.state, |_, _, _| Err(
                ImportError::Cancelled
            ))
            .err(),
        Some(CommandError::InstallationNotFound {
            installation_id: id.clone()
        })
    );
    assert!(matches!(
        app.state.resolve(&id),
        Err(CommandError::InstallationNotFound { .. })
    ));
}

#[test]
fn launches_use_the_requested_installation_and_its_own_options() {
    let app = App::new();
    let old = app.add("0.279", "0.279");
    let new = app.add("0.280", "0.280");
    app.import(&old, LAUNCHER);
    app.import(&new, LAUNCHER);
    let old_resolved = app.state.resolve(&old).expect("resolve");
    let new_resolved = app.state.resolve(&new).expect("resolve");

    // Different global options and a profile that only exists for `old`.
    let mut old_config = LaunchConfig::default();
    old_config
        .global_options
        .insert("window".to_owned(), OptionValue::Boolean(true));
    old_config.profiles.push(Profile {
        id: "fast".to_owned(),
        name: "Fast".to_owned(),
        options: OptionSet::from([("speed".to_owned(), OptionValue::Number(2.0))]),
    });
    old_config.machine_overrides.insert(
        "apple2e".to_owned(),
        OptionSet::from([("volume".to_owned(), OptionValue::Number(-6.0))]),
    );
    old_resolved
        .launch_config
        .save(old_config.clone())
        .expect("save");
    let new_config = new_resolved.launch_config.load().expect("load");
    assert_eq!(new_config, LaunchConfig::default());

    app.state.set_active(&new).expect("switch");
    let old_plan = prepare_resolved(
        &old_resolved,
        &LaunchSettings::with_config(old_config.clone()),
        &LaunchRequest {
            profile_id: Some("fast".to_owned()),
            ..machine_request(&old, "apple2e")
        },
    )
    .expect("prepare")
    .plan;
    assert_eq!(old_plan.installation_id, old);
    assert_eq!(old_plan.profile_id, "fast");
    assert_eq!(
        old_plan.executable,
        old_resolved.installation.executable_path
    );
    assert_eq!(
        old_plan.command_line(),
        ["apple2e", "-window", "-speed", "2", "-volume", "-6"]
    );

    let new_plan = prepare_resolved(
        &new_resolved,
        &LaunchSettings::with_config(new_config.clone()),
        &machine_request(&new, "apple2e"),
    )
    .expect("prepare")
    .plan;
    assert_eq!(new_plan.installation_id, new);
    assert_eq!(new_plan.profile_id, "default");
    assert_eq!(
        new_plan.executable,
        new_resolved.installation.executable_path
    );
    assert_eq!(new_plan.command_line(), ["apple2e"]);
    assert!(matches!(
        prepare_resolved(
            &new_resolved,
            &LaunchSettings::with_config(new_config.clone()),
            &LaunchRequest {
                profile_id: Some("fast".to_owned()),
                ..machine_request(&new, "apple2e")
            }
        ),
        Err(CommandError::InvalidRequest { .. })
    ));
}

#[test]
fn a_controller_profile_has_one_configuration_directory_per_installation() {
    let app = App::new();
    let old = app.add("0.279", "0.279");
    let new = app.add("0.280", "0.280");
    app.import(&old, LAUNCHER);
    app.import(&new, LAUNCHER);
    let config_dir = app.layout().config_dir.clone();
    let mut controllers = ControllersConfig::new();
    let profile = controllers
        .create_profile("Fighting", false, |_| false)
        .expect("profile");
    controllers
        .set_assignment(
            &AssignmentTarget {
                level: AssignmentLevel::Default,
                key: None,
            },
            Assignment {
                profile_id: Some(profile.clone()),
                players: None,
            },
        )
        .expect("assign");
    let settings = LaunchSettings {
        controllers,
        controller_paths: ControllerPaths::new(&config_dir),
        ..LaunchSettings::with_config(LaunchConfig::default())
    };
    let directory = |id: &str| {
        let resolved = app.state.resolve(id).expect("resolve");
        prepare_resolved(&resolved, &settings, &machine_request(id, "apple2e"))
            .expect("prepare")
            .plan
            .controls
            .cfg_directory
            .expect("managed")
    };
    let old_dir = directory(&old);
    let new_dir = directory(&new);
    assert_ne!(old_dir, new_dir);
    assert_eq!(
        old_dir,
        app.layout()
            .launch_config_dir(&old)
            .join("controller-profiles")
            .join(&profile)
            .join("cfg")
    );
    // Preparing creates nothing.
    assert!(!old_dir.exists() && !new_dir.exists());
}

#[test]
fn a_launch_refuses_a_catalog_of_another_executable() {
    let app = App::new();
    let first = app.add("0.279", "0.279");
    let second = app.add("0.280", "0.280");
    // A catalog imported from the first executable, placed in the second one's directory.
    let first_resolved = app.state.resolve(&first).expect("resolve");
    let second_resolved = app.state.resolve(&second).expect("resolve");
    import_xml(
        &second_resolved.catalog,
        LAUNCHER,
        source(&first_resolved.installation.executable_path),
        &AtomicBool::new(false),
        &ImportProgress::default(),
    )
    .expect("import");
    assert_eq!(
        prepare_resolved(
            &second_resolved,
            &LaunchSettings::with_config(LaunchConfig::default()),
            &machine_request(&second, "apple2e")
        )
        .err(),
        Some(CommandError::CatalogMismatch {
            freshness: CatalogFreshness::DifferentExecutable
        })
    );
    assert_eq!(
        prepare_resolved(
            &first_resolved,
            &LaunchSettings::with_config(LaunchConfig::default()),
            &machine_request(&first, "apple2e")
        )
        .err(),
        Some(CommandError::CatalogNotImported)
    );
}

use crate::catalog::freshness::CatalogFreshness;

#[test]
fn removal_deletes_only_what_was_chosen() {
    let app = App::new();
    let first = app.add("0.279", "0.279");
    let second = app.add("0.280", "0.280");
    let third = app.add("0.281", "0.281");
    for id in [&first, &second, &third] {
        app.import(id, ARCADE);
        app.state
            .resolve(id)
            .expect("resolve")
            .launch_config
            .save(LaunchConfig::default())
            .expect("save");
    }
    let executable = app
        .state
        .resolve(&first)
        .expect("resolve")
        .installation
        .executable_path;
    app.machines(&first);

    // Keep everything: only the registration goes away.
    let resolved = app.state.resolve(&first).expect("resolve");
    let result = app.state.remove(&resolved, false, false).expect("remove");
    assert_eq!(
        result.active_installation_id.as_deref(),
        Some(second.as_str())
    );
    assert_eq!(
        result.kept_paths,
        [
            app.layout().catalog_dir(&first),
            app.layout().launch_config_dir(&first)
        ]
    );
    assert!(executable.is_file(), "the MAME executable is never deleted");

    // Delete the catalog only.
    let resolved = app.state.resolve(&second).expect("resolve");
    let result = app.state.remove(&resolved, true, false).expect("remove");
    assert_eq!(
        result.active_installation_id.as_deref(),
        Some(third.as_str())
    );
    assert!(!app.layout().catalog_dir(&second).exists());
    assert_eq!(result.kept_paths, [app.layout().launch_config_dir(&second)]);

    // Delete everything.
    let resolved = app.state.resolve(&third).expect("resolve");
    let result = app.state.remove(&resolved, true, true).expect("remove");
    assert_eq!(result.active_installation_id, None);
    assert!(result.kept_paths.is_empty());
    assert!(!app.layout().catalog_dir(&third).exists());
    assert!(!app.layout().launch_config_dir(&third).exists());
    assert!(app.state.view().expect("view").installations.is_empty());
}

#[test]
fn duplicate_registrations_are_refused() {
    let app = App::new();
    let id = app.add("0.280", "0.280");
    let path = app
        .state
        .resolve(&id)
        .expect("resolve")
        .installation
        .executable_path;
    let report = ValidationReport {
        executable_path: path,
        status: ValidationStatus::Valid,
        version: None,
        output: None,
        checked_at: 2,
    };
    assert_eq!(
        app.state.register(&report),
        Err(CommandError::DuplicateInstallation {
            existing_installation_id: id
        })
    );
    assert_eq!(app.state.view().expect("view").installations.len(), 1);
}

#[test]
fn media_sources_are_shared_by_all_installations() {
    let app = App::new();
    let first = app.add("0.279", "0.279");
    let second = app.add("0.280", "0.280");
    let snaps = app.dir.path().join("snap");
    fs::create_dir_all(&snaps).expect("dir");
    fs::write(snaps.join("pacman.png"), b"\x89PNG").expect("write");
    let store = MediaConfigStore::in_dir(&app.layout().config_dir);
    let mut config = MediaConfig::new();
    config
        .sources
        .insert("snap".to_owned(), vec![snaps.clone()]);
    store.save(config).expect("save");

    // One file in the configuration directory, not per installation.
    for id in [&first, &second] {
        app.state.set_active(id).expect("switch");
        let loaded = store.load().expect("load");
        let resolved = resolve_media(loaded.sources("snap"), MediaKey::Machine("pacman"));
        assert_eq!(
            resolved.map(|media| media.path.ends_with("snap/pacman.png")),
            Some(true)
        );
        assert!(
            !app.layout()
                .launch_config_dir(id)
                .join("media.json")
                .exists()
        );
    }
}

#[test]
fn the_state_migrates_and_retries_after_a_failure() {
    let dir = tempfile::tempdir().expect("temp dir");
    let layout = AppLayout::new(&dir.path().join("config"), &dir.path().join("data"));
    fs::create_dir_all(&layout.config_dir).expect("dir");
    let settings = layout.config_dir.join("settings.json");
    fs::write(&settings, "{ broken").expect("write");

    let state = InstallationsState::new(Some(layout.clone()));
    assert!(matches!(
        state.view(),
        Err(CommandError::MigrationFailed { .. })
    ));
    assert!(matches!(
        state.resolve("0123456789abcdef0123456789abcdef"),
        Err(CommandError::MigrationFailed { .. })
    ));

    // Once the file is fixed, the next access migrates.
    let mame: PathBuf = dir.path().join("mame.exe");
    fs::write(&mame, "binary").expect("write");
    fs::write(
        &settings,
        serde_json::json!({ "mame": { "executablePath": mame } }).to_string(),
    )
    .expect("write");
    let view = state.view().expect("view");
    assert_eq!(view.installations.len(), 1);
    assert!(view.installations[0].active);
    assert!(!view.installations[0].catalog_imported);
}
