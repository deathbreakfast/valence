//! SurrealDB row deserialization to JSON shared by backends.

mod from_surreal;
mod projection;
mod to_surreal;
mod types;

pub use from_surreal::{
    decode_query_response_rows_to_json, map_looks_like_surreal_thing_only,
    record_map_to_json_object, select_record_json, thing_only_key_from_tb_id_map, thing_to_id_only,
    try_value_as_record_map,
};
pub use projection::{
    decode_query_value_projection_rows_to_json, is_select_value_projection_query,
};
pub use to_surreal::json_to_surreal_content_value;

use crate::error::db_err;
use surrealdb::{Connection, Surreal};
use valence_core::error::{Error, Result};

/// Ensure a schemaless table exists (SurrealDB v3 requires explicit table definition).
///
/// Prefer [`ensure_typed_table`] for schema-backed models (SCHEMAFULL + DEFINE FIELD).
pub async fn ensure_schemaless_table<C>(db: &Surreal<C>, table: &str) -> Result<()>
where
    C: Connection,
{
    if table.is_empty() || !table.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(Error::Validation(format!("Invalid table name: {table}")));
    }
    // Ad-hoc / contract tables (no registry layout). Schema models use ensure_typed_table.
    let q = format!("DEFINE TABLE IF NOT EXISTS `{table}` SCHEMALESS");
    db.query(&q).await.map_err(db_err)?;
    Ok(())
}

/// Ensure SCHEMAFULL table + DEFINE FIELD for each layout field.
pub async fn ensure_typed_table<C>(
    db: &Surreal<C>,
    layout: &valence_core::storage_layout::StorageLayout,
) -> Result<()>
where
    C: Connection,
{
    let ddl = layout.to_ddl(valence_core::KnownEngines::SURREALDB)?;
    for stmt in ddl.split(';').map(str::trim).filter(|s| !s.is_empty()) {
        db.query(stmt).await.map_err(db_err)?;
    }
    Ok(())
}

/// Additive DEFINE FIELD for missing layout fields (Surreal has no column inspect here —
/// DEFINE FIELD IF NOT EXISTS is idempotent).
pub async fn sync_typed_table<C>(
    db: &Surreal<C>,
    layout: &valence_core::storage_layout::StorageLayout,
) -> Result<()>
where
    C: Connection,
{
    ensure_typed_table(db, layout).await
}
