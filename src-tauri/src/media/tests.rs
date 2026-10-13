//! Media configuration, resolution and protocol tests on temporary
//! directories. File contents are placeholders, not real artwork.

use std::fs;
use std::path::{Path, PathBuf};

use super::config::{
    MEDIA_FILE_NAME, MediaConfig, MediaConfigStore, MediaStoreError, SourceStatus, source_status,
};
use super::protocol::{CategoryImages, MediaTarget, list_images, respond};
use super::resolve::{MAX_IMAGES, MediaKey, resolve, resolve_all};

fn write(path: &Path, content: &str) {
    fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
    fs::write(path, content).expect("write");
}

fn config(category: &str, sources: &[&Path]) -> MediaConfig {
    let mut config = MediaConfig::new();
    config.sources.insert(
        category.to_owned(),
        sources.iter().map(|path| path.to_path_buf()).collect(),
    );
    config
}

fn resolved(sources: &[PathBuf], key: MediaKey<'_>) -> Option<String> {
    resolve(sources, key).map(|media| fs::read_to_string(media.path).expect("read"))
}

#[test]
fn validates_the_configuration() {
    let dir = tempfile::tempdir().expect("temp dir");
    assert_eq!(config("snap", &[dir.path()]).validate(), Ok(()));
    assert!(config("screenshots", &[dir.path()]).validate().is_err());
    assert!(
        config("snap", &[Path::new("relative/snap")])
            .validate()
            .is_err()
    );
    assert!(
        config("snap", &[dir.path(), dir.path()])
            .validate()
            .is_err()
    );
    let many: Vec<PathBuf> = (0..17)
        .map(|index| dir.path().join(index.to_string()))
        .collect();
    let mut too_many = MediaConfig::new();
    too_many.sources.insert("snap".to_owned(), many);
    assert!(too_many.validate().is_err());
    let mut future = MediaConfig::new();
    future.schema_version = 2;
    assert!(future.validate().is_err());
    // Missing directories are allowed: they are reported as a status.
    assert_eq!(
        config("snap", &[&dir.path().join("offline share")]).validate(),
        Ok(())
    );
}

#[test]
fn reports_the_status_of_sources() {
    let dir = tempfile::tempdir().expect("temp dir");
    let file = dir.path().join("file.png");
    write(&file, "x");
    assert_eq!(source_status(dir.path()), SourceStatus::Available);
    assert_eq!(
        source_status(&dir.path().join("missing")),
        SourceStatus::Missing
    );
    assert_eq!(source_status(&file), SourceStatus::NotADirectory);
}

#[test]
fn persists_sources_with_unicode_paths() {
    let dir = tempfile::tempdir().expect("temp dir");
    let store = MediaConfigStore::in_dir(&dir.path().join("config ✓"));
    assert_eq!(store.load().expect("default"), MediaConfig::new());

    let snaps = dir.path().join("Émulateurs MAME ✓").join("snap");
    let mut config = config("snap", &[&snaps, &dir.path().join("snap_SL")]);
    config.sources.insert("flyers".to_owned(), Vec::new());
    let saved = store.save(config).expect("save");
    assert!(!saved.sources.contains_key("flyers"));
    assert_eq!(store.load().expect("load"), saved);
    let json: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(dir.path().join("config ✓").join(MEDIA_FILE_NAME)).expect("read"),
    )
    .expect("json");
    assert_eq!(json["schemaVersion"], 1);
    assert_eq!(json["sources"]["snap"][0], snaps.to_string_lossy().as_ref());
}

#[test]
fn reports_invalid_files_and_rejects_invalid_configurations() {
    let dir = tempfile::tempdir().expect("temp dir");
    let store = MediaConfigStore::in_dir(dir.path());
    for content in [
        "{ broken",
        r#"{ "sources": { "music": ["C:\\music"] } }"#,
        r#"{ "sources": { "snap": ["relative"] } }"#,
        r#"{ "schemaVersion": 7 }"#,
        r#"{ "unknown": true }"#,
    ] {
        fs::write(dir.path().join(MEDIA_FILE_NAME), content).expect("write");
        assert!(
            matches!(store.load(), Err(MediaStoreError::Invalid { .. })),
            "{content}"
        );
    }
    assert!(matches!(
        store.save(config("snap", &[Path::new("relative")])),
        Err(MediaStoreError::Rejected(_))
    ));
}

