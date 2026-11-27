//! Connection cardinality, delete semantics, and id helpers for generated models.

use std::fmt;

use crate::error::{Error, Result};
use crate::RecordId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cardinality {
    HasOne,
    HasMany,
    ManyToMany,
}

impl fmt::Display for Cardinality {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Cardinality::HasOne => write!(f, "HasOne"),
            Cardinality::HasMany => write!(f, "HasMany"),
            Cardinality::ManyToMany => write!(f, "ManyToMany"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnDelete {
    Cascade,
    SetNull,
    Restrict,
}

impl fmt::Display for OnDelete {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OnDelete::Cascade => write!(f, "Cascade"),
            OnDelete::SetNull => write!(f, "SetNull"),
            OnDelete::Restrict => write!(f, "Restrict"),
        }
    }
}

/// # Errors
///
/// Returns an error when the requested operation cannot be completed.
pub fn id_from_model<T>(model: &T) -> Result<String>
where
    T: IdHolder,
{
    let r = model
        .record_id()
        .ok_or_else(|| Error::Validation("Model has no id (new/unsaved record)".into()))?;
    extract_id_from_record(r)
}

pub trait IdHolder {
    fn record_id(&self) -> Option<&RecordId>;
}

/// # Errors
///
/// Returns an error when the requested operation cannot be completed.
pub fn extract_id_from_record(r: &RecordId) -> Result<String> {
    Ok(r.id().to_string())
}

/// Drop one pair of backticks wrapping the id part of a SurrealDB `table:id` string.
/// SurrealDB 3 quotes ids that need escaping (a hyphenated UUID comes back as
/// `` `aa90…` ``).
fn trim_id_quotes(id: &str) -> &str {
    id.strip_prefix('`')
        .and_then(|rest| rest.strip_suffix('`'))
        .unwrap_or(id)
}

/// # Errors
///
/// Returns an error when the requested operation cannot be completed.
pub fn extract_id_from_record_display(s: &str) -> Result<String> {
    let id = s.split_once(':').map_or(s, |(_, id_part)| id_part).trim();
    let id = trim_id_quotes(
        id.trim_start_matches(['⟨', '‹', '«'])
            .trim_end_matches(['⟩', '›', '»']),
    );
    if id.is_empty() {
        return Err(Error::Validation(format!(
            "Invalid record id string: could not extract ID from {s:?}"
        )));
    }
    Ok(id.to_string())
}

/// # Errors
///
/// Returns an error when the requested operation cannot be completed.
pub fn extract_id_from_select_value(v: &serde_json::Value) -> Result<String> {
    if let Ok(rid) = serde_json::from_value::<RecordId>(v.clone()) {
        let id = extract_id_from_record(&rid)?;
        // A `table:id` string is SurrealDB wire form, which may quote the id.
        return Ok(if v.is_string() {
            trim_id_quotes(&id).to_string()
        } else {
            id
        });
    }
    match v {
        serde_json::Value::String(s) => extract_id_from_record_display(s),
        serde_json::Value::Number(n) => Ok(n.to_string()),
        serde_json::Value::Bool(b) => Ok(b.to_string()),
        // SQLite `SELECT id` rows are `{"id": "…"}`. Mem unique checks may return
        // the full typed row (id plus other columns) or a nested RecordId object.
        serde_json::Value::Object(map) => {
            if let Some(id) = map.get("id") {
                return extract_id_from_select_value(id);
            }
            Err(Error::Internal(format!(
                "unexpected id value in query row: {v}"
            )))
        }
        _ => Err(Error::Internal(format!(
            "unexpected id value in query row: {v}"
        ))),
    }
}

#[cfg(test)]
mod extract_id_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn backtick_quoted_id_strips_happy() {
        let uuid = "aa90d993-a8e9-4433-bdcb-db51b35ceb9d";
        assert_eq!(
            extract_id_from_record_display(&format!("team:`{uuid}`")).unwrap(),
            uuid
        );
        assert_eq!(
            extract_id_from_select_value(&json!(format!("team:`{uuid}`"))).unwrap(),
            uuid
        );
    }

    #[test]
    fn lone_or_empty_backticks_sad() {
        // A parsed RecordId is taken as is: a caller-chosen id with backticks keeps them.
        assert_eq!(
            extract_id_from_record(&RecordId::new("team", "`x`")).unwrap(),
            "`x`"
        );
        assert_eq!(extract_id_from_record_display("t:`abc").unwrap(), "`abc");
        assert!(extract_id_from_record_display("t:``").is_err());
    }

    #[test]
    fn string_and_wrapped_id() {
        assert_eq!(
            extract_id_from_select_value(&json!("mem-1")).unwrap(),
            "mem-1"
        );
        assert_eq!(
            extract_id_from_select_value(&json!({"id": "mem-1"})).unwrap(),
            "mem-1"
        );
    }

    #[test]
    fn nested_record_id_object() {
        let v = json!({"id": "mem-9", "table": "account_phone"});
        assert_eq!(extract_id_from_select_value(&v).unwrap(), "mem-9");
    }

    #[test]
    fn full_typed_row_with_nested_id() {
        let v = json!({
            "account": {"id": "acc-1", "table": "account"},
            "created_at": 1,
            "e164": "+15555550102",
            "id": {"id": "mem-9", "table": "account_phone"},
            "updated_at": 1,
            "verified_at": null
        });
        assert_eq!(extract_id_from_select_value(&v).unwrap(), "mem-9");
    }
}
