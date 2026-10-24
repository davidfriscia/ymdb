//! Tests of the controllers model, its persistence and the resolution of
//! the controls of a launch.

use std::fs;

use super::ControllerPaths;
use super::model::{
    Assignment, AssignmentLevel, AssignmentTarget, AssignmentUpdate, ControllerInput,
    ControllerKind, ControllersConfig, ControllersError, DeviceClass, FieldUpdate, MAME_PROFILE,
    ProfileKind,
};
use super::resolve::{ControlsSource, levels, resolve};
use super::store::{CONTROLLERS_FILE_NAME, ControllersStore, ControllersStoreError};

fn input(name: &str) -> ControllerInput {
    ControllerInput {
        name: name.to_owned(),
        kind: ControllerKind::ArcadeStick,
        device_class: DeviceClass::Joystick,
        device_id: None,
        provider: None,
        binding: None,
    }
}

fn target(level: AssignmentLevel, key: Option<&str>) -> AssignmentTarget {
    AssignmentTarget {
        level,
        key: key.map(str::to_owned),
    }
}

fn assignment(profile: Option<&str>, players: Option<&[&str]>) -> Assignment {
    Assignment {
        profile_id: profile.map(str::to_owned),
        players: players.map(|players| players.iter().map(|&id| Some(id.to_owned())).collect()),
    }
}

fn never(_: &str) -> bool {
    false
}

/// Two controllers, three user profiles (one with a base file).
fn config() -> ControllersConfig {
    let mut config = ControllersConfig::new();
    assert_eq!(
        config
            .create_controller(input("Arcade Stick P1"))
            .expect("p1"),
        "arcade-stick-p1"
    );
    assert_eq!(
        config
            .create_controller(input("Arcade Stick P2"))
            .expect("p2"),
        "arcade-stick-p2"
    );
    config
        .create_profile("Arcade Standard", false, never)
        .expect("standard");
    config
        .create_profile("Six Buttons", true, never)
        .expect("six");
    config
        .create_profile("Gamepad", false, never)
        .expect("gamepad");
    config.validate().expect("valid");
    config
}

