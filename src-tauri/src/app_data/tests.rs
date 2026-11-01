//! Export and import tests. Every test works in temporary directories.

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use rusqlite::Connection;
use tempfile::TempDir;
use zip::write::{SimpleFileOptions, ZipWriter};

use super::archive::{self, Limits};
use super::export::{self, SnapshotSources};
use super::manifest::{ARCHIVE_ROOT, MANIFEST_NAME, Manifest};
use super::restore::{self, RestoreOutcome};
use super::scope::Category;
use super::*;
use crate::catalog::store::CatalogStore;
use crate::controllers::model::ControllersConfig;
use crate::controllers::store::ControllersStore;
use crate::custom_themes::{CustomThemes, CustomThemesStore, NewTheme};
use crate::lists::model::{ListEntry, Lists};
use crate::lists::store::ListsStore;
use crate::media::config::{MediaConfig, MediaConfigStore};
use crate::saved_filters::{SavedFilters, SavedFiltersStore};
use crate::support_files::repository::SupportRepository;

const VERSION: &str = "0.1.0";
const ID_A: &str = "0123456789abcdef0123456789abcdef";
const ID_B: &str = "fedcba9876543210fedcba9876543210";

struct Fixture {
    _temp: TempDir,
    dirs: AppDirs,
    /// Outside YMDB's roots: archives and "external" files.
    outside: PathBuf,
}

fn fixture() -> Fixture {
    let temp = tempfile::tempdir().expect("temp dir");
    let dirs = AppDirs::new(&temp.path().join("config"), &temp.path().join("data"));
    let outside = temp.path().join("outside");
    fs::create_dir_all(&outside).expect("outside dir");
    Fixture {
        _temp: temp,
        dirs,
        outside,
    }
}

fn write(path: &Path, content: &[u8]) {
    fs::create_dir_all(path.parent().expect("parent")).expect("create dir");
    fs::write(path, content).expect("write");
}

/// A database that the catalog store accepts (current schema version).
fn fake_catalog(dirs: &AppDirs, id: &str, rows: i64) {
    let dir = dirs.data.join("installations").join(id);
    fs::create_dir_all(&dir).expect("catalog dir");
    let _ = fs::remove_file(dir.join("catalog.sqlite"));
    let connection = Connection::open(dir.join("catalog.sqlite")).expect("open");
    connection
        .execute_batch("CREATE TABLE t (x INTEGER)")
        .expect("schema");
    for row in 0..rows {
        connection
            .execute("INSERT INTO t VALUES (?1)", [row])
            .expect("insert");
    }
    connection
        .pragma_update(None, "user_version", crate::catalog::schema::SCHEMA_VERSION)
        .expect("version");
}

fn catalog_rows(path: &Path) -> i64 {
    Connection::open(path)
        .expect("open")
        .query_row("SELECT count(*) FROM t", [], |row| row.get(0))
        .expect("count")
}

/// A complete set of application data, with external references.
fn populate(fixture: &Fixture) {
    let dirs = &fixture.dirs;
    let mut lists = Lists::new();
    lists
        .set_membership(
            "favorites",
            &ListEntry::Machine {
                name: "pacman".to_owned(),
            },
            true,
        )
        .expect("favorite");
    // A custom list, with a machine and its dates.
    lists
        .create_user_list("mazes-id", "Arcade / Mazes", 5)
        .expect("custom list");
    lists
        .set_user_memberships(
            &ListEntry::Machine {
                name: "pacman".to_owned(),
            },
            &[("mazes-id".to_owned(), true)],
            6,
        )
        .expect("membership");
    ListsStore::in_dir(&dirs.config)
        .save(&lists)
        .expect("lists");
    let mut filters = SavedFilters::default();
    filters
        .create("Shooters", 1, serde_json::json!({ "text": "shoot" }), 1)
        .expect("filter");
    SavedFiltersStore::in_dir(&dirs.config)
        .save(&filters)
        .expect("filters");
    let mut themes = CustomThemes::default();
    themes
        .create(
            NewTheme {
                name: "Midnight".to_owned(),
                source_theme_id: Some("neon80s".to_owned()),
                tokens_version: 1,
                tokens: serde_json::json!({ "colors": { "accent": "#ff27d8" } }),
            },
            2,
        )
        .expect("theme");
    CustomThemesStore::in_dir(&dirs.config)
        .save(&themes)
        .expect("themes");
    let mut controllers = ControllersConfig::new();
    let profile = controllers
        .create_profile("Pad 1", true, |_| false)
        .expect("profile");
    ControllersStore::in_dir(&dirs.config)
        .save(&controllers)
        .expect("controllers");
    write(
        &dirs
            .config
            .join(format!("controller-profiles/{profile}/base.cfg")),
        b"<mameconfig version=\"10\"><system name=\"default\" /></mameconfig>",
    );
    write(
        &dirs.config.join(format!(
            "installations/{ID_A}/controller-profiles/{profile}/cfg/sf2.cfg"
        )),
        b"<mameconfig version=\"10\"><system name=\"sf2\" /></mameconfig>",
    );
    let media_dir = fixture.outside.join("snaps");
    fs::create_dir_all(&media_dir).expect("media dir");
    let mut media = MediaConfig::new();
    media.sources.insert("snap".to_owned(), vec![media_dir]);
    MediaConfigStore::in_dir(&dirs.config)
        .save(media)
        .expect("media");
    fake_catalog(dirs, ID_A, 3);
    fake_catalog(dirs, ID_B, 5);
    SupportRepository::in_dir(&dirs.data)
        .view()
        .expect("repository");
    // Outside the backup scope.
    write(&dirs.config.join("window-state.json"), b"{}");
    write(&dirs.data.join("launch-logs/1.log"), b"log");
}

