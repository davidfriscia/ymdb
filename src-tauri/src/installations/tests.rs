//! Registry, persistence and migration tests. MAME is never run: executables
//! are plain files and catalogs are imported from XML fixtures.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use tempfile::TempDir;

use super::migration::{MigrationError, load_or_migrate, plan};
use super::registry::{
    ExecutableIdentity, ExecutableStatus, InstallationRegistry, MameInstallation, MigrationState,
    RegistryError,
};
use super::store::{REGISTRY_FILE_NAME, RegistryStore, RegistryStoreError};
use super::{AppLayout, is_installation_id, new_id};
use crate::catalog::import::{ImportProgress, import_from_reader};
use crate::catalog::store::{CATALOG_FILE_NAME, CatalogSource, CatalogStore};
use crate::launcher::store::LAUNCH_FILE_NAME;
use crate::mame::platform;
use crate::mame::version::MameVersion;

const MACHINES: &str = include_str!("../catalog/testdata/listxml-sample.xml");
const LAUNCH_JSON: &str = r#"{
  "schemaVersion": 1,
  "globalOptions": {},
  "profiles": [
    { "id": "default", "name": "Default", "options": {} },
    { "id": "windowed", "name": "Windowed", "options": { "window": true } }
  ],
  "defaultProfileId": "default",
  "machineOverrides": { "pacman": { "speed": 2 } },
  "softwareOverrides": {}
}
"#;

/// An application directory with the files of the single-installation layout.
struct App {
    dir: TempDir,
    layout: AppLayout,
}

impl App {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        let layout = AppLayout::new(&dir.path().join("config ✓"), &dir.path().join("data ✓"));
        fs::create_dir_all(&layout.config_dir).expect("config dir");
        fs::create_dir_all(&layout.data_dir).expect("data dir");
        Self { dir, layout }
    }

    /// A fake executable (never run).
    fn executable(&self, relative: &str) -> PathBuf {
        let path = self.dir.path().join("Émulateurs MAME").join(relative);
        fs::create_dir_all(path.parent().expect("parent")).expect("dir");
        fs::write(&path, format!("binary of {relative}")).expect("write");
        path
    }

    fn legacy_settings(&self, executable: &Path) {
        let json =
            serde_json::json!({ "schemaVersion": 1, "mame": { "executablePath": executable } });
        fs::write(
            self.layout.config_dir.join("settings.json"),
            json.to_string(),
        )
        .expect("write settings");
    }

    fn legacy_launch_config(&self) {
        fs::write(self.layout.config_dir.join(LAUNCH_FILE_NAME), LAUNCH_JSON).expect("write");
    }

    /// Imports the fixture catalog into the legacy location, as if read from `executable`.
    fn legacy_catalog(&self, executable: &Path, release: &str) {
        import_catalog(&self.layout.data_dir, executable, release);
    }

    fn registry_file(&self) -> PathBuf {
        self.layout.config_dir.join(REGISTRY_FILE_NAME)
    }
}

fn import_catalog(dir: &Path, executable: &Path, release: &str) {
    let identity = ExecutableIdentity::read(executable);
    import_from_reader(
        MACHINES.as_bytes(),
        &CatalogStore::in_dir(dir),
        CatalogSource {
            mame_release: release.to_owned(),
            mame_revision: format!("mame{}", release.replace('.', "")),
            executable_path: executable.to_path_buf(),
            executable_size: identity.map(|identity| identity.size),
            executable_modified_at: identity.and_then(|identity| identity.modified_at),
        },
        &AtomicBool::new(false),
        &ImportProgress::default(),
        || Ok(()),
    )
    .expect("import");
}

fn installation(path: &str) -> MameInstallation {
    MameInstallation::new(new_id(), PathBuf::from(path), 1)
}

fn absolute(name: &str) -> String {
    if cfg!(windows) {
        format!("C:\\MAME\\{name}\\mame.exe")
    } else {
        format!("/opt/mame/{name}/mame")
    }
}

fn version(release: &str) -> MameVersion {
    MameVersion {
        release: release.to_owned(),
        revision: format!("mame{}", release.replace('.', "")),
    }
}

// Registry

#[test]
fn ids_are_random_opaque_hex_strings() {
    let first = new_id();
    let second = new_id();
    assert_ne!(first, second);
    assert!(is_installation_id(&first), "{first}");
    for invalid in [
        "",
        "../../etc",
        "0.280",
        &first.to_uppercase(),
        &first[..31],
    ] {
        assert!(!is_installation_id(invalid), "{invalid}");
    }
}

