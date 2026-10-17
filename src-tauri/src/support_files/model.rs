//! Normalized model of the MAME support files.
//!
//! Every format is parsed into the same shape: memberships (a machine belongs
//! to a value of a classification, possibly to several) and attributes (a
//! machine has one value). The dimensions exposed to the user do not depend
//! on the physical format: `catver.ini` and `catlist.ini` both feed Category,
//! `catver.ini` and `version.ini` both feed Version Added.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::{Deserialize, Serialize};

/// The support file formats YMDB imports. Each format has at most one
/// imported source, identified by the format itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SupportFileFormat {
    Catver,
    Genre,
    Catlist,
    Version,
    Series,
}

impl SupportFileFormat {
    pub const ALL: [Self; 5] = [
        Self::Catver,
        Self::Genre,
        Self::Catlist,
        Self::Version,
        Self::Series,
    ];

    /// Identifier stored in the repository.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Catver => "catver",
            Self::Genre => "genre",
            Self::Catlist => "catlist",
            Self::Version => "version",
            Self::Series => "series",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|format| format.as_str() == id)
    }

    /// The conventional file name, as written in the header of the files.
    pub fn file_name(self) -> &'static str {
        match self {
            Self::Catver => "catver.ini",
            Self::Genre => "genre.ini",
            Self::Catlist => "catlist.ini",
            Self::Version => "version.ini",
            Self::Series => "series.ini",
        }
    }

    /// The dimensions this format can feed.
    pub fn dimensions(self) -> &'static [Dimension] {
        match self {
            Self::Catver => &[Dimension::Category, Dimension::VersionAdded],
            Self::Genre => &[Dimension::Genre],
            Self::Catlist => &[Dimension::Category],
            Self::Version => &[Dimension::VersionAdded],
            Self::Series => &[Dimension::Series],
        }
    }
}

impl fmt::Display for SupportFileFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.file_name())
    }
}

/// A classification dimension shown to the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Dimension {
    Genre,
    Category,
    Series,
    VersionAdded,
}

/// How a dimension relates machines to values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DimensionKind {
    /// A machine belongs to zero or more values (Genre, Category, Series).
    Membership,
    /// A machine has at most one value (Version Added).
    Attribute,
}

impl Dimension {
    pub const ALL: [Self; 4] = [
        Self::Genre,
        Self::Category,
        Self::Series,
        Self::VersionAdded,
    ];

    /// Identifier stored in the repository and the projections.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Genre => "genre",
            Self::Category => "category",
            Self::Series => "series",
            Self::VersionAdded => "versionAdded",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|dimension| dimension.as_str() == id)
    }

    pub fn kind(self) -> DimensionKind {
        match self {
            Self::VersionAdded => DimensionKind::Attribute,
            Self::Genre | Self::Category | Self::Series => DimensionKind::Membership,
        }
    }

    /// Whether values are paths whose ancestors are selectable too.
    pub fn is_hierarchical(self) -> bool {
        self == Self::Category
    }
}

/// A machine belongs to a value of a membership dimension.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Membership {
    pub dimension: Dimension,
    pub value: String,
    pub machine: String,
}

/// A machine has a value for an attribute dimension.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Attribute {
    pub dimension: Dimension,
    pub machine: String,
    pub value: String,
}

/// Non-fatal problems found while parsing, grouped by kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WarningKind {
    /// The file is not valid UTF-8 and was decoded as Windows-1252.
    NotUtf8,
    /// A line that is neither a section, a comment nor a valid entry.
    InvalidLine,
    /// A machine name that is not a MAME short name.
    InvalidMachineName,
    /// The same entry is listed more than once (kept once).
    DuplicateEntry,
    /// A machine has two values for an attribute (the first one is kept).
    ConflictingValue,
    /// A version that YMDB cannot order (kept, sorted last).
    UnrecognizedVersion,
    /// An expected section is missing.
    MissingSection,
    /// Entries of a section YMDB does not use.
    IgnoredSection,
    /// Entries before the first section.
    EntriesOutsideSection,
}

/// Number of examples kept per warning kind.
pub const MAX_WARNING_EXAMPLES: usize = 5;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParseWarning {
    pub kind: WarningKind,
    pub count: u64,
    /// The first occurrences, such as `line 12: foo bar`.
    pub examples: Vec<String>,
}

/// Collects warnings by kind, keeping a few examples of each.
#[derive(Debug, Default)]
pub struct Warnings {
    by_kind: BTreeMap<WarningKind, ParseWarning>,
}

impl Warnings {
    pub fn add(&mut self, kind: WarningKind, example: impl FnOnce() -> String) {
        let warning = self.by_kind.entry(kind).or_insert_with(|| ParseWarning {
            kind,
            count: 0,
            examples: Vec::new(),
        });
        warning.count += 1;
        if warning.examples.len() < MAX_WARNING_EXAMPLES {
            warning.examples.push(example());
        }
    }

    pub fn into_vec(self) -> Vec<ParseWarning> {
        self.by_kind.into_values().collect()
    }
}

/// A support file after parsing, before it is stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedSupportFile {
    pub format: SupportFileFormat,
    /// Version declared in the header, such as `0.288`.
    pub declared_version: Option<String>,
    /// Sorted and without duplicates.
    pub memberships: Vec<Membership>,
    /// Sorted, one per machine and dimension.
    pub attributes: Vec<Attribute>,
    pub warnings: Vec<ParseWarning>,
}

/// Counts of one dimension of a parsed or stored source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DimensionStatistics {
    pub dimension: Dimension,
    /// Distinct machines with a value.
    pub machine_count: u64,
    /// Distinct values (categories, genres, series or versions).
    pub value_count: u64,
}

