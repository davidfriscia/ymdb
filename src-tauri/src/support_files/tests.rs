//! Tests of the support file parsers and of the global repository, on small
//! fixtures shaped like the files of MAME 0.288 (machine names match the
//! search fixture catalog; `galaga` is in no catalog fixture).

use super::model::{
    Attribute, Dimension, Membership, ParsedSupportFile, SupportFileFormat, WarningKind,
};
use super::parser::{ParseError, parse};
use super::repository::{RepositoryError, SupportRepository};

pub const CATVER: &str = include_str!("testdata/catver.ini");
pub const GENRE: &str = include_str!("testdata/genre.ini");
pub const CATLIST: &str = include_str!("testdata/catlist.ini");
pub const VERSION: &str = include_str!("testdata/version.ini");
pub const SERIES: &str = include_str!("testdata/series.ini");

/// The parsed fixture of `format`.
pub fn fixture(format: SupportFileFormat) -> ParsedSupportFile {
    let content = match format {
        SupportFileFormat::Catver => CATVER,
        SupportFileFormat::Genre => GENRE,
        SupportFileFormat::Catlist => CATLIST,
        SupportFileFormat::Version => VERSION,
        SupportFileFormat::Series => SERIES,
    };
    parse(content.as_bytes(), Some(format.file_name())).expect("fixture parses")
}

fn parse_text(text: &str) -> Result<ParsedSupportFile, ParseError> {
    parse(text.as_bytes(), None)
}

fn warning_count(parsed: &ParsedSupportFile, kind: WarningKind) -> u64 {
    parsed
        .warnings
        .iter()
        .find(|warning| warning.kind == kind)
        .map_or(0, |warning| warning.count)
}

fn values_of<'a>(parsed: &'a ParsedSupportFile, machine: &str) -> Vec<&'a str> {
    parsed
        .memberships
        .iter()
        .filter(|entry| entry.machine == machine)
        .map(|entry| entry.value.as_str())
        .collect()
}

// Parsers

#[test]
fn parses_catver_categories_and_version_added_as_an_attribute() {
    let parsed = fixture(SupportFileFormat::Catver);

    assert_eq!(parsed.format, SupportFileFormat::Catver);
    assert_eq!(parsed.declared_version.as_deref(), Some("0.288"));
    assert!(parsed.warnings.is_empty(), "{:?}", parsed.warnings);
    assert!(
        parsed
            .memberships
            .iter()
            .all(|entry| entry.dimension == Dimension::Category)
    );
    assert_eq!(values_of(&parsed, "sf2"), ["Fighter / Versus"]);
    assert!(parsed.attributes.contains(&Attribute {
        dimension: Dimension::VersionAdded,
        machine: "mspacman".to_owned(),
        value: "0.10".to_owned(),
    }));
    let statistics = parsed.statistics();
    assert_eq!(statistics.len(), 2);
    assert_eq!(statistics[0].dimension, Dimension::Category);
    assert_eq!(statistics[0].machine_count, 13);
    assert_eq!(statistics[1].dimension, Dimension::VersionAdded);
    assert_eq!(parsed.machine_count(), 13);
}

#[test]
fn parses_section_formats_with_multiple_membership() {
    let genre = fixture(SupportFileFormat::Genre);
    assert_eq!(genre.declared_version.as_deref(), Some("0.287"));
    assert_eq!(values_of(&genre, "pacman"), ["Maze"]);

    let catlist = fixture(SupportFileFormat::Catlist);
    assert_eq!(values_of(&catlist, "pacman"), ["Arcade: Maze / Collect"]);
    assert!(
        catlist
            .memberships
            .iter()
            .all(|entry| entry.dimension == Dimension::Category)
    );

    let series = fixture(SupportFileFormat::Series);
    assert_eq!(
        values_of(&series, "sf2"),
        ["Street Fighter", "Street Fighter II"]
    );
    assert_eq!(series.statistics()[0].value_count, 5);

    let version = fixture(SupportFileFormat::Version);
    assert!(version.memberships.is_empty());
    assert_eq!(version.attributes.len(), 8);
}