#[test]
fn registers_several_installations_including_the_same_version() {
    let mut registry = InstallationRegistry::default();
    let mut first = installation(&absolute("0.280"));
    first.version = Some(version("0.280"));
    let mut second = installation(&absolute("0.280-copy"));
    second.version = Some(version("0.280"));
    let third = installation(&absolute("0.252"));
    let (first_id, second_id) = (first.id.clone(), second.id.clone());

    registry.add(first).expect("first");
    registry
        .add(second)
        .expect("same version, other executable");
    registry.add(third).expect("third");

    assert_eq!(registry.installations.len(), 3);
    // The first one becomes active; adding others does not change it.
    assert_eq!(
        registry.active_installation_id.as_deref(),
        Some(first_id.as_str())
    );
    assert_eq!(
        registry
            .get(&second_id)
            .map(MameInstallation::label)
            .as_deref(),
        Some("MAME 0.280")
    );
    registry.validate().expect("valid");
}

#[test]
fn rejects_duplicate_executables_by_platform_rules() {
    let app = App::new();
    let mame = app.executable("0.280/mame.exe");
    let mut registry = InstallationRegistry::default();
    let first = MameInstallation::new(new_id(), mame.clone(), 1);
    let first_id = first.id.clone();
    registry.add(first).expect("add");

    assert_eq!(
        registry.add(MameInstallation::new(new_id(), mame.clone(), 2)),
        Err(RegistryError::Duplicate(first_id.clone()))
    );
    // The same file through a relative component.
    let dotted = mame
        .parent()
        .expect("parent")
        .join("..")
        .join("0.280")
        .join("mame.exe");
    assert_eq!(
        registry.add(MameInstallation::new(new_id(), dotted, 3)),
        Err(RegistryError::Duplicate(first_id.clone()))
    );
    // Case differences only name the same file where the file system ignores case.
    let upper = PathBuf::from(mame.to_string_lossy().to_uppercase());
    assert_eq!(
        registry
            .add(MameInstallation::new(new_id(), upper, 4))
            .is_err(),
        cfg!(any(windows, target_os = "macos"))
    );
    assert!(platform::same_executable(&mame, &mame));
}

#[test]
fn selects_renames_and_records_verifications() {
    let mut registry = InstallationRegistry::default();
    let first = installation(&absolute("a"));
    let second = installation(&absolute("b"));
    let (first_id, second_id) = (first.id.clone(), second.id.clone());
    registry.add(first).expect("add");
    registry.add(second).expect("add");

    registry.set_active(&second_id).expect("select");
    assert_eq!(
        registry.active().map(|active| active.id.as_str()),
        Some(second_id.as_str())
    );
    assert_eq!(
        registry.set_active("ffffffffffffffffffffffffffffffff"),
        Err(RegistryError::NotFound(
            "ffffffffffffffffffffffffffffffff".to_owned()
        ))
    );

    registry
        .rename(&first_id, Some("  Arcade build  "))
        .expect("rename");
    assert_eq!(
        registry.get(&first_id).expect("first").label(),
        "Arcade build"
    );
    assert!(matches!(
        registry.rename(&first_id, Some(&"x".repeat(61))),
        Err(RegistryError::Invalid(_))
    ));
    assert!(matches!(
        registry.rename(&first_id, Some("tab\tname")),
        Err(RegistryError::Invalid(_))
    ));
    registry.rename(&first_id, Some("   ")).expect("clear");
    assert_eq!(registry.get(&first_id).expect("first").display_name, None);
    assert_eq!(
        registry.get(&first_id).expect("first").label(),
        "MAME (version unknown)"
    );

    let identity = ExecutableIdentity {
        size: 10,
        modified_at: Some(20),
    };
    registry
        .record_verification(&first_id, version("0.281"), Some(identity), 30)
        .expect("verify");
    let first = registry.get(&first_id).expect("first");
    assert_eq!(first.label(), "MAME 0.281");
    assert_eq!(
        (first.identity, first.verified_at),
        (Some(identity), Some(30))
    );
}