#[test]
fn resolves_machine_images_by_short_name() {
    let dir = tempfile::tempdir().expect("temp dir");
    let snap = dir.path().join("snap ✓");
    write(&snap.join("pacman.png"), "pacman png");
    write(&snap.join("pacman.jpg"), "pacman jpg");
    write(&snap.join("galaga.jpg"), "galaga jpg");
    write(
        &snap.join("pong").join("0000.png"),
        "pong screenshot by MAME",
    );
    write(&snap.join("puckman.bmp"), "unsupported format");
    write(&snap.join("dkong.gif"), "unsupported format");
    fs::create_dir_all(snap.join("digdug.png")).expect("directory named like an image");
    let sources = vec![snap];

    assert_eq!(
        resolved(&sources, MediaKey::Machine("pacman")).as_deref(),
        Some("pacman png")
    );
    assert_eq!(
        resolved(&sources, MediaKey::Machine("galaga")).as_deref(),
        Some("galaga jpg")
    );
    assert_eq!(
        resolved(&sources, MediaKey::Machine("pong")).as_deref(),
        Some("pong screenshot by MAME")
    );
    for missing in ["puckman", "dkong", "digdug", "mspacman"] {
        assert_eq!(
            resolved(&sources, MediaKey::Machine(missing)),
            None,
            "{missing}"
        );
    }
    let png = resolve(&sources, MediaKey::Machine("pacman")).expect("png");
    assert_eq!(png.content_type, "image/png");
    assert_eq!(
        resolve(&sources, MediaKey::Machine("galaga"))
            .expect("jpg")
            .content_type,
        "image/jpeg"
    );
}

#[test]
fn searches_sources_in_order_of_precedence() {
    let dir = tempfile::tempdir().expect("temp dir");
    let first = dir.path().join("first");
    let second = dir.path().join("second");
    write(&first.join("pacman.jpg"), "first");
    write(&second.join("pacman.png"), "second");
    write(&second.join("galaga.png"), "second galaga");
    let missing = dir.path().join("offline");
    let sources = vec![missing, first, second];

    // The first directory wins even with a less preferred format.
    assert_eq!(
        resolved(&sources, MediaKey::Machine("pacman")).as_deref(),
        Some("first")
    );
    assert_eq!(
        resolved(&sources, MediaKey::Machine("galaga")).as_deref(),
        Some("second galaga")
    );
    assert_eq!(resolved(&[], MediaKey::Machine("pacman")), None);
}

#[test]
fn resolves_software_images_by_list_and_name() {
    let dir = tempfile::tempdir().expect("temp dir");
    let snap = dir.path().join("snap_SL");
    write(&snap.join("nes").join("tetris.png"), "nes tetris");
    write(&snap.join("c64_cart").join("tetris.png"), "c64 tetris");
    write(&snap.join("tetris.png"), "machine tetris");
    let sources = vec![snap];

    assert_eq!(
        resolved(
            &sources,
            MediaKey::Software {
                list: "nes",
                name: "tetris"
            }
        )
        .as_deref(),
        Some("nes tetris")
    );
    assert_eq!(
        resolved(
            &sources,
            MediaKey::Software {
                list: "c64_cart",
                name: "tetris"
            }
        )
        .as_deref(),
        Some("c64 tetris")
    );
    // A software is never matched by a machine image of the same name.
    assert_eq!(
        resolved(
            &sources,
            MediaKey::Software {
                list: "gameboy",
                name: "tetris"
            }
        ),
        None
    );
}

#[test]
fn rejects_names_that_are_not_short_names() {
    let dir = tempfile::tempdir().expect("temp dir");
    let snap = dir.path().join("snap");
    write(&snap.join("pacman.png"), "x");
    write(&dir.path().join("secret.png"), "secret");
    let sources = vec![snap];
    for name in [
        "../secret",
        "..",
        "PACMAN",
        "pac man",
        "",
        "pacman.png",
        "C:\\secret",
    ] {
        assert_eq!(resolved(&sources, MediaKey::Machine(name)), None, "{name}");
    }
    assert_eq!(
        resolved(
            &sources,
            MediaKey::Software {
                list: "..",
                name: "secret"
            }
        ),
        None
    );
}