#[test]
fn identifies_the_format_from_the_header_not_the_file_name() {
    // A genre file saved under another name is still a genre file.
    let parsed = parse(GENRE.as_bytes(), Some("series.ini")).expect("parse");
    assert_eq!(parsed.format, SupportFileFormat::Genre);

    // Without a header: catver and version files are identified by structure.
    let catver = parse_text("[Category]\npacman=Maze / Collect\n").expect("catver");
    assert_eq!(catver.format, SupportFileFormat::Catver);
    let version = parse_text("[0.1]\npacman\n[0.37b5]\ngalaga\n").expect("version");
    assert_eq!(version.format, SupportFileFormat::Version);

    // Genre, catlist and series look alike: only the file name can tell them apart.
    let text = "[Shooter]\ngalaga\n";
    assert!(matches!(parse_text(text), Err(ParseError::Unidentified)));
    let series = parse(text.as_bytes(), Some("Series.INI")).expect("series");
    assert_eq!(series.format, SupportFileFormat::Series);
    assert_eq!(series.declared_version, None);
}

#[test]
fn rejects_unsupported_and_inconsistent_files() {
    let mature = ";; MATURE.ini 0.288 / 09-Jun-26 / MAME 0.288 ;;\n[ROOT_FOLDER]\npacman\n";
    assert!(
        matches!(parse_text(mature), Err(ParseError::Unsupported { name }) if name == "MATURE.ini")
    );
    let not_catver = ";; catver.ini 0.288 ;;\n[Shooter]\ngalaga\n";
    assert!(matches!(
        parse_text(not_catver),
        Err(ParseError::Invalid {
            format: SupportFileFormat::Catver,
            ..
        })
    ));
    let catver_named_genre = ";; genre.ini 0.288 ;;\n[Category]\npacman=Maze\n";
    assert!(matches!(
        parse_text(catver_named_genre),
        Err(ParseError::Invalid {
            format: SupportFileFormat::Genre,
            ..
        })
    ));
    let empty = ";; series.ini 0.288 ;;\n[FOLDER_SETTINGS]\nRootFolderIcon mame\n[ROOT_FOLDER]\n";
    assert!(matches!(
        parse_text(empty),
        Err(ParseError::Empty {
            format: SupportFileFormat::Series
        })
    ));
    assert!(matches!(
        parse(b"", Some("catver.ini")),
        Err(ParseError::Invalid { .. })
    ));
}

#[test]
fn handles_comments_blank_lines_line_endings_and_bom() {
    let text = "\u{feff};; series.ini 0.288 ;;\r\n\r\n# comment\r\n[Pac-Man]\r\n  pacman  \r\n; another comment\r\nmspacman\r\n";
    let parsed = parse_text(text).expect("parse");
    assert_eq!(parsed.format, SupportFileFormat::Series);
    assert_eq!(
        parsed.memberships,
        [
            Membership {
                dimension: Dimension::Series,
                value: "Pac-Man".to_owned(),
                machine: "mspacman".to_owned(),
            },
            Membership {
                dimension: Dimension::Series,
                value: "Pac-Man".to_owned(),
                machine: "pacman".to_owned(),
            },
        ]
    );
    assert!(parsed.warnings.is_empty());

    // Old Mac line endings.
    let parsed = parse_text(";; genre.ini ;;\r[Maze]\rpacman\r").expect("parse");
    assert_eq!(values_of(&parsed, "pacman"), ["Maze"]);
}

#[test]
fn decodes_windows_1252_files_with_a_warning() {
    let mut content = b";; series.ini 0.288 ;;\n[Pok".to_vec();
    content.push(0xE9);
    content.extend_from_slice(b"mon \x93Gold\x94]\npacman\n");
    let parsed = parse(&content, None).expect("parse");

    assert_eq!(values_of(&parsed, "pacman"), ["Pokémon “Gold”"]);
    assert_eq!(warning_count(&parsed, WarningKind::NotUtf8), 1);
}

