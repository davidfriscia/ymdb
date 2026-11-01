//! Detail tests on catalogs imported from the controlled fixtures.

use std::sync::atomic::AtomicBool;

use rusqlite::Connection;
use tempfile::TempDir;

use super::machine::{MachineDetail, machine_detail};
use super::software::{SoftwareDetail, SoftwareListDetail, software_detail, software_list_detail};
use super::{NameValue, RecordLink};
use crate::catalog::import::{ImportProgress, import_from_reader, import_software_from_reader};
use crate::catalog::search::machines::search_machines;
use crate::catalog::search::request::{
    MachineFilters, MachineSearchRequest, SoftwareFilters, SoftwareSearchRequest,
};
use crate::catalog::search::software::search_software;
use crate::catalog::store::{CatalogSource, CatalogStore};

const MACHINES: &str = include_str!("../testdata/listxml-sample.xml");
const SOFTWARE: &str = include_str!("../testdata/listsoftware-sample.xml");

fn source() -> CatalogSource {
    CatalogSource {
        mame_release: "0.280".to_owned(),
        mame_revision: "mame0280".to_owned(),
        executable_path: "/opt/mame/mame".into(),
        executable_size: Some(1),
        executable_modified_at: Some(1),
    }
}

struct Catalog {
    _dir: TempDir,
    store: CatalogStore,
}

impl Catalog {
    fn new(with_software: bool) -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = CatalogStore::in_dir(dir.path());
        let cancel = AtomicBool::new(false);
        import_from_reader(
            MACHINES.as_bytes(),
            &store,
            source(),
            &cancel,
            &ImportProgress::default(),
            || Ok(()),
        )
        .expect("machine import");
        if with_software {
            import_software_from_reader(
                SOFTWARE.as_bytes(),
                &store,
                source(),
                &cancel,
                &ImportProgress::default(),
                || Ok(()),
            )
            .expect("software import");
        }
        Self { _dir: dir, store }
    }

    fn read<T>(&self, operation: impl FnOnce(&Connection) -> T) -> T {
        self.store
            .with_reader(|connection| Ok(operation(connection)))
            .expect("store")
            .expect("catalog")
    }

    fn machine(&self, name: &str) -> Option<MachineDetail> {
        self.read(|connection| machine_detail(connection, name).expect("query"))
    }

    fn software(&self, list: &str, name: &str) -> Option<SoftwareDetail> {
        self.read(|connection| software_detail(connection, list, name).expect("query"))
    }

    fn list(&self, name: &str) -> Option<SoftwareListDetail> {
        self.read(|connection| software_list_detail(connection, name).expect("query"))
    }
}

fn link(name: &str, description: Option<&str>) -> RecordLink {
    RecordLink {
        name: name.to_owned(),
        description: description.map(str::to_owned),
    }
}

fn name_value(name: &str, value: &str) -> NameValue {
    NameValue {
        name: name.to_owned(),
        value: Some(value.to_owned()),
    }
}

#[test]
fn machine_detail_contains_imported_sections() {
    let catalog = Catalog::new(false);
    let neogeo = catalog.machine("neogeo").expect("neogeo");

    assert_eq!(neogeo.description, "Neo-Geo MV-6F");
    assert_eq!(neogeo.year.as_deref(), Some("1990"));
    assert_eq!(neogeo.manufacturer.as_deref(), Some("SNK"));
    assert_eq!(neogeo.source_file.as_deref(), Some("neogeo/neogeo.cpp"));
    assert!(neogeo.is_bios && !neogeo.is_device && neogeo.is_runnable);
    assert_eq!(neogeo.parent, None);
    assert_eq!(neogeo.clone_count, 0);
    assert_eq!(
        neogeo
            .bios_sets
            .iter()
            .map(|b| (b.name.as_str(), b.is_default))
            .collect::<Vec<_>>(),
        [("euro", false), ("japan", true)]
    );
    assert_eq!(neogeo.roms.len(), 2);
    assert_eq!(neogeo.roms[0].bios.as_deref(), Some("euro"));
    assert_eq!(neogeo.roms[0].crc.as_deref(), Some("9036d879"));
    assert_eq!(neogeo.devices.len(), 1);
    assert_eq!(neogeo.devices[0].device, link("z80", Some("Zilog Z80")));
    assert_eq!(neogeo.devices[0].instance_count, 2);
    assert_eq!(neogeo.chips[0].clock, Some(4_000_000));
    assert_eq!(neogeo.displays[0].width, Some(320));
    assert_eq!(neogeo.displays[0].refresh, Some(59.185606));
    assert_eq!(neogeo.sound_channels, Some(2));
    let input = neogeo.input.expect("input");
    assert_eq!(
        (input.players, input.coins, input.service, input.tilt),
        (Some(2), Some(2), true, false)
    );
    assert_eq!(neogeo.controls.len(), 2);
    assert_eq!(neogeo.controls[1].player, Some(2));
    assert_eq!(
        neogeo.driver.expect("driver").save_state.as_deref(),
        Some("supported")
    );
}

