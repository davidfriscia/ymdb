//! Detail views of single catalog records.
//!
//! These DTOs carry every imported attribute of one record and its bounded
//! relations (ROMs, devices, parts...). One-to-many relations that can be large
//! (clones of a machine, software of a list, machines using a list) are not
//! included: they are counted here and listed through the paginated search
//! (`cloneOf`, `lists` and `softwareLists` filters).
//!
//! Functions return `None` when the record does not exist, for example after a
//! re-import removed it.

pub mod machine;
pub mod software;

use serde::Serialize;

/// Reference to another record. `description` is `None` when the reference
/// cannot be resolved in the catalog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordLink {
    pub name: String,
    pub description: Option<String>,
}

/// A free-form name/value pair as declared by MAME.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NameValue {
    pub name: String,
    pub value: Option<String>,
}

fn to_u64(value: i64) -> u64 {
    u64::try_from(value).unwrap_or(0)
}

#[cfg(test)]
mod tests;
