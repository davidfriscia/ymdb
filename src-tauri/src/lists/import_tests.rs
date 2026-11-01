//! Folder file parsing, list naming, merging and custom list rules.

use super::folder_import::{self, Naming, PlannedAction, SEPARATOR};
use super::ini::{self, IniError, IniWarningKind};
use super::model::{FAVORITES, ListEntry, ListError, Lists, MAX_LIST_NAME_LENGTH, MAX_USER_LISTS};

/// Shaped like `bootlegs.ini` of the pS support files.
const FLAT: &str = "[FOLDER_SETTINGS]\r\nRootFolderIcon mame\r\nSubFolderIcon folder\r\n\r\n\
;; BOOTLEGS.ini 0.288 / 09-Jun-26 / MAME 0.288 ;;\r\n;; List of 'Bootleg' machines ;;\r\n\r\n\
[ROOT_FOLDER]\r\n119\r\n1942abl\r\n1942iti\r\n\r\n";

/// Shaped like `category.ini`: an empty root and subfolders named with `/`.
const FOLDERS: &str = "[FOLDER_SETTINGS]\nRootFolderIcon mame\nSubFolderIcon folder\n\n\
;; CATEGORY.ini 0.288 / 09-Jun-26 / MAME 0.288 ;;\n\n[ROOT_FOLDER]\n\n\
[Arcade / Fortune Teller]\nhkuranai\n\n[Arcade / Pinball]\naliencr\nbrkball\n\n\
[Ball & Paddle / Breakout]\narkanoid\n\n[Empty / Folder]\n\n";

fn parse(text: &str) -> ini::FolderFile {
    ini::parse(text.as_bytes()).expect("parse")
}

fn names(list: &[String]) -> Vec<&str> {
    list.iter().map(String::as_str).collect()
}

fn warning(file: &ini::FolderFile, kind: IniWarningKind) -> usize {
    file.warnings
        .iter()
        .find(|warning| warning.kind == kind)
        .map_or(0, |warning| warning.count)
}

fn machines(lists: &Lists, name: &str) -> Vec<String> {
    lists
        .user_list_named(name)
        .unwrap_or_else(|| panic!("no list {name}"))
        .machine_names()
        .map(str::to_owned)
        .collect()
}

fn user_names(lists: &Lists) -> Vec<&str> {
    lists.user_lists().map(|list| list.display_name()).collect()
}

fn counter() -> impl FnMut() -> String {
    let mut next = 0;
    move || {
        next += 1;
        format!("list-{next}")
    }
}

// Parsing

#[test]
fn reads_a_flat_file_and_ignores_frontend_settings_and_comments() {
    let file = parse(FLAT);
    assert_eq!(names(&file.root), ["119", "1942abl", "1942iti"]);
    assert!(file.folders.is_empty());
    assert!(!file.has_subfolders());
    assert_eq!(file.empty_folders, 0);
    // `RootFolderIcon mame` is a setting, never a machine.
    assert!(file.warnings.is_empty(), "{:?}", file.warnings);
}

#[test]
fn reads_subfolders_named_as_written_and_skips_empty_ones() {
    let file = parse(FOLDERS);
    assert!(file.root.is_empty());
    let folders: Vec<(&str, Vec<&str>)> = file
        .folders
        .iter()
        .map(|folder| (folder.name.as_str(), names(&folder.machines)))
        .collect();
    assert_eq!(
        folders,
        [
            ("Arcade / Fortune Teller", vec!["hkuranai"]),
            ("Arcade / Pinball", vec!["aliencr", "brkball"]),
            ("Ball & Paddle / Breakout", vec!["arkanoid"]),
        ]
    );
    assert_eq!(file.empty_folders, 1);
}

#[test]
fn any_section_name_is_a_subfolder() {
    let file = parse("[ROOT_FOLDER]\npacman\n[S1]\ngalaga\n[s2]\n[Shooter * Mature *]\n1942\n");
    let folders: Vec<&str> = file.folders.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(folders, ["S1", "Shooter * Mature *"]);
    assert_eq!(file.empty_folders, 1);
}