#[test]
fn machine_detail_resolves_relations() {
    let catalog = Catalog::new(false);

    let pacman = catalog.machine("pacman").expect("pacman");
    assert_eq!(
        pacman.parent,
        Some(link("puckman", Some("Puck Man (Japan set 1)")))
    );
    assert_eq!(
        pacman.rom_parent,
        Some(link("puckman", Some("Puck Man (Japan set 1)")))
    );
    assert!(pacman.roms[1].is_optional);
    assert_eq!(pacman.roms[1].status, "nodump");

    let puckman = catalog.machine("puckman").expect("puckman");
    assert_eq!(puckman.clone_count, 1);

    let kof98 = catalog.machine("kof98").expect("kof98");
    assert_eq!(kof98.parent, None);
    assert_eq!(
        kof98.rom_parent,
        Some(link("neogeo", Some("Neo-Geo MV-6F")))
    );
    assert_eq!(kof98.roms[1].merge_name.as_deref(), Some("sp-s2.sp1"));
}

#[test]
fn machine_detail_contains_media_samples_disks_and_features() {
    let catalog = Catalog::new(false);

    let fruitbank = catalog.machine("fruitbank").expect("fruitbank");
    assert_eq!(fruitbank.description, "Fruit Bank & Co <Club> ©");
    assert!(fruitbank.is_mechanical);
    assert_eq!(fruitbank.sample_of.as_deref(), Some("fruitsamples"));
    assert_eq!(fruitbank.samples, ["coin"]);
    assert_eq!(fruitbank.disks[0].name, "fruit_hdd");
    assert!(fruitbank.disks[0].is_writable);
    assert_eq!(fruitbank.disks[0].index, Some(0));
    let driver = fruitbank.driver.expect("driver");
    assert!(driver.requires_artwork && driver.incomplete && !driver.unofficial);
    assert_eq!(fruitbank.features[0].feature_type, "mechanical-reels");
    assert_eq!(fruitbank.features[0].overall.as_deref(), Some("unemulated"));

    let c64 = catalog.machine("c64").expect("c64");
    assert_eq!(c64.media_devices.len(), 2);
    assert_eq!(
        c64.media_devices[0].interface.as_deref(),
        Some("c64_cart,vic10_cart")
    );
    assert_eq!(
        c64.media_devices[0].instance_brief_name.as_deref(),
        Some("cart")
    );
    assert_eq!(c64.media_devices[0].extensions, ["80", "crt"]);
    assert!(c64.media_devices[1].extensions.is_empty());
    assert_eq!(c64.ram_options.len(), 2);
    assert_eq!(c64.ram_options[0].name.as_deref(), Some("64K"));
    assert!(c64.ram_options[0].is_default);
    assert_eq!(c64.chips.len(), 2);
    assert_eq!(c64.controls[0].buttons, Some(66));
}

#[test]
fn device_machine_has_no_driver_or_input() {
    let catalog = Catalog::new(false);
    let z80 = catalog.machine("z80").expect("z80");
    assert!(z80.is_device && !z80.is_runnable);
    assert_eq!(z80.driver, None);
    assert_eq!(z80.input, None);
    assert!(z80.roms.is_empty() && z80.devices.is_empty());
}

