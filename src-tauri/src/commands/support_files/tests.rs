//! Support file commands, from a file on disk to two catalog projections.

use std::sync::atomic::AtomicBool;

use super::*;
use crate::catalog::classification::machine_classifications;
use crate::catalog::import::{ImportProgress, import_from_reader};
use crate::catalog::store::CatalogSource;
use crate::support_files::tests::{CATLIST, CATVER, SERIES};

const MACHINES: &str = include_str!("../../catalog/testdata/search-machines.xml");
const OTHER_MACHINES: &str = include_str!("../../catalog/testdata/listxml-sample.xml");

fn catalog(dir: &Path, machines: &str) -> CatalogStore {
    let store = CatalogStore::in_dir(dir);
    import_from_reader(
        machines.as_bytes(),
        &store,
        CatalogSource {
            mame_release: "0.280".to_owned(),
            mame_revision: "mame0280".to_owned(),
            executable_path: "/opt/mame/mame".into(),
            executable_size: Some(1),
            executable_modified_at: Some(1),
        },
        &AtomicBool::new(false),
        &ImportProgress::default(),
        || Ok(()),
    )
    .expect("catalog import");
    store
}

fn write(dir: &Path, name: &str, content: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, content).expect("write");
    path
}

fn classifications(
    store: &CatalogStore,
    name: &str,
) -> crate::catalog::classification::MachineClassifications {
    store
        .with_reader(|connection| Ok(machine_classifications(connection, name)?))
        .expect("read")
        .expect("catalog")
}

#[test]
fn imports_a_file_after_a_preview_and_projects_it_into_two_catalogs() {
    let dir = tempfile::tempdir().expect("temp dir");
    let state = SupportFilesState::new(Some(&dir.path().join("data")));
    let first = catalog(&dir.path().join("first"), MACHINES);
    let second = catalog(&dir.path().join("second"), OTHER_MACHINES);
    let files = dir.path().join("files ✓");
    std::fs::create_dir_all(&files).expect("dir");
    let catver = write(&files, "catver.ini", CATVER);

    let preview = preview_file(&state, &catver, None).expect("preview");
    assert_eq!(preview.format, SupportFileFormat::Catver);
    assert_eq!(preview.file_name, "catver.ini");
    assert_eq!(preview.declared_version.as_deref(), Some("0.288"));
    assert!(preview.replaces.is_none());
    assert!(
        preview
            .dimensions
            .iter()
            .all(|entry| entry.active_after_import)
    );
    // Nothing is stored before the import is applied.
    assert!(state.view().expect("view").sources.is_empty());

    let view = state.apply_import(preview.import_id, 42).expect("apply");
    assert_eq!(view.sources[0].imported_at, 42);
    // The original file is never modified.
    assert_eq!(std::fs::read_to_string(&catver).expect("read"), CATVER);
    assert!(matches!(
        state.apply_import(preview.import_id, 43),
        Err(CommandError::InvalidRequest { .. })
    ));

    for store in [&first, &second] {
        assert_eq!(sync_catalog(store, state.target()), None);
    }
    assert_eq!(
        classifications(&first, "sf2").categories,
        ["Fighter / Versus"]
    );
    assert_eq!(
        classifications(&first, "sf2").version_added.as_deref(),
        Some("0.33rc1")
    );
    // `kof98` is only in the second catalog; `sf2` only in the first.
    assert!(classifications(&second, "sf2").categories.is_empty());
    assert_eq!(
        classifications(&second, "pacman").categories,
        ["Maze / Collect"]
    );
}

#[test]
fn reimport_requires_the_same_format_and_reports_what_it_replaces() {
    let dir = tempfile::tempdir().expect("temp dir");
    let state = SupportFilesState::new(Some(dir.path()));
    let series = write(dir.path(), "series.ini", SERIES);
    let preview = preview_file(&state, &series, None).expect("preview");
    state.apply_import(preview.import_id, 1).expect("apply");

    let catlist = write(dir.path(), "catlist.ini", CATLIST);
    let refused = preview_file(&state, &catlist, Some(SupportFileFormat::Series));
    assert!(matches!(refused, Err(CommandError::InvalidFile { .. })));

    let preview = preview_file(&state, &series, Some(SupportFileFormat::Series)).expect("preview");
    let replaced = preview.replaces.expect("replaces");
    assert_eq!(replaced.format, SupportFileFormat::Series);
    assert_eq!(replaced.imported_at, 1);

    let broken = write(
        dir.path(),
        "broken.ini",
        ";; MATURE.ini ;;\n[ROOT_FOLDER]\npacman\n",
    );
    let error = preview_file(&state, &broken, None).expect_err("refused");
    assert!(
        matches!(&error, CommandError::InvalidFile { message } if message.contains("broken.ini")),
        "{error:?}"
    );
}