fn snapshot_and_export(fixture: &Fixture, name: &str) -> (PathBuf, Manifest) {
    let progress = Progress::new(Phase::Snapshot);
    let work = export::work_dir(&fixture.dirs).expect("work dir");
    let data = fixture.dirs.data.clone();
    let catalog = move |id: &str| CatalogStore::in_dir(&data.join("installations").join(id));
    let preferences = serde_json::json!({ "ymdb.preferences.v2": "{\"theme\":\"dark\"}" });
    let files = {
        let _quiet = gate::exclusive();
        export::snapshot(
            &SnapshotSources {
                dirs: &fixture.dirs,
                catalog: &catalog,
                interface_preferences: Some(&preferences),
            },
            &work,
            &progress,
        )
        .expect("snapshot")
    };
    let destination = fixture.outside.join(name);
    export::finish(&files, &destination, VERSION, &progress).expect("export");
    fs::remove_dir_all(&work).expect("remove work");
    let manifest = archive::open(&destination, &Limits::default())
        .expect("open")
        .manifest;
    (destination, manifest)
}

fn tree(root: &Path) -> Vec<(String, Vec<u8>)> {
    let mut files = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else {
                let relative = path
                    .strip_prefix(root)
                    .expect("relative")
                    .to_string_lossy()
                    .replace('\\', "/");
                files.push((relative, fs::read(&path).expect("read")));
            }
        }
    }
    files.sort();
    files
}

fn stage(fixture: &Fixture, archive: &Path) -> Result<restore::RestorePreview, BackupError> {
    restore::stage(
        archive,
        &fixture.dirs,
        VERSION,
        &Progress::new(Phase::Checking),
    )
}

#[test]
fn exports_an_empty_installation() {
    let fixture = fixture();

    let (archive, manifest) = snapshot_and_export(&fixture, "empty.ymdb-backup.zip");

    assert_eq!(
        manifest
            .items
            .iter()
            .map(|item| item.path.as_str())
            .collect::<Vec<_>>(),
        ["interface-preferences.json"]
    );
    assert!(!export::partial_path(&archive).exists());
    assert!(
        !fixture
            .dirs
            .data
            .join(scope::EXPORT_WORK_DIR)
            .read_dir()
            .is_ok_and(|mut entries| entries.next().is_some())
    );
}

#[test]
fn exports_every_kind_of_data_with_checksums() {
    let fixture = fixture();
    populate(&fixture);

    let (_, manifest) = snapshot_and_export(&fixture, "full.ymdb-backup.zip");

    let categories: Vec<Category> = manifest.items.iter().map(|item| item.category).collect();
    for category in [
        Category::Lists,
        Category::SavedFilters,
        Category::CustomThemes,
        Category::Controllers,
        Category::ControllerBaseFiles,
        Category::ControllerProfileConfigurations,
        Category::MediaSources,
        Category::Catalogs,
        Category::SupportFiles,
        Category::InterfacePreferences,
    ] {
        assert!(categories.contains(&category), "{category:?}");
    }
    assert_eq!(
        categories
            .iter()
            .filter(|category| **category == Category::Catalogs)
            .count(),
        2
    );
    assert!(manifest.items.iter().all(|item| item.sha256.len() == 64));
    for excluded in ["window-state.json", "launch-logs", "classifications.sqlite"] {
        assert!(
            !manifest
                .items
                .iter()
                .any(|item| item.path.contains(excluded)),
            "{excluded}"
        );
    }
    assert_eq!(manifest.platform, manifest::Platform::current());
}

