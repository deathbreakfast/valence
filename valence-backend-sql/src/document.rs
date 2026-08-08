//! Shared SQL helpers (edges + legacy row shaping).
//!
//! Document-era `(id, body)` tables are gone — typed columns live in [`crate::typed_table`].

use serde_json::{Map, Value};

/// Primary key column name.
pub const ID_COLUMN: &str = "id";

/// Edge junction table shared by SQL backends.
pub const EDGES_TABLE: &str = "valence_edges";

/// Minimal DDL for schemaless/ad-hoc tables (`id` only; fields added on write).
pub fn ensure_table_ddl(table: &str) -> String {
    format!("CREATE TABLE IF NOT EXISTS {table} ({ID_COLUMN} TEXT PRIMARY KEY NOT NULL)")
}

/// DDL for the shared edge junction table.
pub fn ensure_edges_table_ddl() -> String {
    format!(
        "CREATE TABLE IF NOT EXISTS {EDGES_TABLE} (\
         from_table TEXT NOT NULL, \
         from_id TEXT NOT NULL, \
         edge_type TEXT NOT NULL, \
         to_table TEXT NOT NULL, \
         to_id TEXT NOT NULL, \
         PRIMARY KEY (from_table, from_id, edge_type, to_table, to_id))"
    )
}

/// Ensure a table exists (caller runs DDL via sqlx).
pub fn ensure_table(table: &str) -> String {
    ensure_table_ddl(table)
}

/// Build a JSON row object from flat fields + id (Valence wire shape).
pub fn row_from_body(table: &str, id: &str, body: Value) -> Value {
    let mut obj = match body {
        Value::Object(map) => map,
        _ => Map::new(),
    };
    obj.insert(
        "id".into(),
        Value::Object(Map::from_iter([
            ("table".into(), Value::String(table.to_string())),
            ("id".into(), Value::String(id.to_string())),
        ])),
    );
    Value::Object(obj)
}

/// Merge content fields into a map for insert/update.
pub fn upsert_body_fields(content: Value) -> Map<String, Value> {
    match content {
        Value::Object(map) => map,
        _ => Map::new(),
    }
}
