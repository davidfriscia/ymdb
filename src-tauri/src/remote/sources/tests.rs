use super::*;
use crate::remote::connections::{
    ConnectionInput, HostKeyVerification, ProviderInput, SftpSettings,
};

const CONNECTION_A: &str = "0123456789abcdef0123456789abcdef";
const CONNECTION_B: &str = "fedcba9876543210fedcba9876543210";

fn absolute(name: &str) -> PathBuf {
    if cfg!(windows) {
        PathBuf::from(format!("E:\\YMDB downloads ✓\\{name}"))
    } else {
        PathBuf::from(format!("/srv/YMDB downloads ✓/{name}"))
    }
}

fn source(connection: &str, path: &str, organization: CollectionOrganization) -> CategorySource {
    CategorySource {
        enabled: true,
        connection_id: Some(connection.to_owned()),
        remote_base_path: Some(path.to_owned()),
        organization: Some(organization),
        destination: Some(LocalDestination::MameDirectory {
            directory: "roms".to_owned(),
        }),
    }
}

/// Four ROM/CHD categories from two connections and one sample source.
fn config() -> RemoteSourcesConfig {
    let mut config = RemoteSourcesConfig::default();
    config.categories.insert(
        SourceCategory::MachineRoms,
        source(
            CONNECTION_A,
            "/volume1/mame/0280/roms",
            CollectionOrganization::Merged,
        ),
    );
    config.categories.insert(
        SourceCategory::MachineChds,
        CategorySource {
            destination: Some(LocalDestination::Custom {
                directory: absolute("chds"),
            }),
            ..source(
                CONNECTION_A,
                "/volume1/mame/0280/chds",
                CollectionOrganization::NonMerged,
            )
        },
    );
    config.categories.insert(
        SourceCategory::SoftwareListRoms,
        source(CONNECTION_B, "/sl-roms", CollectionOrganization::Split),
    );
    config.categories.insert(
        SourceCategory::Samples,
        CategorySource {
            destination: Some(LocalDestination::Custom {
                directory: absolute("samples"),
            }),
            ..source(
                CONNECTION_B,
                "/samples",
                CollectionOrganization::ZipArchives,
            )
        },
    );
    config
}

fn connections(ids: &[&str]) -> ConnectionsConfig {
    let mut connections = ConnectionsConfig::default();
    for (index, id) in ids.iter().enumerate() {
        connections
            .upsert(
                ConnectionInput {
                    id: None,
                    name: format!("NAS {index}"),
                    provider: ProviderInput::Sftp(SftpSettings {
                        host: "nas.local".to_owned(),
                        port: 22,
                        username: "mame".to_owned(),
                        private_key_path: absolute("id_ed25519"),
                        host_key_verification: HostKeyVerification::Ymdb,
                    }),
                },
                (*id).to_owned(),
            )
            .expect("connection");
    }
    connections
}

#[test]
fn accepts_partial_configurations_and_disabled_drafts() {
    RemoteSourcesConfig::default().validate().expect("empty");
    let config = config();
    config.validate().expect("valid");
    assert!(
        !config
            .categories
            .contains_key(&SourceCategory::SoftwareListChds)
    );

    // A disabled category may be incomplete; its values are still checked.
    let mut draft = RemoteSourcesConfig::default();
    draft.categories.insert(
        SourceCategory::SoftwareListChds,
        CategorySource {
            enabled: false,
            connection_id: Some(CONNECTION_A.to_owned()),
            ..CategorySource::default()
        },
    );
    draft.validate().expect("disabled draft");
    draft
        .categories
        .get_mut(&SourceCategory::SoftwareListChds)
        .expect("draft")
        .enabled = true;
    let error = draft.validate().expect_err("incomplete");
    assert!(
        error.starts_with("Software List CHDs: an enabled category needs"),
        "{error}"
    );
    assert!(error.contains("a remote base path"));
}

#[test]
fn validates_categories() {
    type Change = Box<dyn Fn(&mut RemoteSourcesConfig)>;
    let edit = |category: SourceCategory, change: fn(&mut CategorySource)| -> Change {
        Box::new(move |config: &mut RemoteSourcesConfig| {
            change(config.categories.get_mut(&category).expect("category"));
        })
    };
    let cases: Vec<(&str, Change)> = vec![
        (
            "relative remote path",
            edit(SourceCategory::MachineRoms, |s| {
                s.remote_base_path = Some("mame/roms".to_owned());
            }),
        ),
        (
            "traversal",
            edit(SourceCategory::MachineRoms, |s| {
                s.remote_base_path = Some("/volume1/../etc".to_owned());
            }),
        ),
        (
            "bad connection id",
            edit(SourceCategory::MachineRoms, |s| {
                s.connection_id = Some("nas".to_owned());
            }),
        ),
        (
            "zip organization for ROMs",
            edit(SourceCategory::MachineRoms, |s| {
                s.organization = Some(CollectionOrganization::ZipArchives);
            }),
        ),
        (
            "merged samples",
            edit(SourceCategory::Samples, |s| {
                s.organization = Some(CollectionOrganization::Merged);
            }),
        ),
        (
            "relative custom directory",
            edit(SourceCategory::MachineChds, |s| {
                s.destination = Some(LocalDestination::Custom {
                    directory: PathBuf::from("chds"),
                });
            }),
        ),
        (
            "directory with a separator",
            edit(SourceCategory::MachineChds, |s| {
                s.destination = Some(LocalDestination::Custom {
                    directory: absolute("a;b"),
                });
            }),
        ),
        (
            "empty MAME directory",
            edit(SourceCategory::MachineRoms, |s| {
                s.destination = Some(LocalDestination::MameDirectory {
                    directory: " ".to_owned(),
                });
            }),
        ),
        (
            "MAME directory as option",
            edit(SourceCategory::MachineRoms, |s| {
                s.destination = Some(LocalDestination::MameDirectory {
                    directory: "-rompath".to_owned(),
                });
            }),
        ),
        ("future version", Box::new(|c| c.schema_version = 2)),
    ];
    for (label, change) in cases {
        let mut config = config();
        change(&mut config);
        assert!(config.validate().is_err(), "{label}");
    }
}

