//! Desired vs inspected layout → additive ops only.

use super::{FieldStorage, LayoutField, StorageLayout};
use crate::error::{Error, Result};

fn inspect_compatible(live: FieldStorage, desired: FieldStorage) -> bool {
    if live == desired {
        return true;
    }
    // Coarse PRAGMA / information_schema mapping.
    matches!(
        (live, desired),
        (
            FieldStorage::String,
            FieldStorage::String
                | FieldStorage::Date
                | FieldStorage::Json
                | FieldStorage::Currency
        ) | (FieldStorage::Json, FieldStorage::Currency | FieldStorage::Json)
            | (FieldStorage::Integer, FieldStorage::Boolean)
            | (FieldStorage::Boolean, FieldStorage::Integer)
    )
}

/// One additive change to apply during sync.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdditiveOp {
    /// Add a missing field/column.
    AddField(LayoutField),
    /// Create a unique index that is missing.
    AddUniqueIndex { field: String },
    /// Create a non-unique index that is missing.
    AddIndex { field: String },
}

/// Result of comparing desired layout to live physical layout.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LayoutDiff {
    pub ops: Vec<AdditiveOp>,
}

/// Compute additive ops to bring `live` toward `desired`.
///
/// Ship policy: refuse drops / renames / type changes (orphan live fields → Validation).
///
/// # Errors
///
/// Returns [`Error::Validation`] when live has fields not in desired (destructive).
pub fn additive_ops(desired: &StorageLayout, live: &StorageLayout) -> Result<LayoutDiff> {
    for live_f in &live.fields {
        if !desired.fields.iter().any(|d| d.name == live_f.name) {
            return Err(Error::Validation(format!(
                "sync_typed_table refuses drop of live field {}.{} (destructive sync is Future)",
                live.table, live_f.name
            )));
        }
        if let Some(d) = desired.fields.iter().find(|d| d.name == live_f.name) {
            // Inspected layouts often collapse TEXT/JSONB to String — only refuse when
            // both sides look precise and disagree (e.g. Integer vs Boolean).
            if d.storage != live_f.storage
                && live_f.name != "id"
                && !inspect_compatible(live_f.storage, d.storage)
            {
                return Err(Error::Validation(format!(
                    "sync_typed_table refuses type change on {}.{} ({:?} → {:?})",
                    live.table, live_f.name, live_f.storage, d.storage
                )));
            }
        }
    }

    let mut ops = Vec::new();
    for d in &desired.fields {
        if !live.fields.iter().any(|l| l.name == d.name) {
            ops.push(AdditiveOp::AddField(d.clone()));
            continue;
        }
        let live_f = live.fields.iter().find(|l| l.name == d.name).unwrap();
        if d.unique && !live_f.unique {
            ops.push(AdditiveOp::AddUniqueIndex {
                field: d.name.clone(),
            });
        }
        if d.indexed && !live_f.indexed && !d.unique {
            ops.push(AdditiveOp::AddIndex {
                field: d.name.clone(),
            });
        }
    }
    Ok(LayoutDiff { ops })
}