#[test]
fn handles_bom_line_endings_and_spaces() {
    for text in [
        "\u{feff}[ROOT_FOLDER]\npacman\n  galaga  \n",
        "[ROOT_FOLDER]\r\npacman\r\n\tgalaga\r\n",
        "[ROOT_FOLDER]\rpacman\rgalaga\r",
        "  [ ROOT_FOLDER ]  \n\n\npacman\ngalaga",
    ] {
        let file = parse(text);
        assert_eq!(names(&file.root), ["pacman", "galaga"], "{text:?}");
        assert!(file.warnings.is_empty(), "{text:?}: {:?}", file.warnings);
    }
}

#[test]
fn duplicates_are_imported_once_and_duplicate_sections_combined() {
    let file = parse(
        "[ROOT_FOLDER]\npacman\npacman\n[Maze]\npacman\nmspacman\n[Shooter]\n1942\n\
         [Maze]\nmspacman\npuckman\n[ROOT_FOLDER]\ngalaga\n",
    );
    assert_eq!(names(&file.root), ["pacman", "galaga"]);
    let maze = &file.folders[0];
    assert_eq!(maze.name, "Maze");
    // A machine may be in the root and in a folder: they are different lists.
    assert_eq!(names(&maze.machines), ["pacman", "mspacman", "puckman"]);
    assert_eq!(file.folders.len(), 2);
    assert_eq!(file.duplicates, 2);
    assert_eq!(warning(&file, IniWarningKind::DuplicateEntry), 2);
    assert_eq!(warning(&file, IniWarningKind::DuplicateFolder), 2);
}

#[test]
fn lines_that_are_not_short_names_are_reported_never_imported() {
    let file = parse(
        "RootFolderIcon mame\n[ROOT_FOLDER]\npacman\nName=Bootlegs\nsome text\nPacMan\n\
         ../roms\n[]\nhidden\n[Maze]\nmspacman\n",
    );
    assert_eq!(names(&file.root), ["pacman"]);
    assert_eq!(names(&file.folders[0].machines), ["mspacman"]);
    assert_eq!(warning(&file, IniWarningKind::OutsideSection), 1);
    assert_eq!(warning(&file, IniWarningKind::InvalidLine), 4);
    assert_eq!(warning(&file, IniWarningKind::UnnamedSection), 1);
    let invalid = file
        .warnings
        .iter()
        .find(|warning| warning.kind == IniWarningKind::InvalidLine)
        .expect("warning");
    assert_eq!(invalid.examples[0], "line 4: Name=Bootlegs");
}

#[test]
fn a_missing_root_folder_is_reported() {
    let file = parse("[Maze]\npacman\n");
    assert!(file.root.is_empty());
    assert_eq!(warning(&file, IniWarningKind::MissingRootFolder), 1);
}

#[test]
fn windows_1252_files_are_read_with_a_warning() {
    let mut content = b"[ROOT_FOLDER]\npacman\n[Caf\xe9]\ngalaga\n".to_vec();
    content.extend_from_slice(b"[Ma\x96ze]\nmspacman\n");
    let file = ini::parse(&content).expect("parse");
    assert_eq!(file.folders[0].name, "Café");
    assert_eq!(file.folders[1].name, "Ma–ze");
    assert_eq!(warning(&file, IniWarningKind::NotUtf8), 1);
}

#[test]
fn refuses_empty_invalid_and_unsupported_files() {
    let empty = |text: &[u8]| matches!(ini::parse(text), Err(IniError::Empty));
    assert!(empty(b""));
    assert!(empty(b"\xEF\xBB\xBF  \r\n"));
    assert!(empty(
        b"[FOLDER_SETTINGS]\nRootFolderIcon mame\n[ROOT_FOLDER]\n"
    ));
    assert!(empty(b"[ROOT_FOLDER]\n[Empty]\n"));
    assert!(matches!(
        ini::parse(b"pacman\ngalaga\n"),
        Err(IniError::NotAFolderFile)
    ));
    assert!(matches!(
        ini::parse(b"\xFF\xFE[\0R\0"),
        Err(IniError::UnsupportedEncoding(_))
    ));
    assert!(matches!(
        ini::parse(b"PK\x03\x04\0\0binary"),
        Err(IniError::UnsupportedEncoding(_))
    ));
}