#[test]
fn reports_duplicates_conflicts_and_malformed_lines() {
    let text = "\
;; catver.ini 0.288 ;;
orphan=Maze
[Category]
pacman=Maze / Collect
pacman=Maze / Collect
pacman=Shooter
no equal sign here
Bad Name=Maze
empty=
[VerAdded]
pacman=0.1
galaga=unknown
[Extra]
foo=bar
";
    let parsed = parse_text(text).expect("parse");

    assert_eq!(values_of(&parsed, "pacman"), ["Maze / Collect"]);
    assert_eq!(warning_count(&parsed, WarningKind::DuplicateEntry), 1);
    assert_eq!(warning_count(&parsed, WarningKind::ConflictingValue), 1);
    assert_eq!(warning_count(&parsed, WarningKind::InvalidLine), 2);
    assert_eq!(warning_count(&parsed, WarningKind::InvalidMachineName), 1);
    assert_eq!(
        warning_count(&parsed, WarningKind::EntriesOutsideSection),
        1
    );
    assert_eq!(warning_count(&parsed, WarningKind::IgnoredSection), 1);
    // Unordered versions are kept and reported.
    assert_eq!(warning_count(&parsed, WarningKind::UnrecognizedVersion), 1);
    assert_eq!(parsed.attributes.len(), 2);
    let invalid = parsed
        .warnings
        .iter()
        .find(|warning| warning.kind == WarningKind::InvalidLine)
        .expect("warning");
    assert_eq!(invalid.examples[0], "line 7: no equal sign here");
}

#[test]
fn reports_a_missing_catver_section_and_section_duplicates() {
    let parsed = parse_text(";; catver.ini ;;\n[VerAdded]\npacman=0.1\n").expect("parse");
    assert!(parsed.memberships.is_empty());
    assert_eq!(warning_count(&parsed, WarningKind::MissingSection), 1);
    assert_eq!(parsed.statistics().len(), 1);

    let text = ";; series.ini ;;\n[Pac-Man]\npacman\npacman\n[ROOT_FOLDER]\nmspacman\n\
                [Pac-Man]\npuckman\nPAC MAN\n";
    let parsed = parse_text(text).expect("parse");
    assert_eq!(parsed.memberships.len(), 2);
    assert_eq!(warning_count(&parsed, WarningKind::DuplicateEntry), 1);
    assert_eq!(warning_count(&parsed, WarningKind::InvalidLine), 1);
    // Frontend folders are not classifications.
    assert_eq!(warning_count(&parsed, WarningKind::IgnoredSection), 1);
}

#[test]
fn keeps_the_first_version_of_a_machine_listed_twice() {
    let parsed = parse_text(";; version.ini ;;\n[0.1]\npacman\n[0.2]\npacman\n").expect("parse");
    assert_eq!(parsed.attributes[0].value, "0.1");
    assert_eq!(warning_count(&parsed, WarningKind::ConflictingValue), 1);
}

#[test]
fn keeps_machines_unknown_to_every_catalog() {
    // `galaga` is not in the catalog fixtures: the parser does not know catalogs.
    let series = fixture(SupportFileFormat::Series);
    assert_eq!(values_of(&series, "galaga"), ["Galaga"]);
}

/// Parses the real files of MAME 0.288 when `YMDB_SUPPORT_FILES` names their
/// directory (`pS_SupportFiles_288`).
#[test]
#[ignore = "needs the real support files"]
fn parses_real_support_files() {
    let dir =
        std::path::PathBuf::from(std::env::var("YMDB_SUPPORT_FILES").expect("YMDB_SUPPORT_FILES"));
    for (path, format) in [
        ("CatVer/catver.ini", SupportFileFormat::Catver),
        ("CatVer/genre.ini", SupportFileFormat::Genre),
        ("CatVer/catlist.ini", SupportFileFormat::Catlist),
        ("Version/version.ini", SupportFileFormat::Version),
        ("Series/series.ini", SupportFileFormat::Series),
    ] {
        let started = std::time::Instant::now();
        let parsed = super::parser::read_file(&dir.join(path)).expect(path);
        assert_eq!(parsed.format, format);
        println!(
            "{path}: {} machines, {:?}, {} warnings, {:.1} ms",
            parsed.machine_count(),
            parsed.statistics(),
            parsed.warnings.len(),
            started.elapsed().as_secs_f64() * 1000.0
        );
        for warning in &parsed.warnings {
            println!("  {warning:?}");
        }
    }
    let mature = super::parser::read_file(&dir.join("CatVer/mature.ini"));
    assert!(matches!(mature, Err(ParseError::Unsupported { .. })));
}

// Repository

