//! Surreal record id helpers and Valence [`RecordId`] conversions.

use valence_core::error::{Error, Result};
use valence_core::RecordId;

/// Build a SurrealDB record id for a table and id string.
pub fn surreal_record_id_for(table: &str, id: &str) -> surrealdb::types::RecordId {
    surrealdb::types::RecordId::new(table, id)
}

/// Convert a Valence [`RecordId`] to Surreal's native record id.
pub fn surreal_from_valence(r: &RecordId) -> surrealdb::types::RecordId {
    surrealdb::types::RecordId::new(r.table(), r.id())
}

/// Convert a Surreal record id into Valence [`RecordId`].
pub fn valence_from_surreal(record: surrealdb::types::RecordId) -> RecordId {
    let table = record.table.to_string();
    let id = extract_id_from_surreal_record(&record).unwrap_or_else(|_| {
        surrealdb::types::Value::RecordId(record)
            .into_json_value()
            .to_string()
    });
    RecordId::new(table, id)
}

/// Extract the bare id string from a SurrealDB record id wire form.
///
/// # Errors
///
/// Returns [`Error::Validation`] when the id portion is empty.
pub fn extract_id_from_surreal_record(record: &surrealdb::types::RecordId) -> Result<String> {
    // Read plain keys directly: the wire form backtick-quotes ids that need escaping.
    match &record.key {
        surrealdb::types::RecordIdKey::String(s) if !s.is_empty() => return Ok(s.clone()),
        surrealdb::types::RecordIdKey::Number(n) => return Ok(n.to_string()),
        _ => {}
    }
    let wire = match surrealdb::types::Value::RecordId(record.clone()).into_json_value() {
        serde_json::Value::String(s) => s,
        other => other.to_string(),
    };
    extract_id_from_record_display(&wire)
}

/// Strip the first `table:` prefix from a Surreal thing display string; return the id portion.
///
/// # Errors
///
/// Returns [`Error::Validation`] when the id portion is empty.
pub fn extract_id_from_record_display(s: &str) -> Result<String> {
    let id = s.split_once(':').map_or(s, |(_, id_part)| id_part).trim();
    let id = id
        .trim_start_matches(['⟨', '‹', '«'])
        .trim_end_matches(['⟩', '›', '»']);
    // SurrealDB 3 wraps ids that need escaping in backticks.
    let id = id
        .strip_prefix('`')
        .and_then(|rest| rest.strip_suffix('`'))
        .unwrap_or(id);
    if id.is_empty() {
        return Err(Error::Validation(format!(
            "Invalid record id string: could not extract ID from {s:?}"
        )));
    }
    Ok(id.to_string())
}

/// Parse a row from `SELECT VALUE id` (JSON) into the record id string (no `table:` prefix).
///
/// # Errors
///
/// Returns [`Error::Validation`] / [`Error::Internal`] for empty or unexpected id shapes.
pub fn extract_id_from_select_value(v: &serde_json::Value) -> Result<String> {
    if let Ok(rid) = serde_json::from_value::<RecordId>(v.clone()) {
        return Ok(rid.id().to_string());
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

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::print_stdout,
        clippy::print_stderr
    )]

    use super::*;

    #[test]
    fn record_id_for_extract_id_roundtrip() {
        let rid = surreal_record_id_for("user", "abc123");
        let extracted = extract_id_from_surreal_record(&rid).unwrap();
        assert_eq!(extracted, "abc123");
    }

    #[test]
    fn hyphenated_uuid_key_extracts_bare_happy() {
        let uuid = "aa90d993-a8e9-4433-bdcb-db51b35ceb9d";
        let rid = surreal_record_id_for("team", uuid);
        assert_eq!(extract_id_from_surreal_record(&rid).unwrap(), uuid);
        assert_eq!(valence_from_surreal(rid).id(), uuid);
    }

    #[test]
    fn backtick_display_form_strips_happy() {
        let uuid = "aa90d993-a8e9-4433-bdcb-db51b35ceb9d";
        assert_eq!(
            extract_id_from_record_display(&format!("team:`{uuid}`")).unwrap(),
            uuid
        );
        assert!(extract_id_from_record_display("team:``").is_err());
    }

    #[test]
    fn valence_record_id_converts_to_surreal() {
        let v = RecordId::new("counter", "singleton");
        let s = surreal_from_valence(&v);
        assert_eq!(s.table.to_string(), "counter");
    }
}