#[test]
fn round_trip_restores_the_same_data() {
    let fixture = fixture();
    populate(&fixture);
    let config_before = tree(&fixture.dirs.config);
    let support_before = SupportRepository::in_dir(&fixture.dirs.data)
        .view()
        .expect("view");
    let (archive, _) = snapshot_and_export(&fixture, "trip.ymdb-backup.zip");

    // Change everything after the export.
    ListsStore::in_dir(&fixture.dirs.config)
        .save(&Lists::new())
        .expect("clear lists");
    fs::remove_dir_all(fixture.dirs.data.join("installations").join(ID_B)).expect("remove catalog");
    fake_catalog(&fixture.dirs, ID_A, 99);
    write(
        &fixture
            .dirs
            .config
            .join("installations/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/launch.json"),
        b"{}",
    );

    let preview = stage(&fixture, &archive).expect("stage");
    assert_eq!(preview.app_version, VERSION);
    assert!(preview.same_platform);
    assert!(
        preview
            .categories
            .iter()
            .any(|summary| summary.category == Category::Catalogs && summary.files == 2)
    );
    assert!(
        preview
            .replaced
            .iter()
            .any(|summary| summary.category == Category::Lists)
    );
    assert!(
        preview
            .external_references
            .iter()
            .any(|reference| reference.exists)
    );
    restore::schedule(&fixture.dirs).expect("schedule");

    let outcome = restore::recover(&fixture.dirs).expect("outcome");

    let RestoreOutcome::Restored {
        interface_preferences,
        ..
    } = outcome
    else {
        panic!("{outcome:?}");
    };
    assert_eq!(
        interface_preferences,
        Some(serde_json::json!({ "ymdb.preferences.v2": "{\"theme\":\"dark\"}" }))
    );
    assert_eq!(tree(&fixture.dirs.config), config_before);
    let catalogs = fixture.dirs.data.join("installations");
    assert_eq!(catalog_rows(&catalogs.join(ID_A).join("catalog.sqlite")), 3);
    assert_eq!(catalog_rows(&catalogs.join(ID_B).join("catalog.sqlite")), 5);
    assert_eq!(
        SupportRepository::in_dir(&fixture.dirs.data)
            .view()
            .expect("view"),
        support_before
    );
    // The restored data is usable by the stores.
    let lists = ListsStore::in_dir(&fixture.dirs.config)
        .load()
        .expect("lists");
    assert_eq!(lists.list("favorites").expect("favorites").entries.len(), 1);
    // Custom lists come back with their id, name, dates and entries.
    let mazes = lists.list("mazes-id").expect("custom list");
    assert_eq!(mazes.display_name(), "Arcade / Mazes");
    assert_eq!((mazes.created_at, mazes.updated_at), (Some(5), Some(6)));
    assert_eq!(mazes.machine_names().collect::<Vec<_>>(), ["pacman"]);
    // Custom themes come back with their id, name, source and tokens.
    let themes = CustomThemesStore::in_dir(&fixture.dirs.config)
        .load()
        .expect("themes");
    let midnight = &themes.themes[0];
    assert_eq!(midnight.name, "Midnight");
    assert_eq!(midnight.source_theme_id.as_deref(), Some("neon80s"));
    assert_eq!(midnight.tokens["colors"]["accent"], "#ff27d8");
    assert!(
        CatalogStore::in_dir(&catalogs.join(ID_A))
            .with_reader(|_| Ok(()))
            .expect("catalog")
            .is_some()
    );
    // Work directories are gone; data outside the scope is untouched.
    for root in Root::ALL {
        assert!(
            !fixture
                .dirs
                .root(root)
                .join(scope::RESTORE_WORK_DIR)
                .exists()
        );
    }
    assert!(fixture.dirs.data.join("launch-logs/1.log").exists());

    // Next start: nothing left to do, data unchanged.
    assert_eq!(restore::recover(&fixture.dirs), None);
    assert_eq!(tree(&fixture.dirs.config), config_before);
}

#[test]
fn external_references_are_kept_and_never_written() {
    let fixture = fixture();
    populate(&fixture);
    let snaps = fixture.outside.join("snaps");
    write(&snaps.join("pacman.png"), b"png");
    let (archive, _) = snapshot_and_export(&fixture, "ext.ymdb-backup.zip");
    let media_before = fs::read(fixture.dirs.config.join("media.json")).expect("media");
    fs::remove_dir_all(&snaps).expect("remove external");

    let preview = stage(&fixture, &archive).expect("stage");
    restore::schedule(&fixture.dirs).expect("schedule");
    restore::recover(&fixture.dirs).expect("outcome");

    assert!(
        preview
            .external_references
            .iter()
            .any(|reference| reference.path == snaps && !reference.exists)
    );
    assert_eq!(
        fs::read(fixture.dirs.config.join("media.json")).expect("media"),
        media_before
    );
    assert!(!snaps.exists());
}

