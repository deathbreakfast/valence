//! Record JSON ↔ typed field maps and write-time type checks.

use super::{FieldStorage, LayoutField, StorageLayout};
use crate::error::{Error, Result};
use serde_json::{Map, Value};

/// Split a Valence record into storage id + field map (no nested `id` object).
pub fn split_record_fields(content: Value) -> (Option<String>, Map<String, Value>) {
    let mut map = match content {
        Value::Object(m) => m,
        _ => Map::new(),
    };
    let id = map.remove("id").and_then(|v| match v {
        Value::String(s) => Some(s),
        Value::Object(o) => o.get("id").and_then(|x| x.as_str()).map(str::to_string),
        _ => None,
    });
    (id, map)
}

/// Rebuild Valence JSON row from flat column values.
pub fn row_from_columns(table: &str, id: &str, mut fields: Map<String, Value>) -> Value {
    fields.insert(
        "id".into(),
        Value::Object(Map::from_iter([
            ("table".into(), Value::String(table.to_string())),
            ("id".into(), Value::String(id.to_string())),
        ])),
    );
    Value::Object(fields)
}

/// Non-id field names from layout.
#[must_use]
pub fn field_names_excluding_id(layout: &StorageLayout) -> Vec<&str> {
    layout
        .fields
        .iter()
        .filter(|f| f.name != "id")
        .map(|f| f.name.as_str())
        .collect()
}

/// Build field list from content keys (for inspect-less sync of ad-hoc tables).
pub fn fields_from_content(content: &Value) -> Vec<String> {
    content
        .as_object()
        .map(|o| o.keys().filter(|k| *k != "id").cloned().collect::<Vec<_>>())
        .unwrap_or_default()
}

/// Coerce / validate a JSON value for a physical storage kind on write.
///
/// # Errors
///
/// Returns [`Error::Validation`] on type mismatch.
pub fn coerce_for_storage(storage: FieldStorage, value: &Value) -> Result<Value> {
    if value.is_null() {
        return Ok(Value::Null);
    }
    match storage {
        FieldStorage::String | FieldStorage::Date => match value {
            Value::String(_) => Ok(value.clone()),
            Value::Number(n) => Ok(Value::String(n.to_string())),
            Value::Bool(b) => Ok(Value::String(b.to_string())),
            // Record links arrive as `{ "table", "id" }`; persist canonical `table:id`.
            Value::Object(o) => record_object_to_rid(o)
                .map(Value::String)
                .ok_or_else(|| Error::Validation("expected string".into())),
            other => Err(Error::Validation(format!(
                "expected string, got {}",
                type_name(other)
            ))),
        },
        FieldStorage::Integer => match value {
            Value::Number(n) if n.is_i64() || n.is_u64() => Ok(value.clone()),
            Value::Number(n) => n
                .as_f64()
                .and_then(|f| {
                    if f.fract() == 0.0 {
                        #[allow(clippy::cast_possible_truncation)]
                        Some(Value::Number((f as i64).into()))
                    } else {
                        None
                    }
                })
                .ok_or_else(|| Error::Validation("expected integer".into())),
            Value::String(s) => s
                .parse::<i64>()
                .map(|i| Value::Number(i.into()))
                .map_err(|_| Error::Validation(format!("expected integer, got string {s:?}"))),
            other => Err(Error::Validation(format!(
                "expected integer, got {}",
                type_name(other)
            ))),
        },
        FieldStorage::Decimal => match value {
            Value::Number(_) => Ok(value.clone()),
            Value::String(s) => s
                .parse::<f64>()
                .map(|f| Value::Number(serde_json::Number::from_f64(f).unwrap_or_else(|| 0.into())))
                .map_err(|_| Error::Validation("expected decimal".into())),
            other => Err(Error::Validation(format!(
                "expected decimal, got {}",
                type_name(other)
            ))),
        },
        FieldStorage::Boolean => match value {
            Value::Bool(_) => Ok(value.clone()),
            other => Err(Error::Validation(format!(
                "expected boolean, got {}",
                type_name(other)
            ))),
        },
        FieldStorage::Json | FieldStorage::Currency => match value {
            Value::Object(_) | Value::Array(_) => Ok(value.clone()),
            // Allow scalars stored in JSON columns (Mongo/Redis flexible).
            Value::String(_) | Value::Number(_) | Value::Bool(_) => Ok(value.clone()),
            other => Err(Error::Validation(format!(
                "expected json, got {}",
                type_name(other)
            ))),
        },
    }
}

