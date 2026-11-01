//! Tests of the lists model, `lists.json` and the export format.

use std::fs;

use serde_json::json;

use super::exchange::{self, FORMAT, FORMAT_VERSION, MAX_FILE_SIZE};
use super::model::{
    FAVORITES, ImportMode, List, ListEntry, ListError, ListKind, Lists, MAX_ENTRIES,
};
use super::store::{LISTS_FILE_NAME, ListsStore, ListsStoreError};

fn favorites(lists: &Lists) -> &[ListEntry] {
    &lists.get(FAVORITES).expect("favorites").entries
}

fn with_favorites(entries: &[ListEntry]) -> Lists {
    let mut lists = Lists::new();
    for entry in entries {
        lists.set_membership(FAVORITES, entry, true).expect("add");
    }
    lists
}

// Model

#[test]
fn favorites_is_an_unnamed_system_list_available_by_default() {
    let lists = Lists::new();

    let list = lists.get(FAVORITES).expect("favorites");
    assert_eq!(list.kind, ListKind::System);
    assert_eq!(list.name, None);
    assert_eq!(list.display_name(), "Favorites");
    assert!(list.entries.is_empty());
    assert_eq!(lists.validate(), Ok(()));
}

#[test]
fn adds_and_removes_machines_and_software() {
    let mut lists = Lists::new();
    let machine = ListEntry::machine("1942");
    let software = ListEntry::software("nes", "smb");

    assert_eq!(lists.set_membership(FAVORITES, &machine, true), Ok(true));
    assert_eq!(lists.set_membership(FAVORITES, &software, true), Ok(true));
    assert_eq!(favorites(&lists), [machine.clone(), software.clone()]);

    assert_eq!(lists.set_membership(FAVORITES, &machine, false), Ok(true));
    assert_eq!(favorites(&lists), std::slice::from_ref(&software));
    assert_eq!(lists.set_membership(FAVORITES, &software, false), Ok(true));
    assert!(favorites(&lists).is_empty());
}

#[test]
fn membership_changes_are_idempotent_and_never_duplicate() {
    let mut lists = Lists::new();
    let machine = ListEntry::machine("pacman");

    assert_eq!(lists.set_membership(FAVORITES, &machine, true), Ok(true));
    assert_eq!(lists.set_membership(FAVORITES, &machine, true), Ok(false));
    assert_eq!(favorites(&lists).len(), 1);
    // Toggling twice restores the initial state.
    assert_eq!(lists.set_membership(FAVORITES, &machine, false), Ok(true));
    assert_eq!(lists.set_membership(FAVORITES, &machine, false), Ok(false));
    assert!(favorites(&lists).is_empty());
}

#[test]
fn software_is_identified_by_list_and_name() {
    let lists = with_favorites(&[ListEntry::software("nes", "tetris")]);
    let list = lists.get(FAVORITES).expect("favorites");

    assert!(list.contains(&ListEntry::software("nes", "tetris")));
    assert!(!list.contains(&ListEntry::software("c64_cart", "tetris")));
    // A machine and a software with the same short name are different entries.
    assert!(!list.contains(&ListEntry::machine("tetris")));
    assert_eq!(list.machine_names().count(), 0);
    assert_eq!(
        list.software_keys().collect::<Vec<_>>(),
        [("nes", "tetris")]
    );
}

#[test]
fn rejects_invalid_names_and_unknown_lists() {
    let mut lists = Lists::new();
    for entry in [
        ListEntry::machine(""),
        ListEntry::machine("pac man"),
        ListEntry::machine("../roms"),
        ListEntry::machine(&"a".repeat(65)),
        ListEntry::software("nes", "a:b"),
        ListEntry::software("", "smb"),
    ] {
        assert!(
            matches!(
                lists.set_membership(FAVORITES, &entry, true),
                Err(ListError::Invalid(_))
            ),
            "{entry:?}"
        );
    }
    assert_eq!(
        lists.set_membership("arcade", &ListEntry::machine("pacman"), true),
        Err(ListError::UnknownList("arcade".to_owned()))
    );
    assert!(favorites(&lists).is_empty());
}

