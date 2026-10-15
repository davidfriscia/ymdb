//! Lists state tests without Tauri: persistence, failures, imports and
//! searches restricted to Favorites on the catalogs of two MAME versions.

use std::fs;
use std::path::Path;
use std::sync::atomic::AtomicBool;

use tempfile::TempDir;

use super::*;
use crate::catalog::import::{ImportProgress, import_from_reader, import_software_from_reader};
use crate::catalog::search::machines::search_machines;
use crate::catalog::search::request::{
    FacetRequest, MachineFacet, MachineFilters, MachineSearchRequest, SoftwareFilters,
    SoftwareSearchRequest,
};
use crate::catalog::search::software::search_software;
use crate::catalog::store::{CatalogSource, CatalogStore};
use crate::commands::search::search;
use crate::lists::exchange;
use crate::lists::model::FAVORITES;
use crate::lists::store::LISTS_FILE_NAME;

const NEW_MACHINES: &str = include_str!("../../catalog/testdata/search-machines.xml");
const OLD_MACHINES: &str = include_str!("../../catalog/testdata/listxml-sample.xml");
const SOFTWARE: &str = include_str!("../../catalog/testdata/listsoftware-sample.xml");

fn state(dir: &Path) -> ListsState {
    ListsState::new(Some(&dir.join("config")))
}

fn favorite(state: &ListsState, entry: &ListEntry, member: bool) {
    state
        .update(|lists| {
            lists.set_membership(FAVORITES, entry, member)?;
            Ok(())
        })
        .expect("update");
}

fn entries(state: &ListsState) -> Vec<ListEntry> {
    state
        .read(|lists| list_view(lists, FAVORITES))
        .expect("read")
        .expect("favorites")
        .entries
}

#[test]
fn changes_are_saved_and_survive_a_restart() {
    let dir = tempfile::tempdir().expect("temp dir");
    let first = state(dir.path());
    favorite(&first, &ListEntry::machine("1942"), true);
    favorite(&first, &ListEntry::software("nes", "smb"), true);
    favorite(&first, &ListEntry::machine("1942"), false);

    let restarted = state(dir.path());

    assert_eq!(entries(&restarted), [ListEntry::software("nes", "smb")]);
}

#[test]
fn a_failed_save_keeps_the_previous_lists() {
    let dir = tempfile::tempdir().expect("temp dir");
    let state = state(dir.path());
    favorite(&state, &ListEntry::machine("1942"), true);
    // A directory where the temporary file goes makes the next save fail.
    fs::create_dir_all(
        dir.path()
            .join("config")
            .join(format!("{LISTS_FILE_NAME}.tmp")),
    )
    .expect("dir");

    let result = state.update(|lists| {
        lists.set_membership(FAVORITES, &ListEntry::machine("pacman"), true)?;
        Ok(())
    });

    assert!(matches!(result, Err(CommandError::Settings { .. })));
    assert_eq!(entries(&state), [ListEntry::machine("1942")]);
    let saved = ListsStore::in_dir(&dir.path().join("config"))
        .load()
        .expect("load");
    assert_eq!(saved.get(FAVORITES).expect("favorites").entries.len(), 1);
}