/// Creates a directory link (a junction on Windows, a symbolic link elsewhere).
fn link_dir(link: &Path, target: &Path) -> bool {
    #[cfg(windows)]
    {
        std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(link)
            .arg(target)
            .output()
            .is_ok_and(|output| output.status.success())
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link).is_ok()
    }
}

#[test]
fn links_must_stay_inside_the_source() {
    let dir = tempfile::tempdir().expect("temp dir");
    let snap = dir.path().join("snap");
    let outside = dir.path().join("outside");
    write(&outside.join("secret.png"), "outside");
    write(&snap.join("real_list").join("game.png"), "inside");
    fs::create_dir_all(&snap).expect("snap");
    if !link_dir(&snap.join("evil"), &outside)
        || !link_dir(&snap.join("alias"), &snap.join("real_list"))
    {
        eprintln!("directory links are not available; skipping");
        return;
    }
    let sources = vec![snap];
    assert_eq!(
        resolved(
            &sources,
            MediaKey::Software {
                list: "evil",
                name: "secret"
            }
        ),
        None
    );
    assert_eq!(
        resolved(
            &sources,
            MediaKey::Software {
                list: "alias",
                name: "game"
            }
        )
        .as_deref(),
        Some("inside")
    );
}

#[test]
fn serves_images_by_identifiers_only() {
    let dir = tempfile::tempdir().expect("temp dir");
    let snap = dir.path().join("Émulateurs ✓").join("snap");
    let titles = dir.path().join("titles");
    write(&snap.join("pacman.png"), "pacman");
    write(&snap.join("nes").join("smb.jpg"), "smb");
    write(&titles.join("pacman.png"), "pacman title");
    write(&dir.path().join("secret.png"), "secret");
    let mut config = config("snap", &[&snap]);
    config.sources.insert("titles".to_owned(), vec![titles]);

    let ok = respond(&config, "/snap/machine/pacman");
    assert_eq!(
        (ok.status, ok.content_type, ok.body.as_slice()),
        (200, "image/png", b"pacman".as_slice())
    );
    // `convertFileSrc` encodes the whole path as one segment.
    assert_eq!(
        respond(&config, "/snap%2Fsoftware%2Fnes%2Fsmb").body,
        b"smb"
    );
    assert_eq!(
        respond(&config, "/snap/software/nes/smb").content_type,
        "image/jpeg"
    );
    assert_eq!(
        respond(&config, "/titles/machine/pacman").body,
        b"pacman title"
    );
    assert_eq!(respond(&config, "/flyers/machine/pacman").status, 404);
    assert_eq!(respond(&config, "/snap/machine/galaga").status, 404);

    for bad in [
        "/snap/machine/..%2F..%2Fsecret",
        "/snap/machine/../secret",
        "/snap/software/../../secret",
        "/snap/machine/C:%5Csecret",
        "/snap/file/pacman",
        "/music/machine/pacman",
        "/snap/machine/pacman/extra",
        "/snap/machine/%zz",
        "/snap/machine/%",
        "/",
        "",
    ] {
        let response = respond(&config, bad);
        assert!(
            matches!(response.status, 400 | 404),
            "{bad}: {}",
            response.status
        );
        assert!(response.body.is_empty(), "{bad}");
    }
}

fn contents(sources: &[PathBuf], key: MediaKey<'_>) -> Vec<String> {
    resolve_all(sources, key)
        .into_iter()
        .map(|media| fs::read_to_string(media.path).expect("read"))
        .collect()
}

#[test]
fn lists_every_image_of_a_title_in_order() {
    let dir = tempfile::tempdir().expect("temp dir");
    let first = dir.path().join("first");
    let second = dir.path().join("second");
    write(&first.join("pacman.png"), "first named");
    write(&first.join("pacman.jpg"), "first other format");
    write(&first.join("pacman").join("0010.png"), "first 10");
    write(&first.join("pacman").join("0002.png"), "first 2");
    write(&first.join("pacman").join("10000.png"), "first 10000");
    write(&first.join("pacman").join("notes.png"), "not numbered");
    write(&first.join("pacman").join("0003.txt"), "not an image");
    write(&first.join("pacman").join("0004.PNG"), "first 4 upper case");
    write(&second.join("pacman.jpg"), "second named");
    write(&second.join("pong").join("0000.png"), "second pong");
    let sources = vec![dir.path().join("offline"), first.clone(), second];

    assert_eq!(
        contents(&sources, MediaKey::Machine("pacman")),
        [
            "first named",
            "first 2",
            "first 4 upper case",
            "first 10",
            "first 10000",
            "second named"
        ]
    );
    // The first image is the one served without an index.
    assert_eq!(
        resolved(&sources, MediaKey::Machine("pacman")).as_deref(),
        Some("first named")
    );
    // Without a named image, the first screenshot comes first.
    assert_eq!(
        resolved(&sources, MediaKey::Machine("pong")).as_deref(),
        Some("second pong")
    );
    // A directory listed twice contributes its images once.
    let twice = vec![first.clone(), first.join(".").join("")];
    assert_eq!(contents(&twice, MediaKey::Machine("pacman")).len(), 5);
    assert!(contents(&sources, MediaKey::Machine("galaga")).is_empty());
    assert!(contents(&sources, MediaKey::Machine("../first")).is_empty());
}