#[test]
fn alternative_sources_and_removal_update_the_projection() {
    let dir = tempfile::tempdir().expect("temp dir");
    let state = SupportFilesState::new(Some(&dir.path().join("data")));
    let store = catalog(&dir.path().join("catalog"), MACHINES);
    for (name, content) in [("catver.ini", CATVER), ("catlist.ini", CATLIST)] {
        let path = write(dir.path(), name, content);
        let preview = preview_file(&state, &path, None).expect("preview");
        if name == "catlist.ini" {
            // Category is already fed by catver.ini.
            assert!(!preview.dimensions[0].active_after_import);
        }
        state.apply_import(preview.import_id, 1).expect("apply");
    }
    assert_eq!(sync_catalog(&store, state.target()), None);
    assert_eq!(
        classifications(&store, "pacman").categories,
        ["Maze / Collect"]
    );

    state
        .set_active(Dimension::Category, Some(SupportFileFormat::Catlist))
        .expect("switch");
    assert_eq!(sync_catalog(&store, state.target()), None);
    assert_eq!(
        classifications(&store, "pacman").categories,
        ["Arcade / Maze / Collect"]
    );

    let view = state.remove(SupportFileFormat::Catlist).expect("remove");
    assert_eq!(view.dimensions[1].active_source, None);
    assert_eq!(
        view.dimensions[1].available_sources,
        [SupportFileFormat::Catver]
    );
    assert_eq!(sync_catalog(&store, state.target()), None);
    assert!(classifications(&store, "pacman").categories.is_empty());
    // Version Added still comes from catver.ini.
    assert_eq!(
        classifications(&store, "pacman").version_added.as_deref(),
        Some("0.1")
    );
}

#[test]
fn an_unreadable_repository_is_reported_without_blocking_the_catalog() {
    let dir = tempfile::tempdir().expect("temp dir");
    let data = dir.path().join("data");
    std::fs::create_dir_all(&data).expect("dir");
    std::fs::write(data.join("support-files.sqlite"), "not a database").expect("write");
    let state = SupportFilesState::new(Some(&data));
    let store = catalog(&dir.path().join("catalog"), MACHINES);

    let error = sync_catalog(&store, state.target());
    assert!(error.is_some());
    assert!(classifications(&store, "pacman").categories.is_empty());
}

#[test]
fn serializes_to_the_frontend_contract() {
    let dir = tempfile::tempdir().expect("temp dir");
    let state = SupportFilesState::new(Some(dir.path()));
    let path = write(dir.path(), "catver.ini", CATVER);
    let preview = preview_file(&state, &path, None).expect("preview");
    let json = serde_json::to_value(&preview).expect("serialize");
    assert_eq!(json["format"], "catver");
    assert_eq!(json["declaredVersion"], "0.288");
    assert_eq!(
        json["dimensions"][1],
        serde_json::json!({
            "dimension": "versionAdded",
            "machineCount": 13,
            "valueCount": 13,
            "activeAfterImport": true
        })
    );
    assert_eq!(json["replaces"], serde_json::Value::Null);

    let view = state.apply_import(preview.import_id, 5).expect("apply");
    let json = serde_json::to_value(&view).expect("serialize");
    assert_eq!(
        json["dimensions"][1],
        serde_json::json!({
            "dimension": "category",
            "activeSource": "catver",
            "availableSources": ["catver"],
            "revision": 1
        })
    );
    assert_eq!(json["sources"][0]["fileName"], "catver.ini");
    assert_eq!(json["sources"][0]["importedAt"], 5);
    assert_eq!(
        json["sources"][0]["dimensions"][0],
        serde_json::json!({ "dimension": "category", "machineCount": 13, "valueCount": 10, "active": true })
    );

    let status = ClassificationStatus {
        dimensions: vec![ClassifiedDimension {
            dimension: Dimension::Series,
            source: SupportFileFormat::Series,
            machine_count: 2,
            value_count: 1,
        }],
        sync_error: None,
    };
    assert_eq!(
        serde_json::to_value(status).expect("serialize"),
        serde_json::json!({
            "dimensions": [{ "dimension": "series", "source": "series", "machineCount": 2, "valueCount": 1 }],
            "syncError": null
        })
    );

    let filters: crate::catalog::search::request::MachineFilters =
        serde_json::from_value(serde_json::json!({
            "genres": ["Shooter"],
            "categories": ["Shooter / Flying Vertical"],
            "series": ["194X"],
            "versionsAdded": ["0.37b5"],
            "versionAddedRange": { "from": "0.100", "to": null }
        }))
        .expect("deserialize");
    assert_eq!(filters.series, ["194X"]);
    assert_eq!(
        filters
            .version_added_range
            .and_then(|range| range.from)
            .as_deref(),
        Some("0.100")
    );
}

#[test]
fn reports_classification_values_missing_from_a_catalog() {
    let dir = tempfile::tempdir().expect("temp dir");
    let state = SupportFilesState::new(Some(&dir.path().join("data")));
    let store = catalog(&dir.path().join("catalog"), MACHINES);
    let path = write(dir.path(), "series.ini", SERIES);
    let preview = preview_file(&state, &path, None).expect("preview");
    state.apply_import(preview.import_id, 1).expect("apply");
    assert_eq!(sync_catalog(&store, state.target()), None);

    let values = |dimension, values: &[&str]| DimensionValues {
        dimension,
        values: values.iter().map(|value| (*value).to_owned()).collect(),
    };
    let missing = missing_values(
        &store,
        &[
            values(Dimension::Series, &["194X", "Pong", "Galaga"]),
            values(Dimension::Genre, &["Shooter"]),
            values(Dimension::Category, &[]),
        ],
    )
    .expect("check");
    // Galaga is in series.ini but not in this catalog; Genre has no source.
    assert_eq!(
        missing,
        [
            values(Dimension::Series, &["194X", "Galaga"]),
            values(Dimension::Genre, &["Shooter"]),
        ]
    );
}
