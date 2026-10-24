use std::fs;

use super::*;
use crate::controllers::model::MAME_PROFILE;
use crate::controllers::resolve::ControlsSource;
use crate::controllers::store::CONTROLLERS_FILE_NAME;

const BASE: &[u8] = br#"<?xml version="1.0"?>
<mameconfig version="10">
    <system name="default">
        <input>
            <port type="P1_BUTTON1"><newseq type="standard">KEYCODE_X</newseq></port>
        </input>
    </system>
</mameconfig>
"#;
const INSTALLATION: &str = "0123456789abcdef0123456789abcdef";

fn file(content: &[u8]) -> ControllerFile {
    ControllerFile {
        content: content.to_vec(),
        file_name: Some("xarcade.cfg".to_owned()),
    }
}

#[test]
fn creates_profiles_with_a_base_file_atomically() {
    let dir = tempfile::tempdir().expect("temp dir");
    let state = ControllersState::new(Some(dir.path()));
    let paths = ControllerPaths::new(dir.path());

    // Nothing is written until something changes.
    assert_eq!(
        state.read(controllers_view).expect("read").profiles.len(),
        1
    );
    assert!(fs::read_dir(dir.path()).expect("read").next().is_none());

    let id = state
        .create_profile("Six Buttons", Some(&file(BASE)))
        .expect("create");
    assert_eq!(fs::read(paths.base_file(&id)).expect("base"), BASE);
    assert!(
        state
            .read(|config| config.profile(&id).expect("profile").has_base)
            .expect("read")
    );

    // A failed save leaves no profile and no base file behind.
    let blocker = dir.path().join(format!("{CONTROLLERS_FILE_NAME}.tmp"));
    fs::create_dir(&blocker).expect("dir");
    assert!(matches!(
        state.create_profile("Gamepad", Some(&file(BASE))),
        Err(CommandError::Settings { .. })
    ));
    assert!(!paths.profile_dir("gamepad").exists());
    assert!(
        state
            .read(|config| config.profile("gamepad").is_none())
            .expect("read")
    );
    fs::remove_dir(&blocker).expect("remove");

    // Without a base, only controllers.json changes.
    let plain = state.create_profile("Gamepad", None).expect("create");
    assert!(!paths.profile_dir(&plain).exists());

    // Importing a base onto an existing profile replaces it verbatim.
    let other = String::from_utf8(BASE.to_vec())
        .expect("utf-8")
        .replace("KEYCODE_X", "KEYCODE_V")
        .into_bytes();
    state.import_base(&plain, &file(&other)).expect("import");
    assert_eq!(fs::read(paths.base_file(&plain)).expect("base"), other);
    assert!(
        state
            .read(|config| config.profile(&plain).expect("profile").has_base)
            .expect("read")
    );
    assert!(matches!(
        state.import_base(MAME_PROFILE, &file(BASE)),
        Err(CommandError::InvalidRequest { .. })
    ));
    assert!(!paths.profile_dir(MAME_PROFILE).exists());

    // The configuration survives a restart.
    let reloaded = ControllersState::new(Some(dir.path()));
    assert_eq!(
        reloaded.read(controllers_view).expect("read"),
        state.read(controllers_view).expect("read")
    );
}

