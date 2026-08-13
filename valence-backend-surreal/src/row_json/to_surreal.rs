//! Convert model JSON into Surreal write content.

use std::collections::BTreeMap;

use surrealdb::types::{Array, Number, Object, Value};

/// Convert model JSON into Surreal write content.
///
/// [`crate::RecordId`] serializes as `{ "table", "id" }` objects; Valence stores record
/// links as canonical `table:id` strings on every engine (including Surreal SCHEMAFULL
/// `TYPE string`), so those objects become wire strings here. `"table:id"` strings pass
/// through unchanged.
pub fn json_to_surreal_content_value(v: serde_json::Value) -> Value {
    match v {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Bool(b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Number(Number::Int(i))
            } else if let Some(f) = n.as_f64() {
                Value::Number(Number::Float(f))
            } else {
                Value::String(n.to_string())
            }
        }
        serde_json::Value::String(s) => Value::String(s),
        serde_json::Value::Array(items) => Value::Array(Array::from(
            items
                .into_iter()
                .map(json_to_surreal_content_value)
                .collect::<Vec<_>>(),
        )),
        serde_json::Value::Object(map) => {
            if let Some(wire) = record_wire_from_json_object(&map) {
                return Value::String(wire);
            }
            let mut out = BTreeMap::new();
            for (k, val) in map {
                out.insert(k, json_to_surreal_content_value(val));
            }
            Value::Object(Object::from(out))
        }
    }
}

fn record_wire_from_json_object(
    map: &serde_json::Map<String, serde_json::Value>,
) -> Option<String> {
    if map.len() != 2 {
        return None;
    }
    let table = map.get("table")?.as_str()?;
    let id = map.get("id")?.as_str()?;
    if table.is_empty()
        || id.is_empty()
        || !table.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return None;
    }
    Some(format!("{table}:{id}"))
}