#[test]
fn copies_sqlite_databases_in_wal_mode_consistently() {
    let fixture = fixture();
    let source = fixture.outside.join("wal.sqlite");
    let writer = Connection::open(&source).expect("open");
    writer
        .execute_batch(
            "PRAGMA journal_mode = WAL; PRAGMA wal_autocheckpoint = 0;
             CREATE TABLE t (x INTEGER);
             INSERT INTO t VALUES (1), (2);",
        )
        .expect("setup");
    assert!(fixture.outside.join("wal.sqlite-wal").exists());
    // An uncommitted transaction of another connection is not copied.
    writer
        .execute_batch("BEGIN; INSERT INTO t VALUES (3);")
        .expect("begin");
    let reader = Connection::open(&source).expect("reader");
    let copy = fixture.outside.join("copy.sqlite");

    sqlite::backup(&reader, &copy).expect("backup");

    assert_eq!(
        Connection::open(&copy)
            .expect("copy")
            .query_row("SELECT count(*) FROM t", [], |row| row.get::<_, i64>(0))
            .expect("count"),
        2
    );
    let mode: String = Connection::open(&copy)
        .expect("copy")
        .query_row("PRAGMA journal_mode", [], |row| row.get(0))
        .expect("mode");
    assert_eq!(mode, "delete");
    assert!(!fixture.outside.join("copy.sqlite-wal").exists());
    writer.execute_batch("COMMIT").expect("commit");
}

#[test]
fn an_export_waits_for_changes_in_progress() {
    let fixture = fixture();
    let config = fixture.dirs.config.clone();
    write(&config.join("lists.json"), b"0");
    write(&config.join("saved-filters.json"), b"0");
    let stop = Arc::new(AtomicBool::new(false));
    // A change that writes two files under the shared gate.
    let writer = {
        let stop = Arc::clone(&stop);
        let config = config.clone();
        std::thread::spawn(move || {
            let mut value = 0_u64;
            while !stop.load(Ordering::Relaxed) {
                value += 1;
                let Some(_change) = gate::shared() else {
                    std::thread::yield_now();
                    continue;
                };
                fs::write(config.join("lists.json"), value.to_string()).expect("write");
                std::thread::sleep(std::time::Duration::from_millis(1));
                fs::write(config.join("saved-filters.json"), value.to_string()).expect("write");
            }
        })
    };
    let catalog = |id: &str| CatalogStore::in_dir(&fixture.dirs.data.join(id));

    for _ in 0..20 {
        let work = export::work_dir(&fixture.dirs).expect("work");
        let files = {
            let _quiet = gate::exclusive();
            export::snapshot(
                &SnapshotSources {
                    dirs: &fixture.dirs,
                    catalog: &catalog,
                    interface_preferences: None,
                },
                &work,
                &Progress::new(Phase::Snapshot),
            )
            .expect("snapshot")
        };
        let read = |name: &str| {
            let file = files
                .iter()
                .find(|file| file.path == format!("config/{name}"))
                .expect("file");
            fs::read(&file.file).expect("read")
        };
        assert_eq!(read("lists.json"), read("saved-filters.json"));
        fs::remove_dir_all(&work).expect("remove");
    }
    stop.store(true, Ordering::Relaxed);
    writer.join().expect("writer");
}

#[test]
fn a_failed_export_leaves_no_archive() {
    let fixture = fixture();
    populate(&fixture);
    let progress = Progress::new(Phase::Snapshot);
    let work = export::work_dir(&fixture.dirs).expect("work");
    let catalog =
        |id: &str| CatalogStore::in_dir(&fixture.dirs.data.join("installations").join(id));
    let files = export::snapshot(
        &SnapshotSources {
            dirs: &fixture.dirs,
            catalog: &catalog,
            interface_preferences: None,
        },
        &work,
        &progress,
    )
    .expect("snapshot");
    let destination = fixture.outside.join("cancelled.ymdb-backup.zip");
    progress.request_cancel();

    let error = export::finish(&files, &destination, VERSION, &progress).expect_err("cancelled");

    assert!(error.is_cancelled());
    assert!(!destination.exists());
    assert!(!export::partial_path(&destination).exists());
    // A destination in a missing directory fails without leftovers too.
    let missing = fixture.outside.join("missing").join("x.zip");
    assert!(export::finish(&files, &missing, VERSION, &Progress::new(Phase::Writing)).is_err());
    assert!(!export::partial_path(&missing).exists());
}

/// Writes an archive with the given entries (name, content) and manifest.
fn craft(path: &Path, manifest: Option<&Manifest>, entries: &[(&str, &[u8])]) {
    let mut zip = ZipWriter::new(File::create(path).expect("create"));
    for (name, content) in entries {
        zip.start_file(*name, SimpleFileOptions::default())
            .expect("start");
        zip.write_all(content).expect("write");
    }
    if let Some(manifest) = manifest {
        zip.start_file(
            format!("{ARCHIVE_ROOT}/{MANIFEST_NAME}"),
            SimpleFileOptions::default(),
        )
        .expect("start");
        zip.write_all(&serde_json::to_vec(manifest).expect("json"))
            .expect("write");
    }
    zip.finish().expect("finish");
}

