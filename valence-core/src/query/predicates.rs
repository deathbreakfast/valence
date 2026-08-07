//! Field predicates, sort direction, and lightweight row types used by [`super::QueryCore`].
//!
//! These types are generated into model query builders by `valence-codegen`. See the
//! [crate-level overview](crate) and [`super`](super) for how they compose into SurrealQL.

use serde::{Deserialize, Serialize};

/// Sort direction for ordering query results.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDirection {
    /// Ascending order
    Asc,
    /// Descending order
    Desc,
}

/// Predicate for integer fields.
#[derive(Debug, Clone)]
pub enum IntPredicate {
    /// Field equals value
    Equals(i64),
    /// Field is greater than value
    GreaterThan(i64),
    /// Field is greater than or equal to value
    GreaterThanOrEqual(i64),
    /// Field is less than value
    LessThan(i64),
    /// Field is less than or equal to value
    LessThanOrEqual(i64),
}

/// Predicate for string/text fields.
#[derive(Debug, Clone)]
pub enum StringPredicate {
    /// Field equals value (exact match)
    Equals(String),
    /// Field contains value (substring match)
    Contains(String),
    /// Field starts with value
    StartsWith(String),
    /// Field ends with value
    EndsWith(String),
    /// Field is lexicographically less than value (e.g. RFC3339 expire-at vs now)
    LessThan(String),
}

/// Predicate for datetime fields.
#[derive(Debug, Clone)]
pub enum DateTimePredicate {
    /// Field equals value
    Equals(chrono::DateTime<chrono::Utc>),
    /// Field is after value
    After(chrono::DateTime<chrono::Utc>),
    /// Field is before value
    Before(chrono::DateTime<chrono::Utc>),
}

/// Predicate for record-link fields (stored as Surreal [`RecordId`](crate::RecordId)).
#[derive(Debug, Clone)]
pub enum RecordPredicate {
    /// Field equals value
    Equals(crate::RecordId),
}

/// Predicate for checking null/not-null on optional fields.
#[derive(Debug, Clone, Copy)]
pub enum NullPredicate {
    /// Field is NULL
    IsNone,
    /// Field is NOT NULL
    IsSome,
}

/// Internal representation of an ORDER BY clause.
#[derive(Debug, Clone)]
pub struct OrderBy {
    pub field: String,
    pub direction: SortDirection,
}

/// Minimal record containing only the ID field.
///
/// Used for queries that only need to fetch record identifiers.
/// Deserializes `id` from either a bare string or a `{ table, id }` object.
#[derive(Debug, Clone, Serialize)]
pub struct IdOnlyRecord {
    pub id: String,
}

impl<'de> Deserialize<'de> for IdOnlyRecord {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        use serde::de::Error;
        use serde_json::Value;

        let value = Value::deserialize(deserializer)?;
        let id = match value {
            // Some adapters historically returned bare id strings for `SELECT id`.
            Value::String(s) => crate::row_json::thing_to_id_only(s),
            Value::Object(map) => match map.get("id") {
                Some(Value::String(s)) => crate::row_json::thing_to_id_only(s.clone()),
                Some(Value::Object(inner)) => inner
                    .get("id")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
                    .ok_or_else(|| D::Error::custom("IdOnlyRecord.id object missing string id"))?,
                Some(other) => {
                    return Err(D::Error::custom(format!(
                        "IdOnlyRecord.id expected string or {{table,id}} object, got {other}"
                    )));
                }
                None => {
                    return Err(D::Error::custom("IdOnlyRecord missing id field"));
                }
            },
            other => {
                return Err(D::Error::custom(format!(
                    "IdOnlyRecord expected string or object, got {other}"
                )));
            }
        };
        Ok(IdOnlyRecord { id })
    }
}