/// Validate all fields in `content` against `layout` (extra keys allowed).
///
/// # Errors
///
/// Returns the first type mismatch.
pub fn validate_write_types(layout: &StorageLayout, content: &Value) -> Result<()> {
    let Some(obj) = content.as_object() else {
        return Err(Error::Validation("record must be a JSON object".into()));
    };
    for f in &layout.fields {
        if f.name == "id" {
            continue;
        }
        if let Some(v) = obj.get(&f.name) {
            coerce_for_storage(f.storage, v).map_err(|e| {
                Error::Validation(format!("field {}.{}: {e}", layout.table, f.name))
            })?;
        } else if !f.nullable && !f.primary_key {
            // Required fields may be filled by defaults elsewhere; do not hard-fail here.
        }
    }
    Ok(())
}

/// Encode a field value for SQL TEXT binding (JSON cells serialized).
pub fn sql_bind_text(storage: FieldStorage, value: &Value) -> Result<Option<String>> {
    if value.is_null() {
        return Ok(None);
    }
    let v = coerce_for_storage(storage, value)?;
    match storage {
        FieldStorage::Json | FieldStorage::Currency => Ok(Some(
            serde_json::to_string(&v).map_err(|e| Error::serialization_msg(e.to_string()))?,
        )),
        FieldStorage::Boolean => Ok(Some(if v.as_bool() == Some(true) {
            "1".into()
        } else {
            "0".into()
        })),
        _ => match v {
            Value::String(s) => Ok(Some(s)),
            Value::Number(n) => Ok(Some(n.to_string())),
            Value::Bool(b) => Ok(Some(b.to_string())),
            other => Ok(Some(
                serde_json::to_string(&other)
                    .map_err(|e| Error::serialization_msg(e.to_string()))?,
            )),
        },
    }
}

/// Decode a SQL TEXT/INTEGER cell into JSON for a field.
pub fn decode_sql_cell(storage: FieldStorage, raw: Option<&str>, int_val: Option<i64>) -> Value {
    match storage {
        FieldStorage::Integer => int_val
            .map(|i| Value::Number(i.into()))
            .or_else(|| {
                raw.and_then(|s| s.parse::<i64>().ok())
                    .map(|i| Value::Number(i.into()))
            })
            .unwrap_or(Value::Null),
        FieldStorage::Boolean => {
            if let Some(i) = int_val {
                Value::Bool(i != 0)
            } else if let Some(s) = raw {
                Value::Bool(s == "1" || s.eq_ignore_ascii_case("true"))
            } else {
                Value::Null
            }
        }
        FieldStorage::Json | FieldStorage::Currency => raw
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or(Value::Null),
        FieldStorage::Decimal => raw
            .and_then(|s| s.parse::<f64>().ok())
            .and_then(serde_json::Number::from_f64)
            .map(Value::Number)
            .or_else(|| int_val.map(|i| Value::Number(i.into())))
            .unwrap_or(Value::Null),
        FieldStorage::String | FieldStorage::Date => raw
            .map(|s| Value::String(s.to_string()))
            .unwrap_or(Value::Null),
    }
}

/// Fold a `{ "table", "id" }` record object into a canonical `table:id` string.
///
/// Accepts an object carrying at least `id`; when `table` is present the result is
/// `table:id`, otherwise the bare id is returned.
fn record_object_to_rid(o: &Map<String, Value>) -> Option<String> {
    let id = o.get("id").and_then(Value::as_str)?;
    match o.get("table").and_then(Value::as_str) {
        Some(table) if !table.is_empty() => Some(format!("{table}:{id}")),
        _ => Some(id.to_string()),
    }
}

fn type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// Look up layout field by name.
#[must_use]
pub fn field_by_name<'a>(layout: &'a StorageLayout, name: &str) -> Option<&'a LayoutField> {
    layout.fields.iter().find(|f| f.name == name)
}