fn manifest_for(entries: &[(&str, &[u8])]) -> Manifest {
    let items = entries
        .iter()
        .map(|(path, content)| {
            let classified = scope::classify(path).expect("classify");
            let (size, sha256) = copy_hashed(
                &mut &content[..],
                &mut std::io::sink(),
                u64::MAX,
                &Progress::new(Phase::Writing),
            )
            .expect("hash");
            manifest::ManifestItem {
                path: (*path).to_owned(),
                category: classified.category,
                kind: classified.kind,
                size,
                sha256,
            }
        })
        .collect();
    Manifest {
        format: manifest::FORMAT.to_owned(),
        format_version: manifest::FORMAT_VERSION,
        app_version: VERSION.to_owned(),
        created_at: 1,
        platform: manifest::Platform::current(),
        schemas: manifest::Schemas::current(),
        items,
    }
}

const LISTS: (&str, &[u8]) = ("config/lists.json", b"{\"lists\":[]}");

fn entry(path: &str) -> String {
    format!("{ARCHIVE_ROOT}/{path}")
}

/// Stages `archive` and checks that it is refused without touching anything.
fn assert_refused(fixture: &Fixture, archive: &Path, expected: &str) {
    let before = (tree(&fixture.dirs.config), tree(&fixture.dirs.data));
    let error = stage(fixture, archive).expect_err("refused");
    assert!(
        error.0.contains(expected),
        "{error} does not contain {expected}"
    );
    assert_eq!(
        (tree(&fixture.dirs.config), tree(&fixture.dirs.data)),
        before
    );
    assert!(!restore::has_pending(&fixture.dirs));
}

#[test]
fn refuses_damaged_or_incomplete_archives() {
    let fixture = fixture();
    populate(&fixture);
    let (good, _) = snapshot_and_export(&fixture, "good.ymdb-backup.zip");
    let content = fs::read(&good).expect("read");

    let truncated = fixture.outside.join("truncated.zip");
    fs::write(&truncated, &content[..content.len() / 2]).expect("write");
    assert_refused(&fixture, &truncated, "not a valid ZIP");

    let missing = fixture.outside.join("missing.zip");
    let manifest = manifest_for(&[LISTS]);
    craft(&missing, Some(&manifest), &[]);
    assert_refused(&fixture, &missing, "missing from the archive");

    let unlisted = fixture.outside.join("unlisted.zip");
    craft(
        &unlisted,
        Some(&manifest_for(&[])),
        &[(&entry(LISTS.0), LISTS.1)],
    );
    assert_refused(&fixture, &unlisted, "not listed in the manifest");

    let no_manifest = fixture.outside.join("no-manifest.zip");
    craft(&no_manifest, None, &[(&entry(LISTS.0), LISTS.1)]);
    assert_refused(&fixture, &no_manifest, "no YMDB backup manifest");
}

#[test]
fn refuses_wrong_checksums() {
    let fixture = fixture();
    let archive = fixture.outside.join("checksum.zip");
    let mut manifest = manifest_for(&[LISTS]);
    manifest.items[0].sha256 = "0".repeat(64);
    craft(&archive, Some(&manifest), &[(&entry(LISTS.0), LISTS.1)]);

    assert_refused(&fixture, &archive, "checksum");
}

#[test]
fn refuses_incompatible_manifests() {
    let fixture = fixture();
    for (field, value) in [
        ("formatVersion", serde_json::json!(2)),
        ("appVersion", serde_json::json!("99.0.0")),
        ("format", serde_json::json!("other")),
    ] {
        let mut manifest = serde_json::to_value(manifest_for(&[])).expect("json");
        manifest[field] = value;
        let archive = fixture.outside.join(format!("{field}.zip"));
        let mut zip = ZipWriter::new(File::create(&archive).expect("create"));
        zip.start_file(entry(MANIFEST_NAME), SimpleFileOptions::default())
            .expect("start");
        zip.write_all(manifest.to_string().as_bytes())
            .expect("write");
        zip.finish().expect("finish");

        let error = stage(&fixture, &archive).expect_err(field);
        assert!(!error.0.is_empty(), "{field}");
    }
}

#[test]
fn refuses_path_traversal_and_unexpected_entries() {
    let fixture = fixture();
    for name in [
        "ymdb-backup/../evil.json",
        "ymdb-backup/config/../../evil.json",
        "/etc/passwd",
        "C:/Windows/evil.dll",
        "ymdb-backup\\config\\lists.json",
        "other/config/lists.json",
        "ymdb-backup/config/window-state.json",
        "ymdb-backup/data/launch-logs/1.log",
    ] {
        let archive = fixture.outside.join("traversal.zip");
        craft(&archive, Some(&manifest_for(&[])), &[(name, b"x")]);
        assert!(stage(&fixture, &archive).is_err(), "{name}");
        assert!(
            !fixture
                .outside
                .parent()
                .expect("parent")
                .join("evil.json")
                .exists()
        );
    }
}

