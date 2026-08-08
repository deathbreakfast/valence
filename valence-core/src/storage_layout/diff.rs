//! Desired vs inspected layout → additive ops + safe tweaks.

use super::{FieldStorage, LayoutField, StorageLayout};
use crate::error::{Error, Result};
use crate::KnownEngines;

fn inspect_compatible(live: FieldStorage, desired: FieldStorage) -> bool {
    if live == desired {
        return true;
    }
    // Coarse PRAGMA / information_schema mapping.
    matches!(
        (live, desired),
        (
            FieldStorage::String,
            FieldStorage::String | FieldStorage::Date | FieldStorage::Json | FieldStorage::Currency
        ) | (
            FieldStorage::Json,
            FieldStorage::Currency | FieldStorage::Json
        ) | (FieldStorage::Integer, FieldStorage::Boolean)
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

/// Nullability / DEFAULT changes that do not rewrite existing row payloads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SafeTweak {
    /// Allow NULL (`DROP NOT NULL` on Postgres).
    SetNullable { field: String },
    /// Disallow NULL (`SET NOT NULL` on Postgres; fails if NULLs exist).
    SetNotNull { field: String },
    /// Set column DEFAULT from schema field default string.
    SetDefault { field: String, value: String },
    /// Clear column DEFAULT.
    DropDefault { field: String },
}

/// Result of comparing desired layout to live physical layout.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LayoutDiff {
    /// Additive field / index ops.
    pub ops: Vec<AdditiveOp>,
    /// Safe nullability / default tweaks.
    pub tweaks: Vec<SafeTweak>,
}

fn engine_supports_nullability_tweaks(engine_id: &str) -> bool {
    matches!(
        engine_id,
        KnownEngines::POSTGRES | KnownEngines::HYBRID_INDRA_SQL
    )
}

fn engine_supports_default_tweaks(engine_id: &str) -> bool {
    matches!(
        engine_id,
        KnownEngines::POSTGRES | KnownEngines::HYBRID_INDRA_SQL
    )
}

/// Compute additive ops + safe tweaks to bring `live` toward `desired`.
///
/// Ship policy: refuse drops / renames / type changes (orphan live fields → Validation).
/// SQLite cannot change nullability without a table rebuild → Validation.
///
/// # Errors
///
/// Returns [`Error::Validation`] when live has fields not in desired, incompatible types,
/// or an unsupported safe tweak on this engine.
pub fn additive_ops(desired: &StorageLayout, live: &StorageLayout) -> Result<LayoutDiff> {
    layout_diff(desired, live, KnownEngines::SQLITE)
}

/// Like [`additive_ops`], but engine-aware for safe tweaks.
///
/// # Errors
///
/// Same as [`additive_ops`], plus engine-specific refuse for unsupported tweaks.
pub fn layout_diff(
    desired: &StorageLayout,
    live: &StorageLayout,
    engine_id: &str,
) -> Result<LayoutDiff> {
    for live_f in &live.fields {
        if !desired.fields.iter().any(|d| d.name == live_f.name) {
            return Err(Error::Validation(format!(
                "sync_typed_table refuses drop of live field {}.{} (destructive sync is Future)",
                live.table, live_f.name
            )));
        }
        if let Some(d) = desired.fields.iter().find(|d| d.name == live_f.name) {
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
    let mut tweaks = Vec::new();
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

        if d.name == "id" {
            continue;
        }

        if d.nullable != live_f.nullable {
            if !engine_supports_nullability_tweaks(engine_id) {
                return Err(Error::Validation(format!(
                    "sync_typed_table refuses nullability change on {}.{} on engine {engine_id} \
                     (SQLite requires table rebuild — Future)",
                    live.table, d.name
                )));
            }
            if d.nullable {
                tweaks.push(SafeTweak::SetNullable {
                    field: d.name.clone(),
                });
            } else {
                tweaks.push(SafeTweak::SetNotNull {
                    field: d.name.clone(),
                });
            }
        }

        if engine_supports_default_tweaks(engine_id) {
            match (&d.default, &live_f.default) {
                (Some(want), live_def) if live_def.as_deref() != Some(want.as_str()) => {
                    tweaks.push(SafeTweak::SetDefault {
                        field: d.name.clone(),
                        value: want.clone(),
                    });
                }
                (None, Some(_)) => {
                    tweaks.push(SafeTweak::DropDefault {
                        field: d.name.clone(),
                    });
                }
                _ => {}
            }
        }
    }
    Ok(LayoutDiff { ops, tweaks })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(name: &str, nullable: bool) -> LayoutField {
        LayoutField {
            name: name.into(),
            storage: FieldStorage::String,
            primary_key: name == "id",
            nullable,
            unique: name == "id",
            indexed: false,
            default: None,
        }
    }

    #[test]
    fn add_missing_field() {
        let desired = StorageLayout {
            table: "t".into(),
            fields: vec![
                field("id", false),
                field("name", true),
                field("score", true),
            ],
        };
        let live = StorageLayout {
            table: "t".into(),
            fields: vec![field("id", false), field("name", true)],
        };
        let diff = layout_diff(&desired, &live, KnownEngines::SQLITE).unwrap();
        assert!(matches!(diff.ops[0], AdditiveOp::AddField(ref f) if f.name == "score"));
        assert!(diff.tweaks.is_empty());
    }

    #[test]
    fn refuse_orphan_live_field() {
        let desired = StorageLayout {
            table: "t".into(),
            fields: vec![field("id", false), field("name", true)],
        };
        let live = StorageLayout {
            table: "t".into(),
            fields: vec![
                field("id", false),
                field("name", true),
                field("legacy", true),
            ],
        };
        let err = layout_diff(&desired, &live, KnownEngines::SQLITE).unwrap_err();
        assert!(err.to_string().contains("refuses drop"));
    }

    #[test]
    fn postgres_nullability_tweak() {
        let desired = StorageLayout {
            table: "t".into(),
            fields: vec![field("id", false), field("name", true)],
        };
        let mut live_name = field("name", false);
        live_name.nullable = false;
        let live = StorageLayout {
            table: "t".into(),
            fields: vec![field("id", false), live_name],
        };
        let diff = layout_diff(&desired, &live, KnownEngines::POSTGRES).unwrap();
        assert_eq!(
            diff.tweaks,
            vec![SafeTweak::SetNullable {
                field: "name".into()
            }]
        );
    }

    #[test]
    fn sqlite_nullability_refused() {
        let desired = StorageLayout {
            table: "t".into(),
            fields: vec![field("id", false), field("name", true)],
        };
        let mut live_name = field("name", false);
        live_name.nullable = false;
        let live = StorageLayout {
            table: "t".into(),
            fields: vec![field("id", false), live_name],
        };
        let err = layout_diff(&desired, &live, KnownEngines::SQLITE).unwrap_err();
        assert!(err.to_string().contains("nullability"));
    }
}
