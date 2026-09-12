//! Declared data-use types for Valence transparency (`use_!` / `*_used`).

mod purpose;
mod source_link;

pub use purpose::DataUsePurpose;
pub use source_link::{SourceLink, SourceLinkConfig};

/// CRUD-shaped operation inferred from a `*_used` method name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataOp {
    Read,
    Create,
    Update,
    Delete,
}

impl DataOp {
    /// Stable label for UI tabs and snapshots.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Create => "create",
            Self::Update => "update",
            Self::Delete => "delete",
        }
    }
}

/// What a declared use targets: a concrete schema, a trait, or Unscoped access.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataUseTarget {
    /// Typed Model / schema table access.
    Schema(String),
    /// Trait `*QueryAll` (or trait write helpers).
    Trait(String),
    /// QueryCore / raw backend paths that are not schema- or trait-scoped.
    Unscoped,
}

/// One declared data use (catalog row / scan hit).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DataUse {
    pub purpose: String,
    pub file: String,
    pub line: u32,
    pub crate_name: String,
    pub target: DataUseTarget,
    pub op: DataOp,
    /// Method name that was scanned (`get_used`, `query_used`, …).
    pub method: String,
    /// Optional trait name when this row is shown on a schema page via fan-out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub via_trait: Option<String>,
}