#[test]
fn removing_the_active_installation_selects_the_first_remaining_one() {
    let mut registry = InstallationRegistry::default();
    let ids: Vec<String> = ["a", "b", "c"]
        .iter()
        .map(|name| {
            let installation = installation(&absolute(name));
            let id = installation.id.clone();
            registry.add(installation).expect("add");
            id
        })
        .collect();
    registry.set_active(&ids[1]).expect("select");

    // Removing an inactive installation keeps the selection.
    registry.remove(&ids[2]).expect("remove");
    assert_eq!(
        registry.active_installation_id.as_deref(),
        Some(ids[1].as_str())
    );
    // Removing the active one selects the first remaining one.
    registry.remove(&ids[1]).expect("remove");
    assert_eq!(
        registry.active_installation_id.as_deref(),
        Some(ids[0].as_str())
    );
    registry.remove(&ids[0]).expect("remove");
    assert_eq!(registry.active_installation_id, None);
    assert_eq!(
        registry.remove(&ids[0]),
        Err(RegistryError::NotFound(ids[0].clone()))
    );
}

#[test]
fn executable_status_is_known_without_running_mame() {
    let app = App::new();
    let mame = app.executable("0.280/mame.exe");
    let mut installation = MameInstallation::new(new_id(), mame.clone(), 1);
    assert_eq!(
        installation.executable_status(),
        ExecutableStatus::NotVerified
    );

    installation.identity = ExecutableIdentity::read(&mame);
    assert_eq!(installation.executable_status(), ExecutableStatus::Verified);

    fs::write(&mame, "updated in place, with another size").expect("write");
    assert_eq!(installation.executable_status(), ExecutableStatus::Changed);

    fs::remove_file(&mame).expect("remove");
    assert_eq!(installation.executable_status(), ExecutableStatus::Missing);
}

// Persistence

#[test]
fn saves_and_loads_the_registry() {
    let app = App::new();
    let store = RegistryStore::in_dir(&app.layout.config_dir);
    assert!(store.load().expect("load").is_none());

    let mut registry = InstallationRegistry::default();
    let mut first = MameInstallation::new(new_id(), app.executable("0.280/mame.exe"), 5);
    first.display_name = Some("Émulateur ✓".to_owned());
    first.version = Some(version("0.280"));
    let second = MameInstallation::new(new_id(), app.executable("0.252/mame.exe"), 6);
    let second_id = second.id.clone();
    registry.add(first).expect("add");
    registry.add(second).expect("add");
    registry.set_active(&second_id).expect("select");
    store.save(&registry).expect("save");

    assert_eq!(store.load().expect("load"), Some(registry));
    let json: serde_json::Value =
        serde_json::from_slice(&fs::read(app.registry_file()).expect("read")).expect("json");
    assert_eq!(json["schemaVersion"], 1);
    assert_eq!(json["activeInstallationId"], second_id.as_str());
    assert_eq!(json["installations"][0]["displayName"], "Émulateur ✓");
    assert_eq!(json["installations"][0]["version"]["release"], "0.280");
}