#[test]
fn refuses_symbolic_links_and_duplicates() {
    let fixture = fixture();
    let archive = fixture.outside.join("symlink.zip");
    let mut zip = ZipWriter::new(File::create(&archive).expect("create"));
    zip.add_symlink(entry(LISTS.0), "/etc/passwd", SimpleFileOptions::default())
        .expect("symlink");
    zip.finish().expect("finish");
    assert_refused(&fixture, &archive, "symbolic link");

    let duplicates = fixture.outside.join("duplicates.zip");
    craft(
        &duplicates,
        Some(&manifest_for(&[LISTS])),
        &[
            (&entry(LISTS.0), LISTS.1),
            (&entry("config/LISTS.json"), LISTS.1),
        ],
    );
    assert_refused(&fixture, &duplicates, "more than once");

    let mut manifest = manifest_for(&[LISTS]);
    manifest.items.push(manifest.items[0].clone());
    let listed_twice = fixture.outside.join("twice.zip");
    craft(
        &listed_twice,
        Some(&manifest),
        &[(&entry(LISTS.0), LISTS.1)],
    );
    assert_refused(&fixture, &listed_twice, "more than once");
}

#[test]
fn refuses_decompression_bombs() {
    let fixture = fixture();
    let zeros = vec![0_u8; 4 * 1024 * 1024];
    let archive = fixture.outside.join("bomb.zip");
    let mut zip = ZipWriter::new(File::create(&archive).expect("create"));
    zip.start_file(
        entry(LISTS.0),
        SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated),
    )
    .expect("start");
    zip.write_all(&zeros).expect("write");
    zip.start_file(entry(MANIFEST_NAME), SimpleFileOptions::default())
        .expect("start");
    zip.write_all(&serde_json::to_vec(&manifest_for(&[(LISTS.0, &zeros)])).expect("json"))
        .expect("write");
    zip.finish().expect("finish");

    let strict = Limits {
        max_ratio: 100,
        ..Limits::default()
    };
    assert!(
        archive::open(&archive, &strict)
            .err()
            .is_some_and(|error| error.0.contains("compression ratio"))
    );
    let small = Limits {
        max_total_size: 1024,
        ..Limits::default()
    };
    assert!(
        archive::open(&archive, &small)
            .err()
            .is_some_and(|error| error.0.contains("larger"))
    );
    // Content longer than declared is never written past the declared size.
    let mut sink = Vec::new();
    assert!(matches!(
        copy_hashed(
            &mut &zeros[..],
            &mut sink,
            1000,
            &Progress::new(Phase::Extracting)
        ),
        Err(CopyError::TooLarge)
    ));
    assert!(sink.len() <= 256 * 1024);
}

#[test]
fn a_cancelled_import_check_changes_nothing() {
    let fixture = fixture();
    populate(&fixture);
    let (archive, _) = snapshot_and_export(&fixture, "cancel.ymdb-backup.zip");
    let before = tree(&fixture.dirs.config);
    let progress = Progress::new(Phase::Checking);
    progress.request_cancel();

    let error = restore::stage(&archive, &fixture.dirs, VERSION, &progress).expect_err("cancelled");

    assert!(error.is_cancelled());
    assert_eq!(tree(&fixture.dirs.config), before);
    assert!(!fixture.dirs.config.join(scope::RESTORE_WORK_DIR).exists());
}

#[test]
fn a_discarded_import_removes_the_staged_data() {
    let fixture = fixture();
    populate(&fixture);
    let (archive, _) = snapshot_and_export(&fixture, "discard.ymdb-backup.zip");
    stage(&fixture, &archive).expect("stage");
    assert!(fixture.dirs.data.join(scope::RESTORE_WORK_DIR).exists());

    restore::discard(&fixture.dirs);

    for root in Root::ALL {
        assert!(
            !fixture
                .dirs
                .root(root)
                .join(scope::RESTORE_WORK_DIR)
                .exists()
        );
    }
    assert_eq!(restore::recover(&fixture.dirs), None);
}

#[test]
fn refuses_staged_data_the_stores_cannot_read() {
    let fixture = fixture();
    let invalid = ("config/lists.json", &b"{ not json"[..]);
    let archive = fixture.outside.join("invalid.zip");
    craft(
        &archive,
        Some(&manifest_for(&[invalid])),
        &[(&entry(invalid.0), invalid.1)],
    );

    assert_refused(&fixture, &archive, "cannot be used");

    let invalid = (
        "config/custom-themes.json",
        &br#"{ "schemaVersion": 99, "themes": [] }"#[..],
    );
    craft(
        &archive,
        Some(&manifest_for(&[invalid])),
        &[(&entry(invalid.0), invalid.1)],
    );
    assert_refused(&fixture, &archive, "custom themes");

    let database = (
        "data/support-files.sqlite",
        &b"SQLite format 3\0 but not really a database file at all......."[..],
    );
    craft(
        &archive,
        Some(&manifest_for(&[database])),
        &[(&entry(database.0), database.1)],
    );
    assert_refused(&fixture, &archive, "support-files.sqlite");
}