#[test]
fn missing_records_return_none() {
    let catalog = Catalog::new(true);
    assert_eq!(catalog.machine("missing"), None);
    assert_eq!(catalog.machine(""), None);
    assert_eq!(catalog.software("nes", "missing"), None);
    assert_eq!(catalog.software("missing", "smb"), None);
    assert_eq!(catalog.list("missing"), None);
}

#[test]
fn software_lists_of_a_machine_report_availability() {
    let without = Catalog::new(false);
    let lists = without.machine("c64").expect("c64").software_lists;
    assert_eq!(lists.len(), 2);
    assert!(
        lists
            .iter()
            .all(|list| !list.available && list.description.is_none())
    );

    let with = Catalog::new(true);
    let lists = with.machine("c64").expect("c64").software_lists;
    assert_eq!(lists[0].name, "c64_cart");
    assert_eq!(lists[0].status, "original");
    assert_eq!(lists[0].filter.as_deref(), Some("NTSC"));
    assert_eq!(lists[0].tag.as_deref(), Some("cart_list_c64"));
    assert!(lists[0].available);
    assert_eq!(
        lists[0].description.as_deref(),
        Some("Commodore 64 cartridges")
    );
    assert_eq!(lists[1].name, "vic10");
    assert_eq!(lists[1].status, "compatible");
    assert!(!lists[1].available);
}

#[test]
fn software_detail_contains_parts_and_media() {
    let catalog = Catalog::new(true);

    let clone = catalog.software("c64_cart", "gameb2").expect("gameb2");
    assert_eq!(
        clone.list_description.as_deref(),
        Some("Commodore 64 cartridges")
    );
    assert_eq!(clone.description, "Game Base (alt)");
    assert_eq!(clone.publisher.as_deref(), Some("<unknown>"));
    assert_eq!(clone.supported, "partial");
    assert_eq!(clone.parent, Some(link("gamebase", Some("Game Base"))));
    assert_eq!(clone.machine_count, 1);
    assert_eq!(
        clone.info,
        [
            name_value("alt_title", "Game Base 2"),
            name_value("alt_title", "GB2")
        ]
    );
    assert_eq!(clone.shared_features, [name_value("compatibility", "NTSC")]);
    let part = &clone.parts[0];
    assert_eq!(
        (part.name.as_str(), part.interface.as_str()),
        ("cart", "c64_cart")
    );
    assert_eq!(part.features, [name_value("slot", "standard")]);
    let roms = &part.data_areas[0].roms;
    assert_eq!(roms.len(), 2);
    assert_eq!(roms[0].name.as_deref(), Some("gameb2 lo.u2"));
    assert_eq!(roms[1].name, None);
    assert_eq!(roms[1].load_flag.as_deref(), Some("reload"));
    assert_eq!(roms[1].offset.as_deref(), Some("0x2000"));

    let parent = catalog.software("c64_cart", "gamebase").expect("gamebase");
    assert_eq!(parent.parent, None);
    assert_eq!(parent.clone_count, 1);
    assert_eq!(parent.parts[0].data_areas[0].roms[0].status, "baddump");

    let smb = catalog.software("nes", "smb").expect("smb");
    let chr = &smb.parts[0].data_areas[1];
    assert_eq!((chr.databits, chr.endian.as_str()), (16, "big"));
    assert_eq!(smb.machine_count, 0);

    let ys = catalog.software("pcecd", "ys12").expect("ys12");
    assert_eq!(ys.parts.len(), 2);
    assert!(ys.parts[0].data_areas.is_empty());
    let disk = &ys.parts[0].disk_areas[0].disks[0];
    assert_eq!(
        (disk.name.as_str(), disk.status.as_str()),
        ("ys i & ii (japan)", "baddump")
    );
    assert!(ys.parts[1].disk_areas[0].disks[0].is_writeable);
}

#[test]
fn same_software_name_in_different_lists_is_distinct() {
    let catalog = Catalog::new(true);
    let c64 = catalog.software("c64_cart", "tetris").expect("c64 tetris");
    let nes = catalog.software("nes", "tetris").expect("nes tetris");
    assert_eq!(c64.description, "Tetris (C64)");
    assert_eq!(c64.supported, "no");
    assert_eq!(nes.description, "Tetris (NES)");
}

