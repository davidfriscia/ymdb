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
        state.machine_members(&[FAVORITES.to_owned()]),
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
        state.machine_members(&["arcade".to_owned()]),
        Err(CommandError::NotFound { .. })
    ));
    assert_eq!(state.machine_members(&[]).expect("none"), []);
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
            user_lists: vec![FAVORITES.to_owned()],
            ..request.filters
        },
        ..request
    };
    let members = state
        .machine_members(&request.filters.user_lists)
        .expect("members");
    let response = search(&catalog.store, |connection| {
        search_machines(connection, &request, &members)
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
            user_lists: vec![FAVORITES.to_owned()],
            ..MachineFilters::default()
        },
        facets: vec![FacetRequest {
            facet: MachineFacet::Year,
            search: None,
            limit: None,
        }],
        ..MachineSearchRequest::default()
    };
    let members = state
        .machine_members(&[FAVORITES.to_owned()])
        .expect("members");
    let response = search(&new.store, |connection| {
        search_machines(connection, &request, &members)
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
            user_lists: vec![FAVORITES.to_owned()],
            ..SoftwareFilters::default()
        },
        ..SoftwareSearchRequest::default()
    };
    let members = state
        .software_members(&[FAVORITES.to_owned()])
        .expect("members");
    let response = search(&new.store, |connection| {
        search_software(connection, &request, &members)
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
            user_lists: vec![FAVORITES.to_owned()],
            ..MachineFilters::default()
        },
        ..MachineSearchRequest::default()
    };

    let result = search(&new.store, |connection| {
        search_machines(connection, &request, &[])
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

// Custom lists

const CATEGORY_FILE: &str = "[FOLDER_SETTINGS]\nRootFolderIcon mame\n;; CATEGORY.ini ;;\n\
[ROOT_FOLDER]\npacman\n[Maze / Collect]\npacman\nmspacman\n[Fighter / Versus]\nsf2\nsf2ce\n\
notamachine\n[Empty]\n";

fn prepared(state: &ListsState) -> FolderFileSummary {
    let file = crate::lists::ini::parse(CATEGORY_FILE.as_bytes()).expect("parse");
    state.prepare_folder_import("category.ini".to_owned(), file)
}

fn summaries(state: &ListsState) -> Vec<(String, usize)> {
    state
        .read(user_list_summaries)
        .expect("read")
        .into_iter()
        .map(|summary| (summary.name, summary.machines))
        .collect()
}

#[test]
fn folder_imports_are_previewed_then_applied_once() {
    let dir = tempfile::tempdir().expect("temp dir");
    let state = state(dir.path());
    let summary = prepared(&state);
    assert_eq!(summary.suggested_name, "Category");
    assert_eq!(
        (summary.root_entries, summary.folders, summary.empty_folders),
        (1, 2, 1)
    );

    let preview = state
        .preview_folder_import(summary.import_id, "Category", Naming::Prefixed)
        .expect("preview");
    assert_eq!(
        (preview.to_create, preview.to_merge, preview.new_entries),
        (3, 0, 6)
    );
    assert_eq!(preview.lists[1].name, "Category / Maze / Collect");
    // The preview changes nothing.
    assert_eq!(summaries(&state), []);

    let outcome = state
        .apply_folder_import(summary.import_id, "Category", Naming::Prefixed, 5)
        .expect("apply");
    assert_eq!((outcome.created, outcome.entries_added), (3, 6));
    assert_eq!(
        summaries(&self::state(dir.path())),
        [
            ("Category".to_owned(), 1),
            ("Category / Maze / Collect".to_owned(), 2),
            ("Category / Fighter / Versus".to_owned(), 3),
        ]
    );
    // An applied import cannot be applied again by mistake.
    assert!(matches!(
        state.apply_folder_import(summary.import_id, "Category", Naming::Prefixed, 6),
        Err(CommandError::InvalidRequest { .. })
    ));
    // Importing the same file again merges and adds nothing.
    let again = prepared(&state);
    let preview = state
        .preview_folder_import(again.import_id, "category", Naming::Prefixed)
        .expect("preview");
    assert_eq!(
        (preview.to_create, preview.to_merge, preview.new_entries),
        (0, 3, 0)
    );
    assert_eq!(preview.existing_entries, 6);
}

#[test]
fn a_folder_import_merges_with_lists_created_after_its_preview() {
    let dir = tempfile::tempdir().expect("temp dir");
    let state = state(dir.path());
    let summary = prepared(&state);
    let preview = state
        .preview_folder_import(summary.import_id, "Games", Naming::Subfolder)
        .expect("preview");
    assert_eq!(preview.to_create, 3);
    // Created elsewhere in the meantime, with another spelling and an entry.
    state
        .update(|lists| {
            lists.create_user_list("mine", "maze / COLLECT", 1)?;
            lists.set_membership("mine", &ListEntry::machine("puckman"), true)?;
            Ok(())
        })
        .expect("create");
    let outcome = state
        .apply_folder_import(summary.import_id, "Games", Naming::Subfolder, 2)
        .expect("apply");
    assert_eq!((outcome.created, outcome.merged), (2, 1));
    let merged = state
        .read(|lists| lists.get("mine").cloned())
        .expect("read")
        .expect("mine");
    assert_eq!(merged.display_name(), "maze / COLLECT");
    assert_eq!(
        merged.machine_names().collect::<Vec<_>>(),
        ["puckman", "pacman", "mspacman"]
    );
}

#[test]
fn a_failed_folder_import_saves_nothing() {
    let dir = tempfile::tempdir().expect("temp dir");
    let state = state(dir.path());
    state
        .update(|lists| Ok(lists.create_user_list("keep", "Keep", 1)?))
        .expect("create");
    let summary = prepared(&state);
    fs::create_dir_all(
        dir.path()
            .join("config")
            .join(format!("{LISTS_FILE_NAME}.tmp")),
    )
    .expect("dir");
    assert!(matches!(
        state.apply_folder_import(summary.import_id, "Category", Naming::Prefixed, 2),
        Err(CommandError::Settings { .. })
    ));
    assert_eq!(summaries(&state), [("Keep".to_owned(), 0)]);
    assert_eq!(
        summaries(&self::state(dir.path())),
        [("Keep".to_owned(), 0)]
    );
}

#[test]
fn concurrent_changes_of_the_same_lists_are_all_kept() {
    let dir = tempfile::tempdir().expect("temp dir");
    let state = std::sync::Arc::new(state(dir.path()));
    state
        .update(|lists| {
            lists.create_user_list("a", "A", 1)?;
            lists.create_user_list("b", "B", 1)?;
            Ok(())
        })
        .expect("create");
    let summary = prepared(&state);
    let mut threads = Vec::new();
    for index in 0..8 {
        let state = std::sync::Arc::clone(&state);
        threads.push(std::thread::spawn(move || {
            let entry = ListEntry::machine(&format!("m{index}"));
            let list = if index % 2 == 0 { "a" } else { "b" };
            state
                .update(|lists| {
                    lists.set_user_memberships(&entry, &[(list.to_owned(), true)], 2)?;
                    Ok(())
                })
                .expect("membership");
        }));
    }
    let importer = {
        let state = std::sync::Arc::clone(&state);
        std::thread::spawn(move || {
            state
                .apply_folder_import(summary.import_id, "Category", Naming::Prefixed, 3)
                .expect("import")
        })
    };
    for thread in threads {
        thread.join().expect("thread");
    }
    importer.join().expect("importer");
    let names = summaries(&self::state(dir.path()));
    assert_eq!(names.len(), 5);
    assert!(names.contains(&("A".to_owned(), 4)));
    assert!(names.contains(&("B".to_owned(), 4)));
}

#[test]
fn search_members_follow_every_change_of_a_list() {
    let dir = tempfile::tempdir().expect("temp dir");
    let state = state(dir.path());
    let new = catalog("0.280", NEW_MACHINES, None);
    state
        .update(|lists| Ok(lists.create_user_list("maze", "Maze", 1)?))
        .expect("create");
    let found = |lists: &[&str]| {
        let request = MachineSearchRequest {
            filters: MachineFilters {
                user_lists: lists.iter().map(|id| (*id).to_owned()).collect(),
                ..MachineFilters::default()
            },
            ..MachineSearchRequest::default()
        };
        let members = state
            .machine_members(&request.filters.user_lists)
            .expect("members");
        let response = search(&new.store, |connection| {
            search_machines(connection, &request, &members)
        })
        .expect("search");
        response
            .items
            .into_iter()
            .map(|item| item.name)
            .collect::<Vec<_>>()
    };
    let set = |name: &str, member: bool| {
        state
            .update(|lists| {
                lists.set_user_memberships(
                    &ListEntry::machine(name),
                    &[("maze".to_owned(), member)],
                    2,
                )?;
                Ok(())
            })
            .expect("membership");
    };
    // An empty list matches nothing (it is not "no filter").
    assert_eq!(found(&["maze"]), Vec::<String>::new());
    for name in ["pacman", "mspacman", "sf2"] {
        set(name, true);
    }
    assert_eq!(found(&["maze"]), ["mspacman", "pacman", "sf2"]);
    // Cached members are rebuilt after every change.
    favorite(&state, &ListEntry::machine("pacman"), true);
    favorite(&state, &ListEntry::machine("sf2"), true);
    assert_eq!(found(&["favorites", "maze"]), ["pacman", "sf2"]);
    set("sf2", false);
    assert_eq!(found(&["favorites", "maze"]), ["pacman"]);
    // A deleted list is reported, never treated as "all items".
    state
        .update(|lists| Ok(lists.delete_user_list("maze")?))
        .expect("delete");
    assert!(matches!(
        state.machine_members(&["maze".to_owned()]),
        Err(CommandError::NotFound { .. })
    ));
}

#[test]
fn list_entries_are_paged_with_descriptions_and_missing_machines() {
    let new = catalog("0.280", NEW_MACHINES, None);
    let old = catalog("0.252", OLD_MACHINES, None);
    let mut list = crate::lists::model::List::user("maze", "Maze", 1);
    for name in ["sf2", "pacman", "notamachine", "mspacman"] {
        list.entries.push(ListEntry::machine(name));
    }
    list.entries.push(ListEntry::software("nes", "smb"));
    let members = ListMembers::machines(list.machine_names());
    let page = |store: &CatalogStore, search: Option<&str>, page: u32, size: u32| {
        store
            .with_reader(|connection| {
                Ok(members::machine_page(
                    connection, &members, search, page, size,
                ))
            })
            .expect("store")
            .expect("catalog")
            .expect("page")
    };
    let all = page(&new.store, None, 0, 50);
    assert_eq!(all.matched, 4);
    let rows: Vec<(&str, bool)> = all
        .items
        .iter()
        .map(|item| (item.name.as_str(), item.in_catalog == Some(true)))
        .collect();
    assert_eq!(
        rows,
        [
            ("mspacman", true),
            ("notamachine", false),
            ("pacman", true),
            ("sf2", true)
        ]
    );
    assert!(all.items[1].description.is_none());
    // Search by short name or description, case-insensitively; `%` is text.
    let by_description = page(&new.store, Some("PAC-MAN"), 0, 50);
    assert!(by_description.matched > 0);
    assert!(
        by_description
            .items
            .iter()
            .all(|item| item.name.contains("pacman"))
    );
    assert_eq!(page(&new.store, Some("nota"), 0, 50).matched, 1);
    assert_eq!(page(&new.store, Some("%"), 0, 50).matched, 0);
    // Pages.
    let second = page(&new.store, None, 1, 3);
    assert_eq!((second.matched, second.items.len()), (4, 1));
    // Another MAME version: sf2 is missing there but stays listed.
    let old_rows = page(&old.store, None, 0, 50);
    assert_eq!(old_rows.matched, 4);
    assert_eq!(
        old_rows
            .items
            .iter()
            .find(|item| item.name == "sf2")
            .expect("sf2")
            .in_catalog,
        Some(false)
    );
    // Without a catalog: names only.
    let plain = machine_page_without_catalog(&list, Some("PAC"), 0, 50).expect("plain");
    assert_eq!(plain.matched, 2);
    assert_eq!(plain.items[0].in_catalog, None);
    assert!(machine_page_without_catalog(&list, None, 0, 0).is_err());
    assert!(machine_page_without_catalog(&list, None, 0, 201).is_err());
}

#[test]
fn serializes_custom_lists_to_the_frontend_contract() {
    let mut lists = Lists::new();
    lists.create_user_list("abc", "Maze", 7).expect("create");
    lists
        .set_user_memberships(
            &ListEntry::machine("pacman"),
            &[("abc".to_owned(), true)],
            9,
        )
        .expect("add");
    assert_eq!(
        serde_json::to_value(user_list_summaries(&lists)).expect("serialize"),
        serde_json::json!([{
            "id": "abc", "name": "Maze", "machines": 1, "entries": 1,
            "createdAt": 7, "updatedAt": 9
        }])
    );
    let state = state(tempfile::tempdir().expect("temp dir").path());
    let summary = prepared(&state);
    let preview = state
        .preview_folder_import(summary.import_id, "Cat", Naming::Subfolder)
        .expect("preview");
    let json = serde_json::to_value(&preview).expect("serialize");
    assert_eq!(json["naming"], "subfolder");
    assert_eq!(
        json["lists"][0],
        serde_json::json!({ "name": "Cat", "action": "create", "entries": 1, "newEntries": 1 })
    );
    assert_eq!(json["emptyFolders"], 1);
    let file = crate::lists::ini::parse(b"[ROOT_FOLDER]\npacman\nName=x\n").expect("parse");
    assert_eq!(
        serde_json::to_value(&file.warnings).expect("serialize"),
        serde_json::json!([{ "kind": "invalidLine", "count": 1, "examples": ["line 3: Name=x"] }])
    );
    let change: MembershipChange =
        serde_json::from_value(serde_json::json!({ "listId": "abc", "member": true }))
            .expect("change");
    assert_eq!(change.list_id, "abc");
    let page = ListEntriesPage {
        list: user_list_summaries(&lists).remove(0),
        installation_id: None,
        items: vec![ListMachine {
            name: "pacman".to_owned(),
            description: None,
            in_catalog: None,
        }],
        matched: 1,
        page: 0,
        page_size: 50,
    };
    assert_eq!(
        serde_json::to_value(page).expect("serialize")["items"][0],
        serde_json::json!({ "name": "pacman", "description": null, "inCatalog": null })
    );
    assert_eq!(suggested_name("bootlegs.ini"), "Bootlegs");
    assert_eq!(suggested_name("Non Bootlegs.ini"), "Non Bootlegs");
}