#[test]
fn reads_large_files() {
    let mut text = String::from("[ROOT_FOLDER]\n");
    for index in 0..60_000 {
        text.push_str(&format!("m{index}\n"));
    }
    for folder in 0..700 {
        text.push_str(&format!("[Folder {folder}]\n"));
        for index in 0..50 {
            text.push_str(&format!("m{}\n", folder * 50 + index));
        }
    }
    let file = parse(&text);
    assert_eq!(file.root.len(), 60_000);
    assert_eq!(file.folders.len(), 700);

    let mut lists = Lists::new();
    let outcome = folder_import::apply(&mut lists, &file, "Big", Naming::Prefixed, 1, counter())
        .expect("apply");
    assert_eq!(outcome.created, 701);
    assert_eq!(outcome.entries_added, 60_000 + 700 * 50);
    assert_eq!(lists.validate(), Ok(()));
}

#[test]
fn reads_files_from_disk_and_refuses_large_ones() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("bootlegs ✓.ini");
    std::fs::write(&path, FLAT).expect("write");
    assert_eq!(ini::read_file(&path).expect("read").root.len(), 3);
    assert!(matches!(
        ini::read_file(&dir.path().join("missing.ini")),
        Err(IniError::Io(_))
    ));
}

// Naming and merging

#[test]
fn a_flat_file_creates_one_list_named_by_the_user() {
    let file = parse(FLAT);
    let lists = Lists::new();
    for naming in [Naming::Subfolder, Naming::Prefixed] {
        let plan = folder_import::plan(&lists, &file, " Bootlegs ", naming).expect("plan");
        assert_eq!(plan.lists.len(), 1);
        assert_eq!(plan.lists[0].name, "Bootlegs");
        assert_eq!(plan.lists[0].action(), PlannedAction::Create);
        assert_eq!(plan.new_entries(), 3);
    }
}

#[test]
fn subfolders_are_named_by_subfolder_or_prefixed_with_the_import_name() {
    let file = parse(FOLDERS);
    let mut by_subfolder = Lists::new();
    folder_import::apply(
        &mut by_subfolder,
        &file,
        "Category",
        Naming::Subfolder,
        1,
        counter(),
    )
    .expect("apply");
    assert_eq!(
        user_names(&by_subfolder),
        [
            "Arcade / Fortune Teller",
            "Arcade / Pinball",
            "Ball & Paddle / Breakout"
        ]
    );

    let mut prefixed = Lists::new();
    folder_import::apply(
        &mut prefixed,
        &file,
        "Category",
        Naming::Prefixed,
        1,
        counter(),
    )
    .expect("apply");
    assert_eq!(
        user_names(&prefixed),
        [
            "Category / Arcade / Fortune Teller",
            "Category / Arcade / Pinball",
            "Category / Ball & Paddle / Breakout"
        ]
    );
    assert_eq!(SEPARATOR, " / ");
    assert_eq!(
        machines(&prefixed, "Category / Arcade / Pinball"),
        ["aliencr", "brkball"]
    );
}

#[test]
fn the_root_list_is_separate_from_the_subfolder_lists() {
    let file = parse("[ROOT_FOLDER]\npacman\n[Maze]\nmspacman\n[Shooter]\n1942\n");
    let mut lists = Lists::new();
    folder_import::apply(&mut lists, &file, "Games", Naming::Subfolder, 1, counter())
        .expect("apply");
    assert_eq!(user_names(&lists), ["Games", "Maze", "Shooter"]);
    assert_eq!(machines(&lists, "Games"), ["pacman"]);
    assert_eq!(machines(&lists, "Maze"), ["mspacman"]);
}

