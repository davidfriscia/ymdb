//! In-memory model of the output of `mame -listsoftware`.
//!
//! The model mirrors the XML. Names of `info`, `sharedfeat` and part
//! `feature` entries are kept verbatim: they are free-form metadata defined by
//! each software list, not categories interpreted by YMDB.

/// Attributes of a `<softwarelist>` element.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SoftwareListHeader {
    pub name: String,
    pub description: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Software {
    /// Short name, unique within its list only.
    pub name: String,
    /// Parent software in the same list.
    pub clone_of: Option<String>,
    /// Support status: `yes`, `partial` or `no`.
    pub supported: String,
    pub description: String,
    /// Year as printed by MAME; may contain `?` (for example `19??`).
    pub year: Option<String>,
    pub publisher: Option<String>,
    pub notes: Option<String>,
    /// `<info>` entries (for example `serial`, `release`, `alt_title`).
    pub info: Vec<NameValue>,
    /// `<sharedfeat>` entries shared by all parts (for example `compatibility`).
    pub shared_features: Vec<NameValue>,
    pub parts: Vec<SoftwarePart>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameValue {
    pub name: String,
    pub value: Option<String>,
}

/// One medium of a software item (a cartridge, a disk side, a tape...).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SoftwarePart {
    pub name: String,
    /// Device interface the part is loaded into (for example `nes_cart`).
    pub interface: String,
    pub features: Vec<NameValue>,
    pub data_areas: Vec<DataArea>,
    pub disk_areas: Vec<DiskArea>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataArea {
    pub name: String,
    /// Size in bytes.
    pub size: u64,
    /// Bus width in bits (DTD default 8).
    pub databits: u8,
    /// `little` or `big` (DTD default `little`).
    pub endian: String,
    pub roms: Vec<SoftwareRom>,
}

/// A ROM entry of a data area. Entries without a name are loading
/// instructions (for example `loadflag="reload"` or `fill`) for the
/// previous ROM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SoftwareRom {
    pub name: Option<String>,
    pub size: Option<u64>,
    pub length: Option<u64>,
    pub crc: Option<String>,
    pub sha1: Option<String>,
    /// Offset as printed by MAME (hexadecimal, for example `0x4000`).
    pub offset: Option<String>,
    pub value: Option<String>,
    /// Dump status: `good`, `baddump` or `nodump`.
    pub status: String,
    pub load_flag: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiskArea {
    pub name: String,
    pub disks: Vec<SoftwareDisk>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SoftwareDisk {
    pub name: String,
    pub sha1: Option<String>,
    /// Dump status: `good`, `baddump` or `nodump`.
    pub status: String,
    pub writeable: bool,
}