#[test]
fn initial_state_uses_the_mame_profile_and_round_trips() {
    let config = ControllersConfig::new();
    config.validate().expect("valid");
    assert_eq!(config.profiles.len(), 1);
    let mame = config.profile(MAME_PROFILE).expect("mame");
    assert_eq!(mame.kind, ProfileKind::System);
    assert_eq!(mame.display_name(), "MAME configuration");
    assert!(!mame.is_managed());
    let json = serde_json::to_value(&config).expect("serialize");
    assert_eq!(
        json,
        serde_json::json!({
            "schemaVersion": 1,
            "controllers": [],
            "profiles": [{ "id": "mame", "kind": "system", "hasBase": false }],
            "assignments": {
                "default": { "profileId": "mame" },
                "machines": {},
                "software": {},
                "machineSoftware": {}
            }
        })
    );
    let parsed: ControllersConfig = serde_json::from_value(json).expect("parse");
    assert_eq!(parsed, config);
    // Every field has a default, unknown fields are rejected.
    let mut parsed: ControllersConfig = serde_json::from_str("{}").expect("defaults");
    parsed.ensure_system_profiles();
    assert_eq!(parsed, config);
    assert!(serde_json::from_str::<ControllersConfig>(r#"{ "unknown": 1 }"#).is_err());

    let resolved = resolve(&config, "sf2", None, None).expect("resolve");
    assert_eq!(resolved.profile_id, MAME_PROFILE);
    assert_eq!(resolved.profile_source, ControlsSource::Default);
    assert!(resolved.players.is_empty());
    assert_eq!(resolved.players_source, None);
}

#[test]
fn serializes_controllers_and_profiles() {
    let mut config = config();
    config
        .update_controller(
            "arcade-stick-p1",
            ControllerInput {
                device_id: Some(" VID_D209&PID_1601 ".to_owned()),
                provider: Some("dinput".to_owned()),
                ..input("Arcade Stick P1")
            },
        )
        .expect("update");
    let json = serde_json::to_value(&config).expect("serialize");
    assert_eq!(
        json["controllers"][0],
        serde_json::json!({
            "id": "arcade-stick-p1",
            "name": "Arcade Stick P1",
            "kind": "arcadeStick",
            "deviceClass": "joystick",
            "deviceId": "VID_D209&PID_1601",
            "provider": "dinput"
        })
    );
    assert_eq!(
        json["profiles"][2],
        serde_json::json!({ "id": "six-buttons", "kind": "user", "name": "Six Buttons", "hasBase": true })
    );
}

#[test]
fn rejects_invalid_configurations() {
    type Change = Box<dyn Fn(&mut ControllersConfig)>;
    let cases: Vec<(&str, Change)> = vec![
        ("future version", Box::new(|c| c.schema_version = 2)),
        (
            "bad controller id",
            Box::new(|c| c.controllers[0].id = "Bad Id".to_owned()),
        ),
        (
            "controller id with a separator",
            Box::new(|c| c.controllers[0].id = "../x".to_owned()),
        ),
        (
            "duplicate controller",
            Box::new(|c| c.controllers[1].id = c.controllers[0].id.clone()),
        ),
        (
            "empty controller name",
            Box::new(|c| c.controllers[0].name = " ".to_owned()),
        ),
        (
            "control character in a device id",
            Box::new(|c| c.controllers[0].device_id = Some("a\nb".to_owned())),
        ),
        (
            "bad provider",
            Box::new(|c| c.controllers[0].provider = Some("D Input".to_owned())),
        ),
        (
            "renamed system profile",
            Box::new(|c| c.profiles[0].name = Some("Mine".to_owned())),
        ),
        (
            "system profile with a base",
            Box::new(|c| c.profiles[0].has_base = true),
        ),
        (
            "user profile with a reserved id",
            Box::new(|c| c.profiles[1].id = MAME_PROFILE.to_owned()),
        ),
        (
            "user profile without a name",
            Box::new(|c| c.profiles[1].name = None),
        ),
        (
            "unknown system profile",
            Box::new(|c| c.profiles[1].kind = ProfileKind::System),
        ),
        (
            "missing profile reference",
            Box::new(|c| c.assignments.default.profile_id = Some("missing".to_owned())),
        ),
        (
            "missing controller reference",
            Box::new(|c| {
                c.assignments
                    .machines
                    .insert("sf2".to_owned(), assignment(None, Some(&["missing"])));
            }),
        ),
        (
            "controller assigned twice",
            Box::new(|c| {
                c.assignments.machines.insert(
                    "sf2".to_owned(),
                    assignment(None, Some(&["arcade-stick-p1", "arcade-stick-p1"])),
                );
            }),
        ),
        (
            "bad machine key",
            Box::new(|c| {
                c.assignments.machines.insert(
                    "Street Fighter".to_owned(),
                    assignment(Some("gamepad"), None),
                );
            }),
        ),
        (
            "bad software key",
            Box::new(|c| {
                c.assignments
                    .software
                    .insert("smb".to_owned(), assignment(Some("gamepad"), None));
            }),
        ),
        (
            "bad machine and software key",
            Box::new(|c| {
                c.assignments
                    .machine_software
                    .insert("nes:smb".to_owned(), assignment(Some("gamepad"), None));
            }),
        ),
    ];
    for (label, change) in cases {
        let mut config = config();
        change(&mut config);
        assert!(config.validate().is_err(), "{label}");
    }
}

#[test]
fn controller_crud() {
    let mut config = config();
    // Ids come from the name and stay unique.
    let id = config
        .create_controller(input("Arcade Stick P1"))
        .expect("create");
    assert_eq!(id, "arcade-stick-p1-2");
    let id = config
        .create_controller(input("  Xbox® Controller ✓ "))
        .expect("create");
    assert_eq!(id, "xbox-controller");
    assert!(matches!(
        config.create_controller(input("   ")),
        Err(ControllersError::Invalid(_))
    ));

    // Updating never changes the id.
    config
        .update_controller(
            &id,
            ControllerInput {
                kind: ControllerKind::Gamepad,
                ..input("Xbox Pad")
            },
        )
        .expect("update");
    let controller = config.controller(&id).expect("controller");
    assert_eq!(
        (controller.name.as_str(), controller.kind),
        ("Xbox Pad", ControllerKind::Gamepad)
    );
    assert!(matches!(
        config.update_controller("missing", input("X")),
        Err(ControllersError::NotFound(_))
    ));

    config.delete_controller(&id).expect("delete");
    assert!(config.controller(&id).is_none());
    assert!(matches!(
        config.delete_controller(&id),
        Err(ControllersError::NotFound(_))
    ));
    config.validate().expect("valid");
}

#[test]
fn profile_crud_protects_the_system_profile() {
    let mut config = config();
    // A former profile's files keep their id from being reused.
    let id = config
        .create_profile("Arcade Standard", false, |id| id == "arcade-standard-2")
        .expect("create");
    assert_eq!(id, "arcade-standard-3");
    config.rename_profile(&id, " Fight Stick ").expect("rename");
    assert_eq!(
        config.profile(&id).expect("profile").display_name(),
        "Fight Stick"
    );
    assert_eq!(config.profile(&id).expect("profile").id, id);
    assert!(config.rename_profile(&id, "").is_err());

    for change in [
        config.clone().rename_profile(MAME_PROFILE, "Mine"),
        config
            .clone()
            .set_profile_base_file(MAME_PROFILE, Some(None)),
        config.clone().delete_profile(MAME_PROFILE),
    ] {
        assert!(
            matches!(change, Err(ControllersError::Invalid(_))),
            "{change:?}"
        );
    }
    assert!(matches!(
        config.delete_profile("missing"),
        Err(ControllersError::NotFound(_))
    ));
    config.delete_profile(&id).expect("delete");
    assert!(config.profile(&id).is_none());
    config.validate().expect("valid");
}

#[test]
fn referenced_entities_cannot_be_removed() {
    let mut config = config();
    config
        .set_assignment(
            &target(AssignmentLevel::Machine, Some("sf2")),
            assignment(
                Some("six-buttons"),
                Some(&["arcade-stick-p1", "arcade-stick-p2"]),
            ),
        )
        .expect("machine");
    config
        .set_assignment(
            &target(AssignmentLevel::MachineSoftware, Some("nes/nes:smb")),
            assignment(Some("six-buttons"), None),
        )
        .expect("machine software");

    match config.delete_profile("six-buttons") {
        Err(ControllersError::InUse { references, .. }) => assert_eq!(
            references,
            [
                target(AssignmentLevel::Machine, Some("sf2")),
                target(AssignmentLevel::MachineSoftware, Some("nes/nes:smb")),
            ]
        ),
        other => panic!("{other:?}"),
    }
    match config.delete_controller("arcade-stick-p2") {
        Err(ControllersError::InUse { references, .. }) => {
            assert_eq!(references, [target(AssignmentLevel::Machine, Some("sf2"))]);
        }
        other => panic!("{other:?}"),
    }
    // The default assignment references the system profile, which is never removed.
    assert!(config.delete_profile(MAME_PROFILE).is_err());

    // Once the assignments no longer use them, they can be removed.
    config
        .reset_assignment(&target(AssignmentLevel::Machine, Some("sf2")))
        .expect("reset");
    config
        .delete_controller("arcade-stick-p2")
        .expect("delete controller");
    assert!(config.delete_profile("six-buttons").is_err());
}

#[test]
fn assignment_crud_validates_targets_and_references() {
    let mut config = config();
    let machine = target(AssignmentLevel::Machine, Some("sf2"));
    config
        .set_assignment(&machine, assignment(Some("gamepad"), None))
        .expect("set");
    assert_eq!(
        config.assignment(&machine),
        Some(&assignment(Some("gamepad"), None))
    );

    let invalid = [
        (
            target(AssignmentLevel::Default, Some("sf2")),
            assignment(Some("gamepad"), None),
        ),
        (
            target(AssignmentLevel::Machine, None),
            assignment(Some("gamepad"), None),
        ),
        (
            target(AssignmentLevel::Machine, Some("../sf2")),
            assignment(Some("gamepad"), None),
        ),
        (
            target(AssignmentLevel::Software, Some("nes")),
            assignment(Some("gamepad"), None),
        ),
        (
            target(AssignmentLevel::Software, Some("nes:smb:cart")),
            assignment(Some("gamepad"), None),
        ),
        (
            target(AssignmentLevel::MachineSoftware, Some("nes:smb")),
            assignment(Some("gamepad"), None),
        ),
        (machine.clone(), assignment(Some("missing"), None)),
        (machine.clone(), assignment(None, Some(&["missing"]))),
        (
            machine.clone(),
            assignment(None, Some(&["arcade-stick-p1"; 11])),
        ),
    ];
    for (target, assignment) in invalid {
        assert!(
            matches!(
                config.clone().set_assignment(&target, assignment.clone()),
                Err(ControllersError::Invalid(_))
            ),
            "{target:?} {assignment:?}"
        );
    }

    // An empty assignment removes a keyed level; the default stays, possibly empty.
    config
        .set_assignment(&machine, Assignment::default())
        .expect("clear");
    assert_eq!(config.assignment(&machine), None);
    let default = target(AssignmentLevel::Default, None);
    config
        .set_assignment(&default, Assignment::default())
        .expect("empty default");
    assert_eq!(config.assignment(&default), Some(&Assignment::default()));
    assert_eq!(
        resolve(&config, "sf2", None, None)
            .expect("resolve")
            .profile_id,
        MAME_PROFILE
    );
    config.reset_assignment(&default).expect("reset default");
    assert_eq!(
        config.assignment(&default),
        Some(&assignment(Some(MAME_PROFILE), None))
    );
}

#[test]
fn players_may_leave_gaps_and_are_stored_normalized() {
    let mut config = config();
    let target = target(AssignmentLevel::Machine, Some("sf2"));
    let gap = |players: &[Option<&str>]| AssignmentUpdate {
        players: FieldUpdate::Set {
            value: players.iter().map(|id| id.map(str::to_owned)).collect(),
        },
        ..AssignmentUpdate::default()
    };
    // Player 1 without controller, Player 2 assigned; trailing empty players dropped.
    config
        .update_assignment(&target, gap(&[None, Some("arcade-stick-p2"), None, None]))
        .expect("update");
    assert_eq!(
        config.assignment(&target).expect("stored").players,
        Some(vec![None, Some("arcade-stick-p2".to_owned())])
    );
    // Four players and more are allowed, up to MAME's ten.
    config
        .update_assignment(
            &target,
            gap(&[None, None, Some("arcade-stick-p1"), Some("arcade-stick-p2")]),
        )
        .expect("four players");
    let eleven: Vec<Option<&str>> = std::iter::repeat_n(None, 10)
        .chain([Some("arcade-stick-p1")])
        .collect();
    assert!(config.update_assignment(&target, gap(&eleven)).is_err());
    // The same controller for two players is refused.
    assert!(
        config
            .update_assignment(
                &target,
                gap(&[Some("arcade-stick-p1"), Some("arcade-stick-p1")])
            )
            .unwrap_err()
            .to_string()
            .contains("two players")
    );
    // The JSON keeps the gaps.
    let json = serde_json::to_value(config.assignment(&target).expect("stored")).expect("json");
    assert_eq!(
        json["players"],
        serde_json::json!([null, null, "arcade-stick-p1", "arcade-stick-p2"])
    );
    // Old files (players without gaps) still load.
    let old: Assignment =
        serde_json::from_value(serde_json::json!({ "players": ["arcade-stick-p1"] })).expect("old");
    assert_eq!(old.players, Some(vec![Some("arcade-stick-p1".to_owned())]));
    // A controller used with a gap cannot be deleted.
    assert!(matches!(
        config.delete_controller("arcade-stick-p2"),
        Err(ControllersError::InUse { .. })
    ));
}

#[test]
fn updates_change_one_field_and_keep_the_other() {
    let mut config = config();
    let machine = target(AssignmentLevel::Machine, Some("sf2"));
    let p1 = ["arcade-stick-p1"];
    config
        .set_assignment(&machine, assignment(None, Some(&p1)))
        .expect("players");

    // Setting the profile keeps the players.
    let set_profile: AssignmentUpdate = serde_json::from_value(
        serde_json::json!({ "profileId": { "kind": "set", "value": "gamepad" } }),
    )
    .expect("deserialize");
    assert_eq!(set_profile.players, FieldUpdate::Keep);
    config
        .update_assignment(&machine, set_profile)
        .expect("update");
    assert_eq!(
        config.assignment(&machine),
        Some(&assignment(Some("gamepad"), Some(&p1)))
    );

    // Inheriting the profile again keeps the assignment with its players.
    let inherit_profile = AssignmentUpdate {
        profile_id: FieldUpdate::Inherit,
        players: FieldUpdate::Keep,
    };
    config
        .update_assignment(&machine, inherit_profile.clone())
        .expect("inherit");
    assert_eq!(
        config.assignment(&machine),
        Some(&assignment(None, Some(&p1)))
    );

    // Once no field is left, the level is removed.
    config
        .update_assignment(
            &machine,
            AssignmentUpdate {
                profile_id: FieldUpdate::Keep,
                players: FieldUpdate::Inherit,
            },
        )
        .expect("inherit players");
    assert_eq!(config.assignment(&machine), None);

    // A new level, the default (never removed), and invalid changes.
    let software = target(AssignmentLevel::MachineSoftware, Some("nes/nes:smb"));
    config
        .update_assignment(
            &software,
            AssignmentUpdate {
                profile_id: FieldUpdate::Set {
                    value: "six-buttons".to_owned(),
                },
                players: FieldUpdate::Keep,
            },
        )
        .expect("create");
    assert_eq!(
        config.assignment(&software),
        Some(&assignment(Some("six-buttons"), None))
    );
    let default = target(AssignmentLevel::Default, None);
    config
        .update_assignment(&default, inherit_profile)
        .expect("default");
    assert_eq!(config.assignment(&default), Some(&Assignment::default()));
    let before = config.clone();
    for (target, update) in [
        (
            machine.clone(),
            AssignmentUpdate {
                profile_id: FieldUpdate::Set {
                    value: "missing".to_owned(),
                },
                players: FieldUpdate::Keep,
            },
        ),
        (
            target(AssignmentLevel::Machine, Some("../sf2")),
            AssignmentUpdate::default(),
        ),
    ] {
        assert!(matches!(
            config.update_assignment(&target, update),
            Err(ControllersError::Invalid(_))
        ));
    }
    assert_eq!(config, before);
    assert!(
        serde_json::from_value::<AssignmentUpdate>(
            serde_json::json!({ "profileId": { "kind": "clear" } })
        )
        .is_err()
    );
}

#[test]
fn records_the_name_of_the_imported_base_file() {
    let mut config = config();
    config
        .set_profile_base_file("gamepad", Some(Some("xarcade.cfg".to_owned())))
        .expect("base");
    let profile = config.profile("gamepad").expect("profile");
    assert!(profile.has_base);
    assert_eq!(profile.base_file_name.as_deref(), Some("xarcade.cfg"));
    let json = serde_json::to_value(profile).expect("serialize");
    assert_eq!(json["baseFileName"], "xarcade.cfg");

    // Removing the base forgets the name; profiles without a name still load.
    config
        .set_profile_base_file("gamepad", None)
        .expect("remove");
    let profile = config.profile("gamepad").expect("profile");
    assert!(!profile.has_base && profile.base_file_name.is_none());
    assert!(
        config
            .profile("six-buttons")
            .is_some_and(|profile| profile.has_base && profile.base_file_name.is_none())
    );
    for name in ["", "a\nb"] {
        assert!(matches!(
            config
                .clone()
                .set_profile_base_file("gamepad", Some(Some(name.to_owned()))),
            Err(ControllersError::Invalid(_))
        ));
    }
    let mut invalid = config.clone();
    invalid.profiles[1].base_file_name = Some("orphan.cfg".to_owned());
    invalid.profiles[1].has_base = false;
    assert!(invalid.validate().is_err());
}

#[test]
fn resolves_by_precedence_field_by_field() {
    let mut config = config();
    let set = |config: &mut ControllersConfig, level, key: Option<&str>, value| {
        config
            .set_assignment(&target(level, key), value)
            .expect("set");
    };
    let summary = |config: &ControllersConfig,
                   machine: &str,
                   software: Option<(&str, &str)>,
                   request: Option<&Assignment>| {
        let resolved = resolve(config, machine, software, request).expect("resolve");
        (
            resolved.profile_id,
            resolved.profile_source,
            resolved.players,
            resolved.players_source,
        )
    };
    let players = |ids: &[&str]| {
        ids.iter()
            .map(|&id| Some(id.to_owned()))
            .collect::<Vec<_>>()
    };

    // 1. Default.
    set(
        &mut config,
        AssignmentLevel::Default,
        None,
        assignment(Some("arcade-standard"), Some(&["arcade-stick-p1"])),
    );
    assert_eq!(
        summary(&config, "sf2", None, None),
        (
            "arcade-standard".to_owned(),
            ControlsSource::Default,
            players(&["arcade-stick-p1"]),
            Some(ControlsSource::Default)
        )
    );
    // 2. Machine, profile only: the controllers are still inherited.
    set(
        &mut config,
        AssignmentLevel::Machine,
        Some("nes"),
        assignment(Some("gamepad"), None),
    );
    assert_eq!(
        summary(&config, "nes", None, None),
        (
            "gamepad".to_owned(),
            ControlsSource::Machine,
            players(&["arcade-stick-p1"]),
            Some(ControlsSource::Default)
        )
    );
    // 3. Software, controllers only: the profile is still the machine's.
    set(
        &mut config,
        AssignmentLevel::Software,
        Some("nes:smb"),
        assignment(None, Some(&["arcade-stick-p2", "arcade-stick-p1"])),
    );
    let smb = Some(("nes", "smb"));
    assert_eq!(
        summary(&config, "nes", smb, None),
        (
            "gamepad".to_owned(),
            ControlsSource::Machine,
            players(&["arcade-stick-p2", "arcade-stick-p1"]),
            Some(ControlsSource::Software)
        )
    );
    // The software level applies on any machine; the machine level does not leak.
    assert_eq!(
        summary(&config, "nespal", smb, None).1,
        ControlsSource::Default
    );
    assert_eq!(
        summary(&config, "nespal", smb, None).3,
        Some(ControlsSource::Software)
    );
    // 4. Machine + software beats the software (and the machine).
    set(
        &mut config,
        AssignmentLevel::Software,
        Some("nes:smb"),
        assignment(Some("arcade-standard"), Some(&["arcade-stick-p2"])),
    );
    set(
        &mut config,
        AssignmentLevel::MachineSoftware,
        Some("nes/nes:smb"),
        assignment(Some("six-buttons"), None),
    );
    assert_eq!(
        summary(&config, "nes", smb, None),
        (
            "six-buttons".to_owned(),
            ControlsSource::MachineSoftware,
            players(&["arcade-stick-p2"]),
            Some(ControlsSource::Software)
        )
    );
    // Machine + software only applies to that machine.
    assert_eq!(summary(&config, "nespal", smb, None).0, "arcade-standard");
    // Without the software, only the machine level applies.
    assert_eq!(summary(&config, "nes", None, None).0, "gamepad");

    // 5. Temporary override, field by field, never saved.
    let before = config.clone();
    let request = assignment(Some(MAME_PROFILE), None);
    assert_eq!(
        summary(&config, "nes", smb, Some(&request)),
        (
            MAME_PROFILE.to_owned(),
            ControlsSource::Request,
            players(&["arcade-stick-p2"]),
            Some(ControlsSource::Software)
        )
    );
    let request = assignment(None, Some(&[]));
    assert_eq!(
        summary(&config, "nes", smb, Some(&request)),
        (
            "six-buttons".to_owned(),
            ControlsSource::MachineSoftware,
            Vec::new(),
            Some(ControlsSource::Request)
        )
    );
    assert_eq!(config, before);
    for request in [
        assignment(Some("missing"), None),
        assignment(None, Some(&["missing"])),
    ] {
        assert!(matches!(
            resolve(&config, "nes", smb, Some(&request)),
            Err(ControllersError::Invalid(_))
        ));
    }

    // Reset of an override: the weaker levels apply again.
    config
        .reset_assignment(&target(
            AssignmentLevel::MachineSoftware,
            Some("nes/nes:smb"),
        ))
        .expect("reset");
    assert_eq!(
        summary(&config, "nes", smb, None).1,
        ControlsSource::Software
    );

    // The inheritance seen by the interface.
    let levels = levels(&config, "nes", smb);
    let stored: Vec<_> = levels
        .iter()
        .map(|level| (level.target.level, level.assignment.is_some()))
        .collect();
    assert_eq!(
        stored,
        [
            (AssignmentLevel::Default, true),
            (AssignmentLevel::Machine, true),
            (AssignmentLevel::Software, true),
            (AssignmentLevel::MachineSoftware, false),
        ]
    );
}

#[test]
fn a_reference_to_a_deleted_profile_is_reported() {
    // Stored assignments are validated, so only an unvalidated configuration
    // can reference a missing profile: the launch is refused, never silently
    // given another profile.
    let mut config = config();
    config
        .assignments
        .machines
        .insert("sf2".to_owned(), assignment(Some("deleted"), None));
    assert!(matches!(
        resolve(&config, "sf2", None, None),
        Err(ControllersError::NotFound(_))
    ));
    assert!(config.validate().is_err());
}

#[test]
fn keeps_assignments_of_names_missing_from_any_catalog() {
    let dir = tempfile::tempdir().expect("temp dir");
    let store = ControllersStore::in_dir(dir.path());
    let mut config = config();
    for (level, key) in [
        (AssignmentLevel::Machine, "notamachine"),
        (AssignmentLevel::Software, "nolist:nosoftware"),
        (
            AssignmentLevel::MachineSoftware,
            "nomachine/nolist:nosoftware",
        ),
    ] {
        config
            .set_assignment(&target(level, Some(key)), assignment(Some("gamepad"), None))
            .expect("set");
    }
    store.save(&config).expect("save");
    assert_eq!(store.load().expect("load"), config);
}

#[test]
fn store_loads_saves_and_never_replaces_invalid_files() {
    let dir = tempfile::tempdir().expect("temp dir");
    let config_dir = dir.path().join("config dir ✓");
    let store = ControllersStore::in_dir(&config_dir);
    // A missing file is the initial state, and nothing is created.
    assert_eq!(store.load().expect("load"), ControllersConfig::new());
    assert!(!config_dir.exists());

    let config = config();
    store.save(&config).expect("save");
    assert_eq!(store.load().expect("load"), config);
    let entries: Vec<_> = fs::read_dir(&config_dir)
        .expect("read dir")
        .map(|entry| entry.expect("entry").file_name())
        .collect();
    assert_eq!(entries, [CONTROLLERS_FILE_NAME]);

    // An invalid configuration is not saved.
    let mut invalid = config.clone();
    invalid.assignments.default.profile_id = Some("missing".to_owned());
    assert!(matches!(
        store.save(&invalid),
        Err(ControllersStoreError::Invalid { .. })
    ));
    assert_eq!(store.load().expect("load"), config);

    // Corrupted or invalid files are reported and left untouched.
    let file = config_dir.join(CONTROLLERS_FILE_NAME);
    for content in [
        "{ not json",
        r#"{ "schemaVersion": 2 }"#,
        r#"{ "controllers": [{ "id": "x" }] }"#,
        r#"{ "assignments": { "default": { "profileId": "missing" } } }"#,
    ] {
        fs::write(&file, content).expect("write");
        assert!(
            matches!(store.load(), Err(ControllersStoreError::Invalid { .. })),
            "{content}"
        );
        assert_eq!(fs::read_to_string(&file).expect("read"), content);
    }
    // A file without the system profile gets it back when loaded.
    fs::write(&file, r#"{ "schemaVersion": 1, "profiles": [] }"#).expect("write");
    assert_eq!(store.load().expect("load"), ControllersConfig::new());
}

#[test]
fn write_errors_keep_the_previous_file() {
    let dir = tempfile::tempdir().expect("temp dir");
    let store = ControllersStore::in_dir(dir.path());
    let config = config();
    store.save(&config).expect("save");
    // A directory where the temporary file would go makes the write fail.
    let temp = dir.path().join(format!("{CONTROLLERS_FILE_NAME}.tmp"));
    fs::create_dir(&temp).expect("dir");
    let mut changed = config.clone();
    changed
        .create_profile("Other", false, never)
        .expect("create");
    assert!(matches!(
        store.save(&changed),
        Err(ControllersStoreError::Write { .. })
    ));
    assert_eq!(store.load().expect("load"), config);

    // The base file of a profile cannot be written either.
    let blocked = ControllersStore::in_dir(&dir.path().join("file"));
    fs::write(dir.path().join("file"), "not a directory").expect("write");
    assert!(blocked.write_base("gamepad", b"<mameconfig/>").is_err());
}

#[test]
fn paths_depend_on_ids_only() {
    let dir = tempfile::tempdir().expect("temp dir");
    let paths = ControllerPaths::new(dir.path());
    let installation = "0123456789abcdef0123456789abcdef";
    let other = "fedcba9876543210fedcba9876543210";
    assert_eq!(
        paths.context_dir(installation, "six-buttons"),
        dir.path()
            .join("installations")
            .join(installation)
            .join("controller-profiles")
            .join("six-buttons")
            .join("cfg")
    );
    assert_ne!(
        paths.context_dir(installation, "six-buttons"),
        paths.context_dir(other, "six-buttons")
    );
    assert_ne!(
        paths.context_dir(installation, "six-buttons"),
        paths.context_dir(installation, "gamepad")
    );
    assert_eq!(
        paths.base_file("six-buttons"),
        dir.path()
            .join("controller-profiles")
            .join("six-buttons")
            .join("base.cfg")
    );
    // Computing paths creates nothing; files of former profiles are detected.
    assert!(fs::read_dir(dir.path()).expect("read").next().is_none());
    assert!(!paths.is_used("six-buttons"));
    fs::create_dir_all(paths.context_dir(other, "six-buttons")).expect("dir");
    assert!(paths.is_used("six-buttons"));
    fs::create_dir_all(paths.profile_dir("gamepad")).expect("dir");
    assert!(paths.is_used("gamepad"));
    assert!(!paths.is_used("arcade-standard"));
}