#[test]
fn a_failed_swap_puts_the_previous_data_back() {
    let fixture = fixture();
    populate(&fixture);
    let (archive, _) = snapshot_and_export(&fixture, "fail.ymdb-backup.zip");
    ListsStore::in_dir(&fixture.dirs.config)
        .save(&Lists::new())
        .expect("change lists");
    let config_before = tree(&fixture.dirs.config);
    let data_before = tree(&fixture.dirs.data);
    stage(&fixture, &archive).expect("stage");
    restore::schedule(&fixture.dirs).expect("schedule");
    // A staged catalog damaged between the check and the restart.
    let staged = fixture
        .dirs
        .data
        .join(scope::RESTORE_WORK_DIR)
        .join("staged/installations")
        .join(ID_A)
        .join("catalog.sqlite");
    fs::write(&staged, b"short").expect("damage");

    let outcome = restore::recover(&fixture.dirs).expect("outcome");

    assert!(
        matches!(
            &outcome,
            RestoreOutcome::Failed {
                previous_data_kept: true,
                ..
            }
        ),
        "{outcome:?}"
    );
    assert_eq!(tree(&fixture.dirs.config), config_before);
    assert_eq!(tree(&fixture.dirs.data), data_before);
    assert!(!restore::has_pending(&fixture.dirs));
}

#[test]
fn an_interrupted_swap_is_rolled_back_at_the_next_start() {
    let fixture = fixture();
    populate(&fixture);
    let (archive, _) = snapshot_and_export(&fixture, "crash.ymdb-backup.zip");
    let lists_now = b"{\"lists\": []}".to_vec();
    fs::write(fixture.dirs.config.join("lists.json"), &lists_now).expect("lists");
    fs::remove_file(fixture.dirs.config.join("saved-filters.json")).expect("remove");
    let config_before = tree(&fixture.dirs.config);
    let data_before = tree(&fixture.dirs.data);
    stage(&fixture, &archive).expect("stage");
    restore::schedule(&fixture.dirs).expect("schedule");

    // Simulate a crash in the middle of the swap: the transaction says
    // `swapping`, lists.json was moved aside and replaced, saved-filters.json
    // (absent before) was moved in, the rest was not moved yet.
    let work = fixture.dirs.config.join(scope::RESTORE_WORK_DIR);
    let transaction_path = work.join("transaction.json");
    let mut transaction: serde_json::Value =
        serde_json::from_slice(&fs::read(&transaction_path).expect("read")).expect("json");
    let originals: Vec<String> = scope::replaced_names(Root::Config)
        .into_iter()
        .filter(|name| fixture.dirs.config.join(name).exists())
        .collect();
    let data_originals: Vec<String> = scope::replaced_names(Root::Data)
        .into_iter()
        .filter(|name| fixture.dirs.data.join(name).exists())
        .collect();
    transaction["phase"] = "swapping".into();
    transaction["originals"] = serde_json::json!({ "config": originals, "data": data_originals });
    fs::write(&transaction_path, transaction.to_string()).expect("write");
    fs::create_dir_all(work.join("rollback")).expect("rollback");
    fs::rename(
        fixture.dirs.config.join("lists.json"),
        work.join("rollback/lists.json"),
    )
    .expect("aside");
    fs::rename(
        work.join("staged/lists.json"),
        fixture.dirs.config.join("lists.json"),
    )
    .expect("in");
    fs::rename(
        work.join("staged/saved-filters.json"),
        fixture.dirs.config.join("saved-filters.json"),
    )
    .expect("in");

    let outcome = restore::recover(&fixture.dirs).expect("outcome");

    assert!(
        matches!(
            &outcome,
            RestoreOutcome::Failed {
                previous_data_kept: true,
                ..
            }
        ),
        "{outcome:?}"
    );
    assert_eq!(tree(&fixture.dirs.config), config_before);
    assert_eq!(tree(&fixture.dirs.data), data_before);
    assert_eq!(
        fs::read(fixture.dirs.config.join("lists.json")).expect("lists"),
        lists_now
    );
}

#[test]
fn an_unreadable_transaction_changes_nothing() {
    let fixture = fixture();
    populate(&fixture);
    let path = fixture
        .dirs
        .config
        .join(scope::RESTORE_WORK_DIR)
        .join("transaction.json");
    write(&path, b"{ broken");
    let before = tree(&fixture.dirs.config);

    let outcome = restore::recover(&fixture.dirs).expect("outcome");

    assert!(matches!(
        outcome,
        RestoreOutcome::Failed {
            previous_data_kept: false,
            ..
        }
    ));
    assert_eq!(tree(&fixture.dirs.config), before);
}