#[test]
fn software_list_detail_counts_software_and_machines() {
    let catalog = Catalog::new(true);

    let c64 = catalog.list("c64_cart").expect("c64_cart");
    assert!(c64.available);
    assert_eq!(c64.description.as_deref(), Some("Commodore 64 cartridges"));
    assert_eq!(c64.notes.as_deref(), Some("Cartridges for the C64 & C128."));
    assert_eq!(c64.software_count, 3);
    assert_eq!(
        (c64.original_machine_count, c64.compatible_machine_count),
        (1, 0)
    );

    let nes = catalog.list("nes").expect("nes");
    assert_eq!(nes.software_count, 2);
    assert_eq!(
        (nes.original_machine_count, nes.compatible_machine_count),
        (0, 0)
    );

    // Referenced by a machine but not provided by MAME.
    let vic10 = catalog.list("vic10").expect("vic10");
    assert!(!vic10.available);
    assert_eq!(vic10.description, None);
    assert_eq!(vic10.software_count, 0);
    assert_eq!(
        (vic10.original_machine_count, vic10.compatible_machine_count),
        (0, 1)
    );
}

#[test]
fn software_list_detail_without_imported_software_lists() {
    let catalog = Catalog::new(false);
    let c64 = catalog.list("c64_cart").expect("referenced list");
    assert!(!c64.available);
    assert_eq!(c64.software_count, 0);
    assert_eq!(c64.original_machine_count, 1);
}

#[test]
fn clone_counts_match_the_paginated_clone_search() {
    let catalog = Catalog::new(true);

    let puckman = catalog.machine("puckman").expect("puckman");
    let clones = catalog.read(|connection| {
        search_machines(
            connection,
            &MachineSearchRequest {
                filters: MachineFilters {
                    clone_of: Some("puckman".to_owned()),
                    ..MachineFilters::default()
                },
                page_size: Some(1),
                ..MachineSearchRequest::default()
            },
            &[],
        )
        .expect("search")
    });
    assert_eq!(clones.total, puckman.clone_count);
    assert_eq!(clones.items[0].name, "pacman");

    let gamebase = catalog.software("c64_cart", "gamebase").expect("gamebase");
    let clones = catalog.read(|connection| {
        search_software(
            connection,
            &SoftwareSearchRequest {
                filters: SoftwareFilters {
                    lists: vec!["c64_cart".to_owned()],
                    clone_of: Some("gamebase".to_owned()),
                    ..SoftwareFilters::default()
                },
                ..SoftwareSearchRequest::default()
            },
            &[],
        )
        .expect("search")
    });
    assert_eq!(clones.total, gamebase.clone_count);
    assert_eq!(clones.items[0].name, "gameb2");
}

#[test]
fn details_serialize_with_camel_case_fields() {
    let catalog = Catalog::new(true);
    let machine = serde_json::to_value(catalog.machine("c64").expect("c64")).expect("json");
    for field in [
        "name",
        "sourceFile",
        "isBios",
        "romParent",
        "cloneCount",
        "mediaDevices",
        "softwareLists",
        "ramOptions",
    ] {
        assert!(machine.get(field).is_some(), "missing {field}");
    }
    assert_eq!(machine["softwareLists"][0]["available"], true);
    assert_eq!(machine["mediaDevices"][0]["instanceBriefName"], "cart");

    let software = serde_json::to_value(catalog.software("c64_cart", "gameb2").expect("gameb2"))
        .expect("json");
    assert_eq!(software["listName"], "c64_cart");
    assert_eq!(software["parent"]["name"], "gamebase");
    assert_eq!(
        software["parts"][0]["dataAreas"][0]["roms"][1]["loadFlag"],
        "reload"
    );

    let list = serde_json::to_value(catalog.list("vic10").expect("vic10")).expect("json");
    assert_eq!(list["available"], false);
    assert_eq!(list["compatibleMachineCount"], 1);
}
