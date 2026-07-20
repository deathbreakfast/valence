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

/// # Errors
///
/// Returns an error when the requested operation cannot be completed.
pub fn extract_id_from_record_display(s: &str) -> Result<String> {
    let id = s.split_once(':').map_or(s, |(_, id_part)| id_part).trim();
    let id = id
        .trim_start_matches(['⟨', '‹', '«'])
        .trim_end_matches(['⟩', '›', '»']);
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
        return extract_id_from_record(&rid);
    }
    match v {
        serde_json::Value::String(s) => extract_id_from_record_display(s),
        serde_json::Value::Number(n) => Ok(n.to_string()),
        serde_json::Value::Bool(b) => Ok(b.to_string()),
        _ => Err(Error::Internal(format!(
            "unexpected id value in query row: {v}"
        ))),
    }
}