#[test]
fn conflicting_names_merge_and_keep_existing_entries() {
    let file = parse("[ROOT_FOLDER]\npacman\ngalaga\n[Maze]\nmspacman\npacman\n");
    let mut lists = Lists::new();
    lists.create_user_list("mine", "games", 1).expect("create");
    lists
        .set_user_memberships(&ListEntry::machine("1942"), &[("mine".to_owned(), true)], 1)
        .expect("add");
    lists
        .set_user_memberships(
            &ListEntry::machine("pacman"),
            &[("mine".to_owned(), true)],
            1,
        )
        .expect("add");

    let plan = folder_import::plan(&lists, &file, "Games", Naming::Subfolder).expect("plan");
    assert_eq!(plan.to_merge(), 1);
    assert_eq!(plan.to_create(), 1);
    // The existing spelling wins; only galaga is new.
    assert_eq!(plan.lists[0].name, "games");
    assert_eq!(plan.lists[0].new_entries, 1);
    assert_eq!(plan.existing_entries(), 1);

    let outcome = folder_import::apply(&mut lists, &file, "Games", Naming::Subfolder, 2, counter())
        .expect("apply");
    assert_eq!(
        (outcome.created, outcome.merged, outcome.entries_added),
        (1, 1, 3)
    );
    assert_eq!(machines(&lists, "games"), ["1942", "pacman", "galaga"]);
    let mine = lists.get("mine").expect("mine");
    assert_eq!(mine.created_at, Some(1));
    assert_eq!(mine.updated_at, Some(2));
}

#[test]
fn importing_twice_changes_nothing_the_second_time() {
    let file = parse(FOLDERS);
    let mut lists = Lists::new();
    folder_import::apply(
        &mut lists,
        &file,
        "Category",
        Naming::Prefixed,
        1,
        counter(),
    )
    .expect("first");
    let first = lists.clone();
    let plan = folder_import::plan(&lists, &file, "category", Naming::Prefixed).expect("plan");
    assert_eq!(
        (plan.to_create(), plan.to_merge(), plan.new_entries()),
        (0, 3, 0)
    );
    let outcome = folder_import::apply(
        &mut lists,
        &file,
        "category",
        Naming::Prefixed,
        9,
        counter(),
    )
    .expect("second");
    assert_eq!((outcome.created, outcome.entries_added), (0, 0));
    // Nothing changed, not even the dates.
    assert_eq!(lists, first);
}

#[test]
fn lists_of_the_file_with_the_same_name_are_combined() {
    // The root is named like a subfolder, ignoring case.
    let file = parse("[ROOT_FOLDER]\npacman\n[maze]\nmspacman\npacman\n[Shooter]\n1942\n");
    let plan = folder_import::plan(&Lists::new(), &file, "Maze", Naming::Subfolder).expect("plan");
    assert_eq!(plan.lists.len(), 2);
    assert_eq!(plan.lists[0].name, "Maze");
    assert_eq!(names(&plan.lists[0].machines), ["pacman", "mspacman"]);
}

#[test]
fn an_invalid_name_refuses_the_whole_import() {
    let file = parse(FOLDERS);
    let mut lists = Lists::new();
    assert!(matches!(
        folder_import::plan(&lists, &file, "   ", Naming::Subfolder),
        Err(ListError::Invalid(_))
    ));
    let long = "x".repeat(MAX_LIST_NAME_LENGTH - 5);
    let before = lists.clone();
    // The prefixed names are too long: nothing is created.
    assert!(folder_import::plan(&lists, &file, &long, Naming::Prefixed).is_err());
    assert!(
        folder_import::apply(&mut lists, &file, &long, Naming::Prefixed, 1, counter()).is_err()
    );
    assert_eq!(lists, before);
    // Without prefix the same name is fine for this file.
    assert!(folder_import::plan(&lists, &file, &long, Naming::Subfolder).is_ok());
}

#[test]
fn favorites_are_never_imported_into() {
    let file = parse("[ROOT_FOLDER]\npacman\n");
    let mut lists = Lists::new();
    folder_import::apply(
        &mut lists,
        &file,
        "Favorites",
        Naming::Subfolder,
        1,
        counter(),
    )
    .expect("apply");
    // A user list named "Favorites" is not the system list.
    assert!(lists.get(FAVORITES).expect("favorites").entries.is_empty());
    assert_eq!(machines(&lists, "Favorites"), ["pacman"]);
}

// Custom lists