#[test]
fn lists_one_image_per_source_for_software() {
    let dir = tempfile::tempdir().expect("temp dir");
    let first = dir.path().join("first");
    let second = dir.path().join("second");
    write(&first.join("nes").join("smb.png"), "first smb");
    write(
        &first.join("nes").join("smb").join("0000.png"),
        "not a convention",
    );
    write(&second.join("nes").join("smb.webp"), "second smb");
    let key = MediaKey::Software {
        list: "nes",
        name: "smb",
    };
    assert_eq!(contents(&[first, second], key), ["first smb", "second smb"]);
}

#[test]
fn limits_the_number_of_listed_images() {
    let dir = tempfile::tempdir().expect("temp dir");
    let snap = dir.path().join("snap");
    for index in 0..MAX_IMAGES + 5 {
        write(
            &snap.join("pacman").join(format!("{index:04}.png")),
            &index.to_string(),
        );
    }
    let listed = contents(&[snap], MediaKey::Machine("pacman"));
    assert_eq!(listed.len(), MAX_IMAGES);
    assert_eq!(listed.first().map(String::as_str), Some("0"));
}

#[test]
fn serves_and_lists_images_by_index() {
    let dir = tempfile::tempdir().expect("temp dir");
    let snap = dir.path().join("snap");
    let flyers = dir.path().join("flyers");
    write(&snap.join("pacman.png"), "named");
    write(&snap.join("pacman").join("0000.png"), "screenshot 0");
    write(&snap.join("pacman").join("0001.png"), "screenshot 1");
    write(&flyers.join("pacman.jpg"), "flyer");
    write(&snap.join("nes").join("smb.png"), "smb");
    let mut config = config("snap", &[&snap]);
    config.sources.insert("flyers".to_owned(), vec![flyers]);

    assert_eq!(respond(&config, "/snap/machine/pacman/0").body, b"named");
    assert_eq!(
        respond(&config, "/snap%2Fmachine%2Fpacman%2F2").body,
        b"screenshot 1"
    );
    assert_eq!(respond(&config, "/snap/machine/pacman/3").status, 404);
    assert_eq!(respond(&config, "/snap/software/nes/smb/0").body, b"smb");
    assert_eq!(respond(&config, "/snap/software/nes/smb/1").status, 404);
    for bad in [
        "/snap/machine/pacman/01",
        "/snap/machine/pacman/-1",
        "/snap/machine/pacman/1000",
        "/snap/machine/pacman/",
        "/snap/machine/pacman/1/2",
        "/snap/software/nes/smb/x",
    ] {
        assert_eq!(respond(&config, bad).status, 400, "{bad}");
    }

    let machine: MediaTarget =
        serde_json::from_str(r#"{ "kind": "machine", "name": "pacman" }"#).expect("machine");
    assert_eq!(
        list_images(&config, &machine),
        [
            CategoryImages {
                category: "snap",
                count: 3
            },
            CategoryImages {
                category: "flyers",
                count: 1
            }
        ]
    );
    let software: MediaTarget =
        serde_json::from_str(r#"{ "kind": "software", "list": "nes", "name": "smb" }"#)
            .expect("software");
    assert_eq!(
        serde_json::to_value(list_images(&config, &software)).expect("json"),
        serde_json::json!([{ "category": "snap", "count": 1 }])
    );
    let invalid: MediaTarget =
        serde_json::from_str(r#"{ "kind": "machine", "name": "../snap" }"#).expect("invalid");
    assert!(list_images(&config, &invalid).is_empty());
}