#[test]
fn removes_a_base_file_and_keeps_everything_else() {
    let dir = tempfile::tempdir().expect("temp dir");
    let state = ControllersState::new(Some(dir.path()));
    let paths = ControllerPaths::new(dir.path());
    let id = state
        .create_profile("Six Buttons", Some(&file(BASE)))
        .expect("create");
    assert_eq!(
        state
            .read(controllers_view)
            .expect("read")
            .profiles
            .iter()
            .find(|profile| profile.id == id)
            .and_then(|profile| profile.base_file_name.clone())
            .as_deref(),
        Some("xarcade.cfg")
    );
    // What MAME saved in the profile's directory, and a file the user put
    // next to the base file: neither belongs to the removal.
    let context = paths.context_dir(INSTALLATION, &id);
    fs::create_dir_all(&context).expect("context");
    fs::write(context.join("default.cfg"), "saved by MAME").expect("default.cfg");
    let notes = paths.profile_dir(&id).join("notes.txt");
    fs::write(&notes, "mine").expect("notes");

    // A failed save puts the file back and changes nothing.
    let blocker = dir.path().join(format!("{CONTROLLERS_FILE_NAME}.tmp"));
    fs::create_dir(&blocker).expect("dir");
    let before = state.read(controllers_view).expect("read");
    let saved_before = fs::read(dir.path().join(CONTROLLERS_FILE_NAME)).expect("json");
    assert!(matches!(
        state.remove_base(&id),
        Err(CommandError::Settings { .. })
    ));
    assert_eq!(fs::read(paths.base_file(&id)).expect("base"), BASE);
    assert_eq!(state.read(controllers_view).expect("read"), before);
    assert_eq!(
        fs::read(dir.path().join(CONTROLLERS_FILE_NAME)).expect("json"),
        saved_before
    );
    fs::remove_dir(&blocker).expect("remove");

    state.remove_base(&id).expect("remove base");
    let profile = state
        .read(|config| config.profile(&id).cloned())
        .expect("read")
        .expect("the profile is kept");
    assert!(!profile.has_base && profile.base_file_name.is_none());
    assert!(!paths.base_file(&id).exists());
    assert_eq!(fs::read_to_string(&notes).expect("notes"), "mine");
    assert_eq!(
        fs::read_to_string(context.join("default.cfg")).expect("default.cfg"),
        "saved by MAME"
    );
    let names: Vec<_> = walk(dir.path())
        .iter()
        .filter_map(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .collect();
    assert!(
        !names.iter().any(|name| name.starts_with("base.cfg")),
        "{names:?}"
    );
    // The change is saved.
    let reloaded = ControllersState::new(Some(dir.path()));
    assert_eq!(
        reloaded.read(controllers_view).expect("read"),
        state.read(controllers_view).expect("read")
    );

    // Nothing to remove, the system profile, an unknown profile.
    assert!(matches!(
        state.remove_base(&id),
        Err(CommandError::InvalidRequest { .. })
    ));
    assert!(matches!(
        state.remove_base(MAME_PROFILE),
        Err(CommandError::InvalidRequest { .. })
    ));
    assert!(matches!(
        state.remove_base("missing"),
        Err(CommandError::NotFound { .. })
    ));

    // A base file deleted outside YMDB: the declaration is still removed.
    let other = state
        .create_profile("Gamepad", Some(&file(BASE)))
        .expect("create");
    fs::remove_file(paths.base_file(&other)).expect("delete");
    state.remove_base(&other).expect("remove base");
    assert!(
        state
            .read(|config| !config.profile(&other).expect("profile").has_base)
            .expect("read")
    );
}

#[test]
fn an_invalid_file_is_reported_and_never_replaced() {
    let dir = tempfile::tempdir().expect("temp dir");
    let file = dir.path().join(CONTROLLERS_FILE_NAME);
    fs::write(&file, "{ broken").expect("write");
    let state = ControllersState::new(Some(dir.path()));
    assert!(matches!(
        state.read(controllers_view),
        Err(CommandError::Settings { .. })
    ));
    assert!(matches!(
        state.update(|config| Ok(config.create_profile("X", false, |_| false)?)),
        Err(CommandError::Settings { .. })
    ));
    match state.snapshot() {
        Err(CommandError::Settings { message }) => {
            assert!(
                message.contains("Fix or remove the controllers file"),
                "{message}"
            );
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(fs::read_to_string(&file).expect("read"), "{ broken");
}

#[test]
fn resolution_reports_inheritance_without_side_effects() {
    let dir = tempfile::tempdir().expect("temp dir");
    let state = ControllersState::new(Some(dir.path()));
    let paths = ControllerPaths::new(dir.path());
    let id = state.create_profile("Fight Stick", None).expect("create");
    state
        .update(|config| {
            Ok(config.set_assignment(
                &AssignmentTarget {
                    level: AssignmentLevel::Software,
                    key: Some("nes:smb".to_owned()),
                },
                Assignment {
                    profile_id: Some(id.clone()),
                    players: None,
                },
            )?)
        })
        .expect("assign");
    let before: Vec<_> = walk(dir.path());
    let smb = SoftwareKey {
        list: "nes".to_owned(),
        name: "smb".to_owned(),
    };
    let resolution = state
        .read(|config| resolve_controls_with(config, &paths, INSTALLATION, "nes", Some(&smb)))
        .expect("read")
        .expect("resolve");
    assert_eq!(resolution.effective.profile_id, id);
    assert_eq!(
        resolution.effective.profile_source,
        ControlsSource::Software
    );
    assert_eq!(resolution.profile_name, "Fight Stick");
    assert_eq!(resolution.levels.len(), 4);
    assert_eq!(
        resolution.cfg_directory,
        Some(paths.context_dir(INSTALLATION, &id))
    );
    // The directory is not created by the resolution.
    assert_eq!(walk(dir.path()), before);

    let json = serde_json::to_value(&resolution).expect("serialize");
    assert_eq!(json["profileId"], id);
    assert_eq!(json["profileSource"], "software");
    assert_eq!(json["playersSource"], serde_json::Value::Null);
    assert_eq!(
        json["levels"][2],
        serde_json::json!({ "level": "software", "key": "nes:smb", "assignment": { "profileId": id } })
    );

    // The machine alone uses the default `mame` profile, without directory.
    let resolution = state
        .read(|config| resolve_controls_with(config, &paths, INSTALLATION, "nes", None))
        .expect("read")
        .expect("resolve");
    assert_eq!(resolution.effective.profile_id, MAME_PROFILE);
    assert_eq!(resolution.cfg_directory, None);
    assert!(matches!(
        state
            .read(|config| resolve_controls_with(config, &paths, INSTALLATION, "../nes", None))
            .expect("read"),
        Err(CommandError::InvalidRequest { .. })
    ));
}

#[test]
fn in_use_errors_list_the_references() {
    let error = CommandError::from(ControllersError::InUse {
        message: "in use".to_owned(),
        references: vec![AssignmentTarget {
            level: AssignmentLevel::Machine,
            key: Some("sf2".to_owned()),
        }],
    });
    assert_eq!(
        serde_json::to_value(error).expect("serialize"),
        serde_json::json!({
            "kind": "controllersInUse",
            "message": "in use",
            "references": [{ "level": "machine", "key": "sf2" }]
        })
    );
}

fn walk(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(dir).expect("read").flatten() {
        let path = entry.path();
        if path.is_dir() {
            entries.extend(walk(&path));
        }
        entries.push(path);
    }
    entries.sort();
    entries
}