pub fn repository() -> (tempfile::TempDir, SupportRepository) {
    let dir = tempfile::tempdir().expect("temp dir");
    let repository = SupportRepository::in_dir(dir.path());
    (dir, repository)
}

fn revisions(repository: &SupportRepository) -> Vec<(Dimension, i64, Option<SupportFileFormat>)> {
    repository
        .snapshot()
        .expect("snapshot")
        .dimensions
        .into_iter()
        .map(|entry| (entry.dimension, entry.revision, entry.source))
        .collect()
}

#[test]
fn first_imported_source_of_a_dimension_becomes_active() {
    let (_dir, repository) = repository();
    let empty = repository.view().expect("view");
    assert!(empty.sources.is_empty());
    assert!(
        empty
            .dimensions
            .iter()
            .all(|entry| entry.active_source.is_none())
    );

    let outcome = repository
        .import("catver.ini", &fixture(SupportFileFormat::Catver), 1000)
        .expect("import");
    assert_eq!(
        outcome.activated,
        [Dimension::Category, Dimension::VersionAdded]
    );
    assert_eq!(outcome.changed, outcome.activated);

    // An alternative source is kept but does not replace the active one.
    let outcome = repository
        .import("catlist.ini", &fixture(SupportFileFormat::Catlist), 2000)
        .expect("import");
    assert!(outcome.activated.is_empty());
    assert!(outcome.changed.is_empty());

    let view = repository.view().expect("view");
    assert_eq!(view.sources.len(), 2);
    let catver = &view.sources[0];
    assert_eq!(catver.format, SupportFileFormat::Catver);
    assert_eq!(catver.file_name, "catver.ini");
    assert_eq!(catver.declared_version.as_deref(), Some("0.288"));
    assert_eq!(catver.imported_at, 1000);
    assert_eq!(catver.machine_count, 13);
    assert!(catver.dimensions.iter().all(|entry| entry.active));
    assert!(!view.sources[1].dimensions[0].active);
    let category = &view.dimensions[1];
    assert_eq!(category.dimension, Dimension::Category);
    assert_eq!(category.active_source, Some(SupportFileFormat::Catver));
    assert_eq!(
        category.available_sources,
        [SupportFileFormat::Catver, SupportFileFormat::Catlist]
    );
    assert_eq!(
        revisions(&repository),
        [
            (Dimension::Genre, 0, None),
            (Dimension::Category, 1, Some(SupportFileFormat::Catver)),
            (Dimension::Series, 0, None),
            (Dimension::VersionAdded, 1, Some(SupportFileFormat::Catver)),
        ]
    );
}

#[test]
fn reimport_changes_revisions_only_when_the_content_changes() {
    let (_dir, repository) = repository();
    let catver = fixture(SupportFileFormat::Catver);
    repository.import("catver.ini", &catver, 1).expect("import");

    // Same content (another file name and date): no revision changes.
    let outcome = repository
        .import("catver-copy.ini", &catver, 2)
        .expect("reimport");
    assert!(outcome.changed.is_empty());
    let view = repository.view().expect("view");
    assert_eq!(view.sources[0].file_name, "catver-copy.ini");
    assert_eq!(view.sources[0].imported_at, 2);

    // Only the categories change: Version Added keeps its revision.
    let mut changed = catver.clone();
    changed.memberships[0].value = "Computer / Laptop".to_owned();
    let outcome = repository
        .import("catver.ini", &changed, 3)
        .expect("reimport");
    assert_eq!(outcome.changed, [Dimension::Category]);
    let revisions = revisions(&repository);
    assert_eq!(revisions[1].1, 2);
    assert_eq!(revisions[3].1, 1);

    // A file without [VerAdded] no longer feeds Version Added: it is cleared.
    let mut categories_only = changed.clone();
    categories_only.attributes.clear();
    let outcome = repository
        .import("catver.ini", &categories_only, 4)
        .expect("reimport");
    assert_eq!(outcome.changed, [Dimension::VersionAdded]);
    let view = repository.view().expect("view");
    assert_eq!(view.dimensions[3].active_source, None);
    assert_eq!(view.sources[0].dimensions.len(), 1);
}