#[test]
fn limits_the_number_of_entries() {
    let mut lists = Lists::new();
    lists.lists[0].entries = (0..MAX_ENTRIES)
        .map(|index| ListEntry::machine(&format!("m{index}")))
        .collect();

    assert!(matches!(
        lists.set_membership(FAVORITES, &ListEntry::machine("one_more"), true),
        Err(ListError::Invalid(_))
    ));
}

#[test]
fn validates_list_identities() {
    let mut renamed = Lists::new();
    renamed.lists[0].name = Some("Best".to_owned());
    assert!(renamed.validate().is_err());

    let mut unknown_system = Lists::new();
    unknown_system.lists.push(List::system("recent"));
    assert!(unknown_system.validate().is_err());

    let mut twice = Lists::new();
    twice.lists.push(List::system(FAVORITES));
    assert!(twice.validate().is_err());

    let mut duplicate_entry = with_favorites(&[ListEntry::machine("pacman")]);
    duplicate_entry.lists[0]
        .entries
        .push(ListEntry::machine("pacman"));
    assert!(duplicate_entry.validate().is_err());

    // User lists share the model: an id and a name chosen by the user.
    let mut user = Lists::new();
    user.lists.push(List {
        id: "shmups-1".to_owned(),
        kind: ListKind::User,
        name: Some("Shoot 'em ups".to_owned()),
        entries: vec![ListEntry::machine("1942")],
        created_at: None,
        updated_at: None,
    });
    assert_eq!(user.validate(), Ok(()));
    assert_eq!(user.lists[1].display_name(), "Shoot 'em ups");
    user.lists[1].name = None;
    assert!(user.validate().is_err());
}

#[test]
fn merge_import_adds_new_entries_only() {
    let mut lists = with_favorites(&[ListEntry::machine("pacman"), ListEntry::machine("1942")]);

    let outcome = lists
        .import(
            FAVORITES,
            &[
                ListEntry::machine("1942"),
                ListEntry::software("nes", "smb"),
            ],
            ImportMode::Merge,
        )
        .expect("import");

    assert_eq!((outcome.added, outcome.removed, outcome.entries), (1, 0, 3));
    assert_eq!(
        favorites(&lists),
        [
            ListEntry::machine("pacman"),
            ListEntry::machine("1942"),
            ListEntry::software("nes", "smb"),
        ]
    );
}

#[test]
fn replace_import_keeps_exactly_the_imported_entries() {
    let mut lists = with_favorites(&[ListEntry::machine("pacman"), ListEntry::machine("1942")]);

    let outcome = lists
        .import(
            FAVORITES,
            &[
                ListEntry::software("nes", "smb"),
                ListEntry::machine("1942"),
            ],
            ImportMode::Replace,
        )
        .expect("import");

    assert_eq!((outcome.added, outcome.removed, outcome.entries), (1, 1, 2));
    assert_eq!(
        favorites(&lists),
        [
            ListEntry::software("nes", "smb"),
            ListEntry::machine("1942")
        ]
    );
}

#[test]
fn rejected_import_changes_nothing() {
    let mut lists = with_favorites(&[ListEntry::machine("pacman")]);
    let before = lists.clone();

    let duplicates = [ListEntry::machine("1942"), ListEntry::machine("1942")];
    assert!(
        lists
            .import(FAVORITES, &duplicates, ImportMode::Replace)
            .is_err()
    );
    let invalid = [ListEntry::machine("1942"), ListEntry::machine("bad name")];
    assert!(
        lists
            .import(FAVORITES, &invalid, ImportMode::Merge)
            .is_err()
    );

    assert_eq!(lists, before);
}

// Store

#[test]
fn a_missing_file_gives_empty_system_lists_without_writing() {
    let dir = tempfile::tempdir().expect("temp dir");
    let store = ListsStore::in_dir(&dir.path().join("config"));

    assert_eq!(store.load().expect("load"), Lists::new());
    assert!(!dir.path().join("config").exists());
}

