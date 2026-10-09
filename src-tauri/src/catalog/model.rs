//! In-memory model of one `<machine>` entry of `mame -listxml`.
//!
//! The model mirrors the XML closely and keeps its semantic distinctions
//! (for example devices, BIOS sets and mechanical machines are flags, not a
//! derived category). Values whose format is not guaranteed by MAME, such as
//! the year (`199?`) or ROM offsets, are kept as text.

/// Attributes of the `<mame>` root element.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListXmlHeader {
    /// Build string, for example `0.280 (mame0280)`.
    pub build: Option<String>,
    pub debug: bool,
    /// Version of the MAME configuration file format (`mameconfig`).
    pub mame_config: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Machine {
    pub name: String,
    pub source_file: Option<String>,
    pub is_bios: bool,
    pub is_device: bool,
    pub is_mechanical: bool,
    pub runnable: bool,
    /// Parent machine of a clone.
    pub clone_of: Option<String>,
    /// Machine whose ROMs this machine uses (the parent, or a BIOS machine).
    pub rom_of: Option<String>,
    /// Sample set used by the machine; not necessarily a machine name.
    pub sample_of: Option<String>,
    pub description: String,
    /// Year as printed by MAME; may contain `?` (for example `199?`).
    pub year: Option<String>,
    pub manufacturer: Option<String>,
    pub bios_sets: Vec<BiosSet>,
    pub roms: Vec<Rom>,
    pub disks: Vec<Disk>,
    /// Devices instantiated by the machine, with the number of references.
    pub device_refs: Vec<DeviceRef>,
    pub samples: Vec<String>,
    pub chips: Vec<Chip>,
    pub displays: Vec<Display>,
    pub sound_channels: Option<u32>,
    pub input: Option<Input>,
    pub driver: Option<Driver>,
    pub features: Vec<Feature>,
    /// Media devices (`<device>` elements), such as cartridge slots or floppy drives.
    pub media_devices: Vec<MediaDevice>,
    pub software_lists: Vec<SoftwareListRef>,
    pub ram_options: Vec<RamOption>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BiosSet {
    pub name: String,
    pub description: String,
    pub is_default: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rom {
    pub name: String,
    /// BIOS set this ROM belongs to.
    pub bios: Option<String>,
    pub size: u64,
    pub crc: Option<String>,
    pub sha1: Option<String>,
    /// Name of the same ROM in the parent set.
    pub merge: Option<String>,
    pub region: Option<String>,
    pub offset: Option<String>,
    /// Dump status: `good`, `baddump` or `nodump`.
    pub status: String,
    pub optional: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Disk {
    pub name: String,
    pub sha1: Option<String>,
    pub merge: Option<String>,
    pub region: Option<String>,
    pub index: Option<u32>,
    pub writable: bool,
    pub status: String,
    pub optional: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceRef {
    pub name: String,
    /// How many times the device is referenced by the machine.
    pub count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chip {
    pub name: String,
    pub tag: Option<String>,
    /// `cpu` or `audio`.
    pub chip_type: String,
    pub clock: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Display {
    pub tag: Option<String>,
    /// `raster`, `vector`, `lcd`, `svg` or `unknown`.
    pub display_type: String,
    pub rotate: Option<u16>,
    pub flip_x: bool,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub refresh: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Input {
    pub players: u32,
    pub coins: Option<u32>,
    pub service: bool,
    pub tilt: bool,
    pub controls: Vec<Control>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Control {
    pub control_type: String,
    pub player: Option<u32>,
    pub buttons: Option<u32>,
    pub required_buttons: Option<u32>,
    pub ways: Option<String>,
    pub ways2: Option<String>,
    pub ways3: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Driver {
    /// Overall status: `good`, `imperfect` or `preliminary`.
    pub status: String,
    pub emulation: Option<String>,
    pub cocktail: Option<String>,
    /// `supported` or `unsupported`.
    pub save_state: Option<String>,
    pub requires_artwork: bool,
    pub unofficial: bool,
    pub no_sound_hardware: bool,
    pub incomplete: bool,
}

/// An emulation feature flagged as imperfect or unemulated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Feature {
    pub feature_type: String,
    pub status: Option<String>,
    pub overall: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaDevice {
    pub device_type: String,
    pub tag: Option<String>,
    /// Software interfaces accepted by the device, comma-separated as in the XML.
    pub interface: Option<String>,
    /// Raw `mandatory` attribute (CDATA; MAME prints `1`).
    pub mandatory: Option<String>,
    /// Raw `fixed_image` attribute (CDATA; MAME prints `1`).
    pub fixed_image: Option<String>,
    pub instance_name: Option<String>,
    pub instance_brief_name: Option<String>,
    pub extensions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SoftwareListRef {
    pub tag: Option<String>,
    pub name: String,
    /// `original` or `compatible`.
    pub status: String,
    pub filter: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RamOption {
    pub name: Option<String>,
    /// RAM size in bytes.
    pub size: u64,
    pub is_default: bool,
}