#[test]
fn custom_lists_are_created_renamed_and_deleted_by_id() {
    let mut lists = Lists::new();
    lists.create_user_list("a", "Shooters", 10).expect("create");
    lists
        .create_user_list("b", "Empty", 10)
        .expect("create empty");
    assert!(matches!(
        lists.create_user_list("c", " shooters ", 11),
        Err(ListError::NameInUse(name)) if name == "Shooters"
    ));
    assert!(lists.create_user_list("c", "", 11).is_err());
    assert!(lists.create_user_list("c", "Tab\tname", 11).is_err());
    assert!(
        lists
            .create_user_list("c", &"x".repeat(MAX_LIST_NAME_LENGTH + 1), 11)
            .is_err()
    );
    assert!(lists.create_user_list("favorites", "Other", 11).is_err());
    let pacman = ListEntry::machine("pacman");
    lists
        .set_user_memberships(&pacman, &[("a".to_owned(), true)], 12)
        .expect("add");

    // Renaming keeps the id and the entries; case changes are allowed.
    lists
        .rename_user_list("a", "SHOOTERS / Vertical", 13)
        .expect("rename");
    lists
        .rename_user_list("a", "Shooters / vertical", 14)
        .expect("case");
    let renamed = lists.get("a").expect("a");
    assert_eq!(renamed.display_name(), "Shooters / vertical");
    assert_eq!(renamed.entries, std::slice::from_ref(&pacman));
    assert_eq!(
        (renamed.created_at, renamed.updated_at),
        (Some(10), Some(14))
    );
    // A name in use is refused, never merged.
    assert!(matches!(
        lists.rename_user_list("b", "shooters / VERTICAL", 15),
        Err(ListError::NameInUse(_))
    ));
    assert!(lists.get("b").expect("b").entries.is_empty());
    assert!(matches!(
        lists.rename_user_list(FAVORITES, "Mine", 15),
        Err(ListError::SystemList(_))
    ));
    assert!(matches!(
        lists.rename_user_list("missing", "Mine", 15),
        Err(ListError::UnknownList(_))
    ));

    // Deleting removes only that list.
    lists
        .set_membership(FAVORITES, &pacman, true)
        .expect("favorite");
    lists.delete_user_list("a").expect("delete");
    assert_eq!(user_names(&lists), ["Empty"]);
    assert!(lists.get(FAVORITES).expect("favorites").contains(&pacman));
    assert!(matches!(
        lists.delete_user_list(FAVORITES),
        Err(ListError::SystemList(_))
    ));
    assert!(lists.delete_user_list("a").is_err());
    assert_eq!(lists.validate(), Ok(()));
}

#[test]
fn memberships_change_together_and_only_in_the_named_lists() {
    let mut lists = Lists::new();
    for (id, name) in [("a", "A"), ("b", "B"), ("c", "C")] {
        lists.create_user_list(id, name, 1).expect("create");
    }
    let pacman = ListEntry::machine("pacman");
    let changes = [("a".to_owned(), true), ("b".to_owned(), true)];
    assert_eq!(lists.set_user_memberships(&pacman, &changes, 2), Ok(2));
    // Wanted states: repeating changes nothing.
    assert_eq!(lists.set_user_memberships(&pacman, &changes, 3), Ok(0));
    assert_eq!(lists.get("a").expect("a").updated_at, Some(2));
    // Added elsewhere in the meantime: a later change of `a` keeps `c`.
    lists
        .set_user_memberships(&pacman, &[("c".to_owned(), true)], 4)
        .expect("c");
    lists
        .set_user_memberships(&pacman, &[("a".to_owned(), false)], 5)
        .expect("remove");
    assert_eq!(lists.user_lists_containing(&pacman), ["b", "c"]);

    // A deleted list or a system list fails everything.
    let before = lists.clone();
    for changes in [
        vec![("b".to_owned(), false), ("gone".to_owned(), true)],
        vec![("b".to_owned(), false), (FAVORITES.to_owned(), true)],
    ] {
        assert!(lists.set_user_memberships(&pacman, &changes, 6).is_err());
        assert_eq!(lists, before);
    }
    // Machines missing from every catalog are kept like any other.
    let unknown = ListEntry::machine("notamachine");
    lists
        .set_user_memberships(&unknown, &[("a".to_owned(), true)], 7)
        .expect("unknown");
    assert!(lists.get("a").expect("a").contains(&unknown));
    assert!(
        lists
            .set_user_memberships(
                &ListEntry::machine("bad name"),
                &[("a".to_owned(), true)],
                7
            )
            .is_err()
    );
}