#[test]
fn an_invalid_file_is_reported_until_fixed_and_never_overwritten() {
    let dir = tempfile::tempdir().expect("temp dir");
    let config = dir.path().join("config");
    fs::create_dir_all(&config).expect("dir");
    let path = config.join(LISTS_FILE_NAME);
    fs::write(&path, "{ corrupted").expect("write");
    let state = state(dir.path());

    assert!(matches!(
        state.read(|_| ()),
        Err(CommandError::Settings { .. })
    ));
    let change = state.update(|lists| {
        lists.set_membership(FAVORITES, &ListEntry::machine("1942"), true)?;
        Ok(())
    });
    assert!(matches!(change, Err(CommandError::Settings { .. })));
    assert!(matches!(
        state.machine_members(Some(FAVORITES)),
        Err(CommandError::Settings { .. })
    ));
    assert_eq!(fs::read_to_string(&path).expect("read"), "{ corrupted");

    // Fixed by the user: read again on next use.
    fs::write(&path, r#"{ "schemaVersion": 1, "lists": [] }"#).expect("write");
    assert_eq!(entries(&state), []);
}

#[test]
fn unknown_lists_are_reported() {
    let dir = tempfile::tempdir().expect("temp dir");
    let state = state(dir.path());

    assert!(matches!(
        state.machine_members(Some("arcade")),
        Err(CommandError::NotFound { .. })
    ));
    assert_eq!(state.machine_members(None).expect("none"), None);
}

fn import_file(dir: &Path, entries: serde_json::Value) -> std::path::PathBuf {
    let path = dir.join("import ✓.json");
    let document = serde_json::json!({
        "format": "ymdb-lists",
        "version": 1,
        "lists": [{ "id": "favorites", "kind": "system", "entries": entries }]
    });
    fs::write(&path, document.to_string()).expect("write");
    path
}

fn prepare(state: &ListsState, path: &Path) -> ListsImportPreview {
    let parsed = state
        .read(|lists| exchange::read_file(path, lists))
        .expect("read")
        .expect("parse");
    state.prepare_import(parsed).expect("prepare")
}

#[test]
fn imports_after_a_preview_in_merge_or_replace_mode() {
    let dir = tempfile::tempdir().expect("temp dir");
    let state = state(dir.path());
    favorite(&state, &ListEntry::machine("pacman"), true);
    let path = import_file(
        dir.path(),
        serde_json::json!([
            { "kind": "machine", "name": "1942" },
            { "kind": "software", "listName": "nes", "name": "smb" }
        ]),
    );

    let preview = prepare(&state, &path);
    assert_eq!(preview.lists[0].new_entries, 2);
    assert_eq!(preview.lists[0].removed_by_replace, 1);
    // Nothing changes before the import is applied.
    assert_eq!(entries(&state), [ListEntry::machine("pacman")]);

    let outcome = state
        .apply_import(preview.import_id, ImportMode::Merge)
        .expect("merge");
    assert_eq!((outcome[0].added, outcome[0].removed), (2, 0));
    assert_eq!(entries(&state).len(), 3);
    // An import is applied once.
    assert!(matches!(
        state.apply_import(preview.import_id, ImportMode::Merge),
        Err(CommandError::InvalidRequest { .. })
    ));

    let preview = prepare(&state, &path);
    state
        .apply_import(preview.import_id, ImportMode::Replace)
        .expect("replace");
    assert_eq!(
        entries(&state),
        [
            ListEntry::machine("1942"),
            ListEntry::software("nes", "smb")
        ]
    );
    assert_eq!(entries(&self::state(dir.path())).len(), 2);
}

#[test]
fn a_failed_import_changes_nothing() {
    let dir = tempfile::tempdir().expect("temp dir");
    let state = state(dir.path());
    favorite(&state, &ListEntry::machine("pacman"), true);
    let path = import_file(
        dir.path(),
        serde_json::json!([{ "kind": "machine", "name": "1942" }]),
    );
    let preview = prepare(&state, &path);
    fs::create_dir_all(
        dir.path()
            .join("config")
            .join(format!("{LISTS_FILE_NAME}.tmp")),
    )
    .expect("dir");

    let result = state.apply_import(preview.import_id, ImportMode::Replace);

    assert!(matches!(result, Err(CommandError::Settings { .. })));
    assert_eq!(entries(&state), [ListEntry::machine("pacman")]);
    assert_eq!(
        entries(&self::state(dir.path())),
        [ListEntry::machine("pacman")]
    );
}

#[test]
fn export_writes_the_document_to_the_chosen_file() {
    let dir = tempfile::tempdir().expect("temp dir");
    let state = state(dir.path());
    favorite(&state, &ListEntry::software("nes", "smb"), true);
    let document = state
        .read(|lists| exchange::export(lists, &[FAVORITES.to_owned()]))
        .expect("read")
        .expect("export");
    let path = dir.path().join("exports ✓").join("favorites.json");

    write_export(&path, &document).expect("write");

    let other = tempfile::tempdir().expect("temp dir");
    let target = self::state(other.path());
    let preview = prepare(&target, &path);
    target
        .apply_import(preview.import_id, ImportMode::Merge)
        .expect("import");
    assert_eq!(entries(&target), [ListEntry::software("nes", "smb")]);
}

// Searches restricted to Favorites

struct Catalog {
    _dir: TempDir,
    store: CatalogStore,
}

fn catalog(release: &str, machines: &str, software: Option<&str>) -> Catalog {
    let dir = tempfile::tempdir().expect("temp dir");
    let store = CatalogStore::in_dir(dir.path());
    import(&store, release, machines, software);
    Catalog { _dir: dir, store }
}

fn import(store: &CatalogStore, release: &str, machines: &str, software: Option<&str>) {
    let source = || CatalogSource {
        mame_release: release.to_owned(),
        mame_revision: format!("mame{}", release.replace('.', "")),
        executable_path: "/opt/mame/mame".into(),
        executable_size: Some(1),
        executable_modified_at: Some(1),
    };
    let cancel = AtomicBool::new(false);
    import_from_reader(
        machines.as_bytes(),
        store,
        source(),
        &cancel,
        &ImportProgress::default(),
        || Ok(()),
    )
    .expect("machine import");
    if let Some(software) = software {
        import_software_from_reader(
            software.as_bytes(),
            store,
            source(),
            &cancel,
            &ImportProgress::default(),
            || Ok(()),
        )
        .expect("software import");
    }
}

fn favorite_machines(
    catalog: &Catalog,
    state: &ListsState,
    request: MachineSearchRequest,
) -> (u64, Vec<String>) {
    let request = MachineSearchRequest {
        filters: MachineFilters {
            list: Some(FAVORITES.to_owned()),
            ..request.filters
        },
        ..request
    };
    let members = state
        .machine_members(request.filters.list.as_deref())
        .expect("members");
    let response = search(&catalog.store, |connection| {
        search_machines(connection, &request, members.as_ref())
    })
    .expect("search");
    (
        response.total,
        response.items.into_iter().map(|item| item.name).collect(),
    )
}

#[test]
fn favorites_are_shared_by_the_catalogs_of_every_mame_version() {
    let dir = tempfile::tempdir().expect("temp dir");
    let state = state(dir.path());
    let new = catalog("0.280", NEW_MACHINES, None);
    let old = catalog("0.252", OLD_MACHINES, None);
    favorite(&state, &ListEntry::machine("pacman"), true);
    // Only in the newer catalog.
    favorite(&state, &ListEntry::machine("sf2"), true);
    // In no catalog at all.
    favorite(&state, &ListEntry::machine("notamachine"), true);

    let all = MachineSearchRequest::default();
    assert_eq!(
        favorite_machines(&new, &state, all.clone()),
        (2, vec!["pacman".to_owned(), "sf2".to_owned()])
    );
    assert_eq!(
        favorite_machines(&old, &state, all.clone()),
        (1, vec!["pacman".to_owned()])
    );
    // Missing entries are kept: they match again with a catalog that has them.
    assert_eq!(entries(&state).len(), 3);

    // Importing a catalog again does not touch the favorites.
    import(&new.store, "0.281", NEW_MACHINES, None);
    assert_eq!(favorite_machines(&new, &state, all).0, 2);
    assert_eq!(entries(&state).len(), 3);
}

#[test]
fn the_favorites_filter_applies_before_counting_faceting_and_paging() {
    let dir = tempfile::tempdir().expect("temp dir");
    let state = state(dir.path());
    let new = catalog("0.280", NEW_MACHINES, None);
    for name in ["pacman", "mspacman", "puckman", "sf2", "pong"] {
        favorite(&state, &ListEntry::machine(name), true);
    }

    // Second page of two: favorites only, in name order.
    let page = MachineSearchRequest {
        page: 1,
        page_size: Some(2),
        ..MachineSearchRequest::default()
    };
    assert_eq!(
        favorite_machines(&new, &state, page),
        (5, vec!["pong".to_owned(), "puckman".to_owned()])
    );

    // Combined with the text and the other filters (AND).
    let text = MachineSearchRequest {
        text: Some("pac".to_owned()),
        ..MachineSearchRequest::default()
    };
    let (total, names) = favorite_machines(&new, &state, text);
    assert_eq!(total, 2);
    assert!(names.contains(&"pacman".to_owned()) && names.contains(&"mspacman".to_owned()));
    let clones = MachineSearchRequest {
        filters: MachineFilters {
            clone_of: Some("puckman".to_owned()),
            ..MachineFilters::default()
        },
        ..MachineSearchRequest::default()
    };
    assert_eq!(
        favorite_machines(&new, &state, clones),
        (1, vec!["pacman".to_owned()])
    );

    // Facets count favorites only.
    let request = MachineSearchRequest {
        filters: MachineFilters {
            list: Some(FAVORITES.to_owned()),
            ..MachineFilters::default()
        },
        facets: vec![FacetRequest {
            facet: MachineFacet::Year,
            search: None,
            limit: None,
        }],
        ..MachineSearchRequest::default()
    };
    let members = state.machine_members(Some(FAVORITES)).expect("members");
    let response = search(&new.store, |connection| {
        search_machines(connection, &request, members.as_ref())
    })
    .expect("search");
    let counted: u64 = response.facets[0]
        .values
        .iter()
        .map(|value| value.count)
        .sum();
    assert_eq!(counted, 5);

    // An empty list matches nothing.
    for name in ["pacman", "mspacman", "puckman", "sf2", "pong"] {
        favorite(&state, &ListEntry::machine(name), false);
    }
    assert_eq!(
        favorite_machines(&new, &state, MachineSearchRequest::default()),
        (0, vec![])
    );
}

#[test]
fn software_favorites_distinguish_equal_names_of_different_lists() {
    let dir = tempfile::tempdir().expect("temp dir");
    let state = state(dir.path());
    let new = catalog("0.280", NEW_MACHINES, Some(SOFTWARE));
    favorite(&state, &ListEntry::software("nes", "tetris"), true);
    // A machine favorite never restricts software searches, and the other way round.
    favorite(&state, &ListEntry::machine("tetris"), true);

    let request = SoftwareSearchRequest {
        filters: SoftwareFilters {
            list: Some(FAVORITES.to_owned()),
            ..SoftwareFilters::default()
        },
        ..SoftwareSearchRequest::default()
    };
    let members = state.software_members(Some(FAVORITES)).expect("members");
    let response = search(&new.store, |connection| {
        search_software(connection, &request, members.as_ref())
    })
    .expect("search");

    let found: Vec<(String, String)> = response
        .items
        .into_iter()
        .map(|item| (item.list_name, item.name))
        .collect();
    assert_eq!(found, [("nes".to_owned(), "tetris".to_owned())]);
    assert_eq!(
        favorite_machines(&new, &state, MachineSearchRequest::default()).0,
        0
    );
}

#[test]
fn a_list_filter_needs_resolved_members() {
    let new = catalog("0.280", NEW_MACHINES, None);
    let request = MachineSearchRequest {
        filters: MachineFilters {
            list: Some(FAVORITES.to_owned()),
            ..MachineFilters::default()
        },
        ..MachineSearchRequest::default()
    };

    let result = search(&new.store, |connection| {
        search_machines(connection, &request, None)
    });

    assert!(matches!(result, Err(CommandError::InvalidRequest { .. })));
}

#[test]
fn changing_favorites_never_writes_to_a_catalog() {
    let dir = tempfile::tempdir().expect("temp dir");
    let state = state(dir.path());
    let new = catalog("0.280", NEW_MACHINES, Some(SOFTWARE));
    let before = fs::read(new.store.catalog_path()).expect("read");

    favorite(&state, &ListEntry::machine("pacman"), true);
    favorite(&state, &ListEntry::software("nes", "smb"), true);
    favorite(&state, &ListEntry::machine("pacman"), false);
    favorite_machines(&new, &state, MachineSearchRequest::default());

    assert_eq!(fs::read(new.store.catalog_path()).expect("read"), before);
    assert!(!dir.path().join("config").join("catalog.sqlite").exists());
}

#[test]
fn serializes_lists_to_the_frontend_contract() {
    let mut lists = Lists::new();
    lists
        .set_membership(FAVORITES, &ListEntry::software("nes", "smb"), true)
        .expect("add");

    assert_eq!(
        serde_json::to_value(list_view(&lists, FAVORITES).expect("view")).expect("serialize"),
        serde_json::json!({
            "id": "favorites",
            "kind": "system",
            "name": "Favorites",
            "entries": [{ "kind": "software", "listName": "nes", "name": "smb" }]
        })
    );
    let preview = ListsImportPreview {
        import_id: 3,
        lists: vec![exchange::ListImportSummary {
            list_id: "favorites".to_owned(),
            name: "Favorites".to_owned(),
            entries: 4,
            new_entries: 3,
            existing_entries: 1,
            duplicates: 2,
            removed_by_replace: 5,
        }],
        ignored_lists: vec!["Shmups".to_owned()],
    };
    assert_eq!(
        serde_json::to_value(preview).expect("serialize"),
        serde_json::json!({
            "importId": 3,
            "lists": [{
                "listId": "favorites",
                "name": "Favorites",
                "entries": 4,
                "newEntries": 3,
                "existingEntries": 1,
                "duplicates": 2,
                "removedByReplace": 5
            }],
            "ignoredLists": ["Shmups"]
        })
    );
    assert_eq!(
        serde_json::from_value::<ImportMode>(serde_json::json!("replace")).expect("mode"),
        ImportMode::Replace
    );
    assert_eq!(
        serde_json::to_value(CommandError::InvalidFile {
            message: "bad".to_owned()
        })
        .expect("serialize"),
        serde_json::json!({ "kind": "invalidFile", "message": "bad" })
    );
}