impl ParsedSupportFile {
    /// Statistics of the dimensions present in the file, in dimension order.
    pub fn statistics(&self) -> Vec<DimensionStatistics> {
        let mut machines: BTreeMap<Dimension, BTreeSet<&str>> = BTreeMap::new();
        let mut values: BTreeMap<Dimension, BTreeSet<&str>> = BTreeMap::new();
        for membership in &self.memberships {
            machines
                .entry(membership.dimension)
                .or_default()
                .insert(&membership.machine);
            values
                .entry(membership.dimension)
                .or_default()
                .insert(&membership.value);
        }
        for attribute in &self.attributes {
            machines
                .entry(attribute.dimension)
                .or_default()
                .insert(&attribute.machine);
            values
                .entry(attribute.dimension)
                .or_default()
                .insert(&attribute.value);
        }
        machines
            .into_iter()
            .map(|(dimension, machines)| DimensionStatistics {
                dimension,
                machine_count: to_u64(machines.len()),
                value_count: to_u64(values.get(&dimension).map_or(0, BTreeSet::len)),
            })
            .collect()
    }

    /// Distinct machines over every dimension.
    pub fn machine_count(&self) -> u64 {
        let machines: BTreeSet<&str> = self
            .memberships
            .iter()
            .map(|membership| membership.machine.as_str())
            .chain(
                self.attributes
                    .iter()
                    .map(|attribute| attribute.machine.as_str()),
            )
            .collect();
        to_u64(machines.len())
    }
}

/// Separators of the levels of a hierarchical value: `catver.ini` writes
/// `Shooter / Flying Vertical`, `catlist.ini` also `Arcade: Shooter / Flying
/// Vertical`. Text without separators (`Game Console/Computer`) is one level.
const LEVEL_SEPARATORS: [&str; 2] = [" / ", ": "];

/// The joiner of the levels of a normalized path.
pub const PATH_SEPARATOR: &str = " / ";

/// The levels of a hierarchical value, as found in the data: no depth is
/// assumed and empty levels are dropped.
pub fn path_levels(value: &str) -> Vec<&str> {
    let mut levels = vec![value];
    for separator in LEVEL_SEPARATORS {
        levels = levels
            .into_iter()
            .flat_map(|level| level.split(separator))
            .collect();
    }
    let levels: Vec<&str> = levels
        .into_iter()
        .map(str::trim)
        .filter(|level| !level.is_empty())
        .collect();
    if levels.is_empty() {
        vec![value.trim()]
    } else {
        levels
    }
}

/// The node paths of a value: every ancestor and the value itself, joined
/// with [`PATH_SEPARATOR`], from the root down. A non-hierarchical value is
/// its own single node.
pub fn value_nodes(dimension: Dimension, value: &str) -> Vec<String> {
    if !dimension.is_hierarchical() {
        return vec![value.to_owned()];
    }
    let levels = path_levels(value);
    (1..=levels.len())
        .map(|depth| levels[..depth].join(PATH_SEPARATOR))
        .collect()
}

fn to_u64(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn formats_of(dimension: Dimension) -> Vec<SupportFileFormat> {
        SupportFileFormat::ALL
            .into_iter()
            .filter(|format| format.dimensions().contains(&dimension))
            .collect()
    }

    #[test]
    fn formats_feed_their_dimensions() {
        assert_eq!(
            formats_of(Dimension::Category),
            [SupportFileFormat::Catver, SupportFileFormat::Catlist]
        );
        assert_eq!(
            formats_of(Dimension::VersionAdded),
            [SupportFileFormat::Catver, SupportFileFormat::Version]
        );
        assert_eq!(formats_of(Dimension::Series), [SupportFileFormat::Series]);
        for format in SupportFileFormat::ALL {
            assert_eq!(SupportFileFormat::from_id(format.as_str()), Some(format));
        }
        for dimension in Dimension::ALL {
            assert_eq!(Dimension::from_id(dimension.as_str()), Some(dimension));
        }
    }

    #[test]
    fn derives_category_nodes_from_the_data() {
        assert_eq!(
            value_nodes(Dimension::Category, "Shooter / Flying Vertical"),
            ["Shooter", "Shooter / Flying Vertical"]
        );
        assert_eq!(
            value_nodes(Dimension::Category, "Arcade: Arcade / Pinball * Mature *"),
            [
                "Arcade",
                "Arcade / Arcade",
                "Arcade / Arcade / Pinball * Mature *"
            ]
        );
        assert_eq!(
            value_nodes(
                Dimension::Category,
                "Handheld / Plug n' Play TV Game / Music"
            ),
            [
                "Handheld",
                "Handheld / Plug n' Play TV Game",
                "Handheld / Plug n' Play TV Game / Music"
            ]
        );
        assert_eq!(
            value_nodes(Dimension::Category, "Game Console/Computer"),
            ["Game Console/Computer"]
        );
        assert_eq!(value_nodes(Dimension::Category, " / "), ["/"]);
        // Series names may contain separators but are not hierarchical.
        assert_eq!(
            value_nodes(Dimension::Series, "Ketsui: Kizuna Jigoku Tachi"),
            ["Ketsui: Kizuna Jigoku Tachi"]
        );
    }

    #[test]
    fn keeps_a_few_examples_per_warning_kind() {
        let mut warnings = Warnings::default();
        for line in 0..8 {
            warnings.add(WarningKind::InvalidLine, || format!("line {line}"));
        }
        warnings.add(WarningKind::NotUtf8, String::new);
        let warnings = warnings.into_vec();

        assert_eq!(warnings.len(), 2);
        assert_eq!(warnings[0].kind, WarningKind::NotUtf8);
        assert_eq!(warnings[1].count, 8);
        assert_eq!(warnings[1].examples.len(), MAX_WARNING_EXAMPLES);
    }
}