#[test]
fn stored_lists_are_checked_for_names_and_count() {
    let mut lists = Lists::new();
    lists.create_user_list("a", "Maze", 1).expect("create");
    let mut duplicate = lists.clone();
    duplicate.lists.push(duplicate.lists[1].clone());
    duplicate.lists[2].id = "b".to_owned();
    duplicate.lists[2].name = Some("MAZE".to_owned());
    assert!(duplicate.validate().is_err());

    let mut many = Lists::new();
    for index in 0..MAX_USER_LISTS {
        many.create_user_list(&format!("l{index}"), &format!("List {index}"), 1)
            .expect("create");
    }
    assert!(many.create_user_list("extra", "Extra", 1).is_err());
    assert_eq!(many.validate(), Ok(()));
}

#[test]
fn files_written_before_custom_lists_still_load() {
    let old = r#"{ "schemaVersion": 1, "lists": [
        { "id": "favorites", "kind": "system", "entries": [ { "kind": "machine", "name": "pacman" } ] }
    ] }"#;
    let lists: Lists = serde_json::from_str(old).expect("parse");
    assert_eq!(lists.validate(), Ok(()));
    assert!(
        lists
            .get(FAVORITES)
            .expect("favorites")
            .created_at
            .is_none()
    );
    // Dates of user lists are written; system lists have none.
    let mut lists = lists;
    lists.create_user_list("a", "Maze", 7).expect("create");
    let json = serde_json::to_value(&lists).expect("json");
    assert_eq!(json["lists"][1]["createdAt"], 7);
    assert!(json["lists"][0].get("createdAt").is_none());
}

/// The sample files of the pS support files, read from a directory given by
/// `YMDB_FOLDER_FILES` (`bootlegs.ini`, `category.ini`, `mame.ini`). Run with
/// `cargo test --release --lib real_folder_files -- --ignored --nocapture`.
#[test]
#[ignore = "needs the real folder files"]
fn real_folder_files() {
    let dir = std::env::var("YMDB_FOLDER_FILES").expect("YMDB_FOLDER_FILES");
    let mut lists = Lists::new();
    for (file_name, import_name) in [
        ("bootlegs.ini", "Bootlegs"),
        ("mame.ini", "MAME"),
        ("category.ini", "Category"),
    ] {
        let started = std::time::Instant::now();
        let file = ini::read_file(&std::path::Path::new(&dir).join(file_name)).expect(file_name);
        let parsed = started.elapsed();
        let plan = folder_import::plan(&lists, &file, import_name, Naming::Prefixed).expect("plan");
        let first_id = lists.lists.len();
        let outcome = folder_import::apply(
            &mut lists,
            &file,
            import_name,
            Naming::Prefixed,
            1,
            counter_from(first_id),
        )
        .expect("apply");
        println!(
            "{file_name}: root {} machines, {} folders ({} empty), {} duplicates, warnings {:?}; \
             plan {} to create, {} to merge, {} entries; parsed in {:.1} ms, applied in {:.1} ms",
            file.root.len(),
            file.folders.len(),
            file.empty_folders,
            file.duplicates,
            file.warnings
                .iter()
                .map(|warning| (warning.kind, warning.count))
                .collect::<Vec<_>>(),
            plan.to_create(),
            plan.to_merge(),
            plan.new_entries(),
            parsed.as_secs_f64() * 1000.0,
            (started.elapsed() - parsed).as_secs_f64() * 1000.0,
        );
        assert_eq!(outcome.created, plan.to_create());
        // Idempotent.
        let again =
            folder_import::plan(&lists, &file, import_name, Naming::Prefixed).expect("again");
        assert_eq!((again.to_create(), again.new_entries()), (0, 0));
    }
    let started = std::time::Instant::now();
    assert_eq!(lists.validate(), Ok(()));
    let json = serde_json::to_vec_pretty(&lists).expect("json");
    let compact = serde_json::to_vec(&lists).expect("json");
    println!(
        "{} lists, {} entries; lists.json {} bytes (compact {} bytes), validated and serialized in {:.1} ms",
        lists.user_lists().count(),
        lists
            .lists
            .iter()
            .map(|list| list.entries.len())
            .sum::<usize>(),
        json.len(),
        compact.len(),
        started.elapsed().as_secs_f64() * 1000.0
    );
}

fn counter_from(start: usize) -> impl FnMut() -> String {
    let mut next = start;
    move || {
        next += 1;
        format!("list-{next}")
    }
}