#[test]
fn changing_the_active_source_changes_the_revision() {
    let (_dir, repository) = repository();
    repository
        .import("catver.ini", &fixture(SupportFileFormat::Catver), 1)
        .expect("import");
    repository
        .import("catlist.ini", &fixture(SupportFileFormat::Catlist), 1)
        .expect("import");

    assert!(
        repository
            .set_active(Dimension::Category, Some(SupportFileFormat::Catlist))
            .expect("set")
    );
    assert!(
        !repository
            .set_active(Dimension::Category, Some(SupportFileFormat::Catlist))
            .expect("same")
    );
    assert_eq!(
        revisions(&repository)[1],
        (Dimension::Category, 2, Some(SupportFileFormat::Catlist))
    );

    // A source that does not provide the dimension is refused.
    let refused = repository.set_active(Dimension::Genre, Some(SupportFileFormat::Catver));
    assert!(matches!(refused, Err(RepositoryError::Invalid(_))));
    let refused = repository.set_active(Dimension::Series, Some(SupportFileFormat::Series));
    assert!(matches!(refused, Err(RepositoryError::Invalid(_))));

    // Clearing is explicit too.
    assert!(
        repository
            .set_active(Dimension::Category, None)
            .expect("clear")
    );
    assert_eq!(revisions(&repository)[1], (Dimension::Category, 3, None));
}

#[test]
fn removing_the_active_source_leaves_the_dimension_without_one() {
    let (_dir, repository) = repository();
    repository
        .import("catver.ini", &fixture(SupportFileFormat::Catver), 1)
        .expect("import");
    repository
        .import("catlist.ini", &fixture(SupportFileFormat::Catlist), 1)
        .expect("import");
    repository
        .import("version.ini", &fixture(SupportFileFormat::Version), 1)
        .expect("import");

    let changed = repository
        .remove(SupportFileFormat::Catver)
        .expect("remove");
    assert_eq!(changed, [Dimension::Category, Dimension::VersionAdded]);
    let view = repository.view().expect("view");
    // The alternatives are kept and not activated implicitly.
    assert_eq!(view.sources.len(), 2);
    assert_eq!(view.dimensions[1].active_source, None);
    assert_eq!(
        view.dimensions[1].available_sources,
        [SupportFileFormat::Catlist]
    );
    assert_eq!(view.dimensions[3].active_source, None);

    // Removing an inactive source changes no revision.
    assert!(
        repository
            .remove(SupportFileFormat::Catlist)
            .expect("remove")
            .is_empty()
    );
    assert!(matches!(
        repository.remove(SupportFileFormat::Catlist),
        Err(RepositoryError::NotFound(_))
    ));
    // The removed data is gone: reimporting starts from scratch.
    let outcome = repository
        .import("catver.ini", &fixture(SupportFileFormat::Catver), 2)
        .expect("import");
    assert_eq!(
        outcome.activated,
        [Dimension::Category, Dimension::VersionAdded]
    );
}

#[test]
fn a_failed_import_keeps_the_previous_source() {
    let (dir, repository) = repository();
    repository
        .import("series.ini", &fixture(SupportFileFormat::Series), 1)
        .expect("import");
    let before = repository.view().expect("view");

    // A parse failure never reaches the repository; a database failure rolls back.
    let connection =
        rusqlite::Connection::open(dir.path().join("support-files.sqlite")).expect("open");
    connection
        .execute_batch(
            "CREATE TRIGGER fail BEFORE INSERT ON sources BEGIN SELECT RAISE(ABORT, 'disk full'); END",
        )
        .expect("trigger");
    let mut changed = fixture(SupportFileFormat::Series);
    changed.memberships.truncate(1);
    let failed = repository.import("series.ini", &changed, 2);
    assert!(matches!(failed, Err(RepositoryError::Sqlite(_))));
    assert_eq!(repository.view().expect("view"), before);
    assert_eq!(revisions(&repository)[2].1, 1);
}

#[test]
fn a_recreated_repository_has_another_identity() {
    let (dir, repository) = repository();
    let first = repository.snapshot().expect("snapshot").repository_id;
    assert_eq!(
        repository.snapshot().expect("snapshot").repository_id,
        first
    );
    std::fs::remove_file(dir.path().join("support-files.sqlite")).expect("remove");
    assert_ne!(
        repository.snapshot().expect("snapshot").repository_id,
        first
    );
}