#[test]
fn a_second_import_waits_for_the_pending_restore() {
    let fixture = fixture();
    let (archive, _) = snapshot_and_export(&fixture, "pending.ymdb-backup.zip");
    stage(&fixture, &archive).expect("stage");
    restore::schedule(&fixture.dirs).expect("schedule");

    assert!(stage(&fixture, &archive).is_err());
    restore::discard(&fixture.dirs);
    assert!(restore::has_pending(&fixture.dirs));
}

#[test]
fn restoring_an_empty_backup_resets_the_owned_data_only() {
    let fixture = fixture();
    let (archive, _) = snapshot_and_export(&fixture, "reset.ymdb-backup.zip");
    populate(&fixture);

    let preview = stage(&fixture, &archive).expect("stage");
    restore::schedule(&fixture.dirs).expect("schedule");
    restore::recover(&fixture.dirs).expect("outcome");

    assert!(preview.notes.is_empty() || preview.total_size > 0);
    for name in [
        "lists.json",
        "controllers.json",
        "custom-themes.json",
        "installations",
        "controller-profiles",
    ] {
        assert!(!fixture.dirs.config.join(name).exists(), "{name}");
    }
    assert!(!fixture.dirs.data.join("installations").exists());
    assert!(!fixture.dirs.data.join("support-files.sqlite").exists());
    assert!(fixture.dirs.config.join("window-state.json").exists());
    assert!(fixture.dirs.data.join("launch-logs/1.log").exists());
}

/// Export and restore of a copy of real application data. Never point it at
/// the data of a YMDB in use: the catalogs are opened (and may be upgraded).
///
/// `YMDB_TEST_APP_DATA_DIR`: a directory with `config/` and `data/` (for
/// example copies of the platform directories). `YMDB_TEST_RESTORE_DIR`: an
/// isolated `YMDB_DATA_DIR` that receives the restore; with
/// `YMDB_TEST_LEAVE_PENDING=1` the restore is only scheduled, so that the
/// application applies it at its next start.
#[test]
#[ignore = "needs a copy of real application data"]
fn real_data_round_trip() {
    let source = PathBuf::from(std::env::var_os("YMDB_TEST_APP_DATA_DIR").expect("source dir"));
    let target = PathBuf::from(std::env::var_os("YMDB_TEST_RESTORE_DIR").expect("restore dir"));
    let dirs = AppDirs::new(&source.join("config"), &source.join("data"));
    let restore_dirs = AppDirs::new(&target.join("config"), &target.join("data"));
    let archive = target
        .parent()
        .expect("parent")
        .join("real-data-test.ymdb-backup.zip");
    let _ = fs::remove_file(&archive);

    let started = std::time::Instant::now();
    let progress = Progress::new(Phase::Snapshot);
    let work = export::work_dir(&dirs).expect("work");
    let data = dirs.data.clone();
    let catalog = move |id: &str| CatalogStore::in_dir(&data.join("installations").join(id));
    let files = {
        let _quiet = gate::exclusive();
        export::snapshot(
            &SnapshotSources {
                dirs: &dirs,
                catalog: &catalog,
                interface_preferences: None,
            },
            &work,
            &progress,
        )
        .expect("snapshot")
    };
    let snapshot_time = started.elapsed();
    let size = export::finish(&files, &archive, VERSION, &progress).expect("export");
    fs::remove_dir_all(&work).expect("remove work");
    let manifest = archive::open(&archive, &Limits::default())
        .expect("open")
        .manifest;
    println!(
        "exported {} items ({} bytes of data) into {size} bytes in {:?} (snapshot {snapshot_time:?})",
        manifest.items.len(),
        manifest.total_size(),
        started.elapsed()
    );
    for item in &manifest.items {
        println!("  {:?} {} {}", item.category, item.path, item.size);
    }

    let started = std::time::Instant::now();
    let preview = restore::stage(
        &archive,
        &restore_dirs,
        VERSION,
        &Progress::new(Phase::Checking),
    )
    .expect("stage");
    println!(
        "staged in {:?}: {} categories, {} missing external references",
        started.elapsed(),
        preview.categories.len(),
        preview
            .external_references
            .iter()
            .filter(|reference| !reference.exists)
            .count()
    );
    restore::schedule(&restore_dirs).expect("schedule");
    if std::env::var_os("YMDB_TEST_LEAVE_PENDING").is_some() {
        println!("restore scheduled in {}", target.display());
        return;
    }
    let outcome = restore::recover(&restore_dirs).expect("outcome");
    assert!(
        matches!(outcome, RestoreOutcome::Restored { .. }),
        "{outcome:?}"
    );
    for item in &manifest.items {
        let restored = item.path.split('/').skip(1).fold(
            target.join(item.path.split('/').next().expect("root")),
            |path, part| path.join(part),
        );
        assert_eq!(
            fs::metadata(&restored).expect("restored").len(),
            item.size,
            "{}",
            item.path
        );
    }
    println!("restored and verified {} items", manifest.items.len());
}