#[test]
fn refuses_orphan_references_and_finds_users_of_a_connection() {
    let config = config();
    config
        .check_references(&connections(&[CONNECTION_A, CONNECTION_B]))
        .expect("both exist");
    let error = config
        .check_references(&connections(&[CONNECTION_A]))
        .expect_err("orphan");
    assert!(error.starts_with("Software List ROMs"), "{error}");

    // One connection, several categories.
    assert_eq!(
        config.categories_using(CONNECTION_A),
        [SourceCategory::MachineRoms, SourceCategory::MachineChds]
    );
    assert_eq!(
        config.categories_using(CONNECTION_B),
        [SourceCategory::SoftwareListRoms, SourceCategory::Samples]
    );
    assert!(
        config
            .categories_using("00000000000000000000000000000000")
            .is_empty()
    );
}

#[test]
fn lists_destinations_per_search_path() {
    let mut config = config();
    // The same local directory for ROMs of two remote sources is listed once.
    config
        .categories
        .get_mut(&SourceCategory::SoftwareListRoms)
        .expect("category")
        .destination = Some(LocalDestination::MameDirectory {
        directory: "roms".to_owned(),
    });
    assert_eq!(
        config.destinations(SearchPath::Rom),
        [
            "roms".to_owned(),
            absolute("chds").to_string_lossy().into_owned()
        ]
    );
    assert_eq!(
        config.destinations(SearchPath::Sample),
        [absolute("samples").to_string_lossy().into_owned()]
    );
    // Disabled categories do not count.
    config
        .categories
        .get_mut(&SourceCategory::MachineChds)
        .expect("category")
        .enabled = false;
    assert_eq!(config.destinations(SearchPath::Rom), ["roms"]);
}

#[test]
fn persists_normalized_configurations() {
    let dir = tempfile::tempdir().expect("temp dir");
    let store = RemoteSourcesStore::in_dir(&dir.path().join("installation ✓"));
    assert!(!store.exists());
    assert_eq!(
        store.load().expect("missing"),
        RemoteSourcesConfig::default()
    );

    let mut config = config();
    config
        .categories
        .get_mut(&SourceCategory::MachineRoms)
        .expect("category")
        .remote_base_path = Some("//volume1/mame/0280/roms/".to_owned());
    config
        .categories
        .insert(SourceCategory::SoftwareListChds, CategorySource::default());
    let saved = store.save(config).expect("save");
    assert_eq!(
        saved.categories[&SourceCategory::MachineRoms]
            .remote_base_path
            .as_deref(),
        Some("/volume1/mame/0280/roms")
    );
    assert!(
        !saved
            .categories
            .contains_key(&SourceCategory::SoftwareListChds)
    );
    assert_eq!(store.load().expect("reload"), saved);

    let json: serde_json::Value = serde_json::from_slice(
        &fs::read(dir.path().join("installation ✓").join(SOURCES_FILE_NAME)).expect("read"),
    )
    .expect("json");
    assert_eq!(json["schemaVersion"], 1);
    let roms = &json["categories"]["machineRoms"];
    assert_eq!(roms["enabled"], true);
    assert_eq!(roms["connectionId"], CONNECTION_A);
    assert_eq!(roms["organization"], "merged");
    assert_eq!(
        roms["destination"],
        serde_json::json!({ "mode": "mameDirectory", "directory": "roms" })
    );
    assert_eq!(
        json["categories"]["machineChds"]["organization"],
        "nonMerged"
    );
    assert_eq!(
        json["categories"]["samples"]["destination"]["mode"],
        "custom"
    );

    store.delete().expect("delete");
    assert!(!store.exists());
    store.delete().expect("deleting twice is fine");
}

#[test]
fn rejects_invalid_files_and_values() {
    let dir = tempfile::tempdir().expect("temp dir");
    let store = RemoteSourcesStore::in_dir(dir.path());
    for content in [
        "{ nope",
        r#"{ "schemaVersion": 7 }"#,
        r#"{ "categories": { "artwork": {} } }"#,
        r#"{ "categories": { "samples": { "organization": "merged" } } }"#,
        r#"{ "categories": { "machineRoms": { "password": "x" } } }"#,
    ] {
        fs::write(dir.path().join(SOURCES_FILE_NAME), content).expect("write");
        assert!(
            matches!(store.load(), Err(SourcesStoreError::Invalid { .. })),
            "{content}"
        );
    }
    let mut config = config();
    config.schema_version = 9;
    assert!(matches!(
        store.save(config),
        Err(SourcesStoreError::Rejected(_))
    ));
}