#[test]
fn reports_invalid_registry_files_and_never_saves_them() {
    let app = App::new();
    let store = RegistryStore::in_dir(&app.layout.config_dir);
    let id = new_id();
    let path = absolute("a").replace('\\', "\\\\");
    for content in [
        "{ not json".to_owned(),
        r#"{ "schemaVersion": 2 }"#.to_owned(),
        r#"{ "installations": [{ "id": "../x", "executablePath": "/a", "addedAt": 1 }] }"#
            .to_owned(),
        format!(
            r#"{{ "installations": [{{ "id": "{id}", "executablePath": "{path}", "addedAt": 1 }}, {{ "id": "{}", "executablePath": "{path}", "addedAt": 1 }}] }}"#,
            new_id()
        ),
        format!(r#"{{ "activeInstallationId": "{id}" }}"#),
    ] {
        fs::write(app.registry_file(), &content).expect("write");
        assert!(
            matches!(store.load(), Err(RegistryStoreError::Invalid { .. })),
            "{content}"
        );
    }
    let invalid = InstallationRegistry {
        active_installation_id: Some(id),
        ..InstallationRegistry::default()
    };
    assert!(store.save(&invalid).is_err());
}

// Migration

#[test]
fn nothing_to_migrate_gives_an_empty_registry_without_writing_files() {
    let app = App::new();
    let registry = load_or_migrate(&app.layout).expect("load");
    assert_eq!(registry, InstallationRegistry::default());
    assert!(!app.registry_file().exists());
}

#[test]
fn migrates_the_single_installation_layout_without_reimporting() {
    let app = App::new();
    let mame = app.executable("0.252/mame.exe");
    app.legacy_settings(&mame);
    app.legacy_catalog(&mame, "0.252");
    app.legacy_launch_config();
    let catalog_before = fs::read(app.layout.data_dir.join(CATALOG_FILE_NAME)).expect("catalog");
    let settings_before = fs::read(app.layout.config_dir.join("settings.json")).expect("settings");
    let media = app.layout.config_dir.join("media.json");
    fs::write(&media, r#"{ "schemaVersion": 1, "sources": {} }"#).expect("media");

    let registry = load_or_migrate(&app.layout).expect("migrate");

    assert_eq!(registry.installations.len(), 1);
    let installation = &registry.installations[0];
    assert_eq!(installation.executable_path, mame);
    // The version comes from the catalog: MAME was not run.
    assert_eq!(installation.version, Some(version("0.252")));
    assert_eq!(installation.executable_status(), ExecutableStatus::Verified);
    assert_eq!(
        registry.active_installation_id.as_ref(),
        Some(&installation.id)
    );
    let migration = registry.legacy_migration.as_ref().expect("record");
    assert_eq!(migration.state, MigrationState::Completed);

    // The catalog was moved as is, not re-imported.
    let moved = app
        .layout
        .catalog_dir(&installation.id)
        .join(CATALOG_FILE_NAME);
    assert_eq!(fs::read(&moved).expect("moved"), catalog_before);
    assert!(!app.layout.data_dir.join(CATALOG_FILE_NAME).exists());
    let info = CatalogStore::in_dir(&app.layout.catalog_dir(&installation.id))
        .read_info()
        .expect("read")
        .expect("catalog");
    assert_eq!(info.mame_release, "0.252");
    // The launch configuration was copied; the old files are untouched backups.
    assert_eq!(
        fs::read_to_string(
            app.layout
                .launch_config_dir(&installation.id)
                .join(LAUNCH_FILE_NAME)
        )
        .expect("copied"),
        LAUNCH_JSON
    );
    assert_eq!(
        fs::read_to_string(app.layout.config_dir.join(LAUNCH_FILE_NAME)).expect("old"),
        LAUNCH_JSON
    );
    assert_eq!(
        fs::read(app.layout.config_dir.join("settings.json")).expect("settings"),
        settings_before
    );
    // Media sources stay global.
    assert!(media.is_file());

    // Idempotent: loading again changes nothing.
    let saved = fs::read(app.registry_file()).expect("registry");
    assert_eq!(load_or_migrate(&app.layout).expect("again"), registry);
    assert_eq!(fs::read(app.registry_file()).expect("registry"), saved);
}

#[test]
fn an_interrupted_migration_resumes_with_the_same_installation() {
    let app = App::new();
    let mame = app.executable("0.252/mame.exe");
    app.legacy_settings(&mame);
    app.legacy_catalog(&mame, "0.252");
    app.legacy_launch_config();
    // An import interrupted in the old layout left a partial file.
    fs::write(
        app.layout.data_dir.join("catalog.sqlite.partial"),
        "partial",
    )
    .expect("partial");
    let catalog_before = fs::read(app.layout.data_dir.join(CATALOG_FILE_NAME)).expect("catalog");

    // The plan was saved, then the move failed: something occupies the
    // target catalog path.
    let store = RegistryStore::in_dir(&app.layout.config_dir);
    let planned = plan(&app.layout)
        .expect("plan")
        .expect("something to migrate");
    store.save(&planned).expect("save plan");
    let id = planned.installations[0].id.clone();
    let target = app.layout.catalog_dir(&id).join(CATALOG_FILE_NAME);
    fs::create_dir_all(&target).expect("obstacle");

    let error = load_or_migrate(&app.layout).expect_err("blocked");
    assert!(matches!(error, MigrationError::Verification(_)), "{error}");
    let pending = store.load().expect("load").expect("saved plan");
    assert_eq!(pending.installations[0].id, id);
    assert_eq!(
        pending
            .legacy_migration
            .as_ref()
            .map(|migration| migration.state),
        Some(MigrationState::Pending)
    );
    // Nothing was lost: the old catalog is still in place.
    assert_eq!(
        fs::read(app.layout.data_dir.join(CATALOG_FILE_NAME)).expect("catalog"),
        catalog_before
    );

    // Once the obstacle is gone, the next start resumes with the same id.
    fs::remove_dir(&target).expect("remove obstacle");
    let registry = load_or_migrate(&app.layout).expect("resume");
    assert_eq!(registry.installations[0].id, id);
    assert_eq!(
        registry
            .legacy_migration
            .as_ref()
            .map(|migration| migration.state),
        Some(MigrationState::Completed)
    );
    assert_eq!(fs::read(&target).expect("moved"), catalog_before);
    assert!(!app.layout.data_dir.join("catalog.sqlite.partial").exists());
    assert!(
        app.layout
            .launch_config_dir(&id)
            .join(LAUNCH_FILE_NAME)
            .is_file()
    );
}

#[test]
fn a_crash_after_the_move_completes_on_the_next_start() {
    let app = App::new();
    let mame = app.executable("0.252/mame.exe");
    app.legacy_settings(&mame);
    app.legacy_catalog(&mame, "0.252");
    let completed = load_or_migrate(&app.layout).expect("migrate");
    // Simulate a crash before the completion was saved.
    let mut pending = completed.clone();
    if let Some(migration) = pending.legacy_migration.as_mut() {
        migration.state = MigrationState::Pending;
        migration.completed_at = None;
    }
    RegistryStore::in_dir(&app.layout.config_dir)
        .save(&pending)
        .expect("save");

    let resumed = load_or_migrate(&app.layout).expect("resume");
    assert_eq!(resumed.installations, completed.installations);
    assert_eq!(
        resumed
            .legacy_migration
            .as_ref()
            .map(|migration| migration.state),
        Some(MigrationState::Completed)
    );
}

#[test]
fn a_catalog_of_another_executable_gets_its_own_installation() {
    let app = App::new();
    let configured = app.executable("0.252/mame.exe");
    let other = app.executable("0.237/mame.exe");
    app.legacy_settings(&configured);
    app.legacy_catalog(&other, "0.237");
    app.legacy_launch_config();

    let registry = load_or_migrate(&app.layout).expect("migrate");

    assert_eq!(registry.installations.len(), 2);
    let (first, second) = (&registry.installations[0], &registry.installations[1]);
    // The configured executable stays active; its version is unknown until verified.
    assert_eq!(first.executable_path, configured);
    assert_eq!(first.version, None);
    assert_eq!(registry.active_installation_id.as_ref(), Some(&first.id));
    // The catalog is never associated with a build it does not come from.
    assert_eq!(second.executable_path, other);
    assert_eq!(second.version, Some(version("0.237")));
    assert!(
        !app.layout
            .catalog_dir(&first.id)
            .join(CATALOG_FILE_NAME)
            .exists()
    );
    assert!(
        app.layout
            .catalog_dir(&second.id)
            .join(CATALOG_FILE_NAME)
            .is_file()
    );
    // The launch options were used with the configured executable.
    assert!(
        app.layout
            .launch_config_dir(&first.id)
            .join(LAUNCH_FILE_NAME)
            .is_file()
    );
    assert!(!app.layout.launch_config_dir(&second.id).exists());
}

#[test]
fn migrates_a_configuration_without_catalog() {
    let app = App::new();
    let mame = app.executable("0.280/mame.exe");
    app.legacy_settings(&mame);

    let registry = load_or_migrate(&app.layout).expect("migrate");
    assert_eq!(registry.installations.len(), 1);
    assert_eq!(registry.installations[0].version, None);
    assert_eq!(
        registry.installations[0].executable_status(),
        ExecutableStatus::NotVerified
    );
    assert!(
        registry
            .legacy_migration
            .as_ref()
            .is_some_and(|migration| migration.catalog.is_none())
    );
}

#[test]
fn an_invalid_legacy_settings_file_stops_the_migration_without_changes() {
    let app = App::new();
    let mame = app.executable("0.252/mame.exe");
    app.legacy_catalog(&mame, "0.252");
    fs::write(app.layout.config_dir.join("settings.json"), "{ broken").expect("write");

    let error = load_or_migrate(&app.layout).expect_err("invalid settings");
    assert!(
        matches!(error, MigrationError::LegacySettings(_)),
        "{error}"
    );
    assert!(!app.registry_file().exists());
    assert!(app.layout.data_dir.join(CATALOG_FILE_NAME).is_file());
}