#[test]
fn saves_and_loads_entries() {
    let dir = tempfile::tempdir().expect("temp dir");
    let store = ListsStore::in_dir(dir.path());
    let lists = with_favorites(&[
        ListEntry::machine("1942"),
        ListEntry::software("nes", "smb"),
    ]);

    store.save(&lists).expect("save");

    assert_eq!(store.load().expect("load"), lists);
    let saved: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.path().join(LISTS_FILE_NAME)).expect("read"))
            .expect("json");
    assert_eq!(
        saved,
        json!({
            "schemaVersion": 1,
            "lists": [{
                "id": "favorites",
                "kind": "system",
                "entries": [
                    { "kind": "machine", "name": "1942" },
                    { "kind": "software", "listName": "nes", "name": "smb" }
                ]
            }]
        })
    );
}

#[test]
fn adds_missing_system_lists_when_loading() {
    let dir = tempfile::tempdir().expect("temp dir");
    fs::write(
        dir.path().join(LISTS_FILE_NAME),
        r#"{ "schemaVersion": 1, "lists": [] }"#,
    )
    .expect("write");

    let lists = ListsStore::in_dir(dir.path()).load().expect("load");

    assert!(lists.get(FAVORITES).is_some());
}

#[test]
fn reports_invalid_files_and_never_replaces_them() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join(LISTS_FILE_NAME);
    let store = ListsStore::in_dir(dir.path());
    for content in [
        "{ not json",
        r#"{ "schemaVersion": 2, "lists": [] }"#,
        r#"{ "schemaVersion": 1, "lists": [], "extra": true }"#,
        r#"{ "schemaVersion": 1, "lists": [{ "id": "favorites", "kind": "system",
             "entries": [{ "kind": "machine", "name": "a" }, { "kind": "machine", "name": "a" }] }] }"#,
        r#"{ "schemaVersion": 1, "lists": [{ "id": "favorites", "kind": "system",
             "entries": [{ "kind": "game", "name": "a" }] }] }"#,
    ] {
        fs::write(&path, content).expect("write");

        assert!(
            matches!(store.load(), Err(ListsStoreError::Invalid { .. })),
            "{content}"
        );
        assert_eq!(fs::read_to_string(&path).expect("read"), content);
    }
}

#[test]
fn reports_write_failures() {
    let dir = tempfile::tempdir().expect("temp dir");
    // The configuration "directory" is a file: nothing can be written in it.
    let blocked = dir.path().join("config");
    fs::write(&blocked, "").expect("write");

    let result = ListsStore::in_dir(&blocked).save(&Lists::new());

    assert!(matches!(result, Err(ListsStoreError::Write { .. })));
}

// Export and import

#[test]
fn exports_a_portable_versioned_document() {
    let lists = with_favorites(&[
        ListEntry::machine("1942"),
        ListEntry::software("nes", "smb"),
    ]);

    let document = exchange::export(&lists, &[FAVORITES.to_owned()]).expect("export");
    let value: serde_json::Value =
        serde_json::from_slice(&exchange::to_json(&document)).expect("json");

    assert_eq!(
        value,
        json!({
            "format": "ymdb-lists",
            "version": 1,
            "lists": [{
                "id": "favorites",
                "kind": "system",
                "entries": [
                    { "kind": "machine", "name": "1942" },
                    { "kind": "software", "listName": "nes", "name": "smb" }
                ]
            }]
        })
    );
    assert!(matches!(
        exchange::export(&lists, &["missing".to_owned()]),
        Err(ListError::UnknownList(_))
    ));
}

#[test]
fn an_export_imports_back_into_another_profile() {
    let source = with_favorites(&[
        ListEntry::machine("1942"),
        ListEntry::software("nes", "smb"),
    ]);
    let content =
        exchange::to_json(&exchange::export(&source, &[FAVORITES.to_owned()]).expect("export"));
    let mut target = Lists::new();

    let parsed = exchange::parse(&content, &target).expect("parse");
    target
        .import(FAVORITES, &parsed.lists[0].entries, ImportMode::Merge)
        .expect("import");

    assert_eq!(target, source);
}

fn document(entries: serde_json::Value) -> Vec<u8> {
    json!({
        "format": FORMAT,
        "version": FORMAT_VERSION,
        "lists": [{ "id": "favorites", "kind": "system", "entries": entries }]
    })
    .to_string()
    .into_bytes()
}

#[test]
fn previews_new_existing_duplicate_and_removed_entries() {
    let lists = with_favorites(&[ListEntry::machine("pacman"), ListEntry::machine("galaga")]);
    let content = document(json!([
        { "kind": "machine", "name": "pacman" },
        { "kind": "machine", "name": "1942" },
        { "kind": "machine", "name": "1942" },
        // Not in any catalog: still a valid reference.
        { "kind": "software", "listName": "nes", "name": "notinanycatalog" }
    ]));

    let parsed = exchange::parse(&content, &lists).expect("parse");
    let summary = exchange::summarize(&lists, &parsed).expect("summary");

    assert_eq!(parsed.lists[0].entries.len(), 3);
    assert_eq!(summary.len(), 1);
    let summary = &summary[0];
    assert_eq!(summary.name, "Favorites");
    assert_eq!(
        (
            summary.entries,
            summary.new_entries,
            summary.existing_entries,
            summary.duplicates,
            summary.removed_by_replace
        ),
        (3, 2, 1, 1, 1)
    );
}

#[test]
fn rejects_files_that_are_not_supported_exports() {
    let lists = Lists::new();
    let cases: [(&str, Vec<u8>); 7] = [
        ("not valid JSON", b"{ nope".to_vec()),
        ("not a YMDB lists export", br#"{ "lists": [] }"#.to_vec()),
        (
            "not a YMDB lists export",
            json!({ "format": "other", "version": 1, "lists": [] })
                .to_string()
                .into_bytes(),
        ),
        (
            "version 2",
            json!({ "format": FORMAT, "version": 2, "lists": [] })
                .to_string()
                .into_bytes(),
        ),
        (
            "no valid version",
            json!({ "format": FORMAT, "version": "1", "lists": [] })
                .to_string()
                .into_bytes(),
        ),
        (
            "invalid structure",
            json!({ "format": FORMAT, "version": 1, "lists": [{ "id": "favorites" }] })
                .to_string()
                .into_bytes(),
        ),
        (
            "not a valid MAME name",
            document(json!([{ "kind": "machine", "name": "../x" }])),
        ),
    ];
    for (expected, content) in cases {
        let error = exchange::parse(&content, &lists).expect_err(expected);
        assert!(error.contains(expected), "{error}");
    }
    let missing_field = document(json!([{ "kind": "software", "name": "smb" }]));
    assert!(exchange::parse(&missing_field, &lists).is_err());
    let unknown_field = document(json!([{ "kind": "machine", "name": "a", "path": "C:/x" }]));
    assert!(exchange::parse(&unknown_field, &lists).is_err());
}

#[test]
fn ignores_lists_this_version_cannot_import() {
    let lists = Lists::new();
    let content = json!({
        "format": FORMAT,
        "version": 1,
        "lists": [
            { "id": "shmups", "kind": "user", "name": "Shoot 'em ups", "entries": [] },
            { "id": "favorites", "kind": "system", "entries": [{ "kind": "machine", "name": "1942" }] }
        ]
    })
    .to_string();

    let parsed = exchange::parse(content.as_bytes(), &lists).expect("parse");

    assert_eq!(parsed.ignored_lists, ["Shoot 'em ups"]);
    assert_eq!(parsed.lists.len(), 1);

    let only_user = json!({
        "format": FORMAT,
        "version": 1,
        "lists": [{ "id": "shmups", "kind": "user", "name": "Shoot 'em ups", "entries": [] }]
    })
    .to_string();
    assert!(exchange::parse(only_user.as_bytes(), &lists).is_err());
}

#[test]
fn refuses_files_that_are_too_large() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("huge.json");
    let file = fs::File::create(&path).expect("create");
    file.set_len(MAX_FILE_SIZE + 1).expect("size");

    let error = exchange::read_file(&path, &Lists::new()).expect_err("too large");

    assert!(error.contains("larger than"), "{error}");
}
