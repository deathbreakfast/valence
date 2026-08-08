//! Shared SQL document backend logic for SQLite and Postgres.

use serde_json::Value;
use sqlx::{Column, Row};
use valence_core::compiled_query::CompiledQuery;
use valence_core::error::{Error, Result};
use valence_core::record_id::RecordId;

use valence_core::storage_layout::StorageLayout;

use crate::json_merge;
use crate::prepare_compiled;
use crate::typed_table::{
    create_record_typed_sqlite, define_unique_index_column_sqlite, ensure_layout_for_write_sqlite,
    get_record_typed_sqlite, map_select_row_sqlite, update_record_typed_sqlite,
};
use crate::ensure_table;

/// Dialect-specific SQL fragments.
#[allow(dead_code)]
pub trait SqlDialect: Send + Sync + 'static {
    fn insert_or_ignore(&self) -> &'static str;
    fn body_column_type(&self) -> &'static str;
}

pub fn assert_safe_table(table: &str) -> Result<()> {
    if table.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        Ok(())
    } else {
        Err(Error::Validation(format!("unsafe table name: {table}")))
    }
}

pub fn storage_id(content: &Value) -> Option<String> {
    content.get("id").and_then(|v| {
        v.get("id")
            .and_then(|x| x.as_str())
            .map(str::to_string)
            .or_else(|| v.as_str().map(str::to_string))
    })
}

pub fn bind_json_value<'q>(
    query: sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments<'q>>,
    value: &Value,
) -> sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments<'q>> {
    match value {
        Value::Null => query.bind(None::<String>),
        Value::Bool(b) => query.bind(*b),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                query.bind(i)
            } else if let Some(f) = n.as_f64() {
                query.bind(f)
            } else {
                query.bind(n.to_string())
            }
        }
        Value::String(s) => query.bind(s.clone()),
        other => query.bind(other.to_string()),
    }
}

pub async fn execute_select_sqlite(
    pool: &sqlx::SqlitePool,
    compiled: &CompiledQuery,
    default_table: &str,
) -> Result<Vec<Value>> {
    let (sql, params) = prepare_compiled(compiled)?;
    let mut q = sqlx::query(&sql);
    for p in &params {
        q = bind_json_value(q, p);
    }
    let rows = match q.fetch_all(pool).await {
        Ok(rows) => rows,
        Err(e) if e.to_string().to_lowercase().contains("no such table") => return Ok(vec![]),
        Err(e) => return Err(Error::database(e.to_string())),
    };

    if sql.to_ascii_uppercase().contains("COUNT(") {
        let count = rows
            .first()
            .and_then(|r| r.try_get::<i64, _>(0).ok())
            .unwrap_or(0);
        return Ok(vec![Value::Number(count.into())]);
    }

    let table = compiled
        .query_string
        .split("FROM")
        .nth(1)
        .and_then(|s| s.split_whitespace().next())
        .unwrap_or(default_table);

    // Unique probes / id-only: SELECT id … with a single column.
    let upper = sql.to_ascii_uppercase();
    if upper.contains("SELECT ID")
        && !upper.contains("SELECT *")
        && rows
            .first()
            .map(|r| r.columns().len() == 1)
            .unwrap_or(false)
    {
        return Ok(rows
            .iter()
            .filter_map(|r| r.try_get::<String, _>(0).ok())
            .map(|id| serde_json::json!({ "id": id }))
            .collect());
    }

    Ok(rows
        .iter()
        .map(|r| map_select_row_sqlite(table, r))
        .collect())
}

pub async fn ensure_edges_sqlite(pool: &sqlx::SqlitePool) -> Result<()> {
    sqlx::query(&crate::document::ensure_edges_table_ddl())
        .execute(pool)
        .await
        .map_err(|e| Error::database(e.to_string()))?;
    Ok(())
}

pub async fn ensure_table_sqlite(pool: &sqlx::SqlitePool, table: &str) -> Result<()> {
    assert_safe_table(table)?;
    sqlx::query(&ensure_table(table))
        .execute(pool)
        .await
        .map_err(|e| Error::database(e.to_string()))?;
    Ok(())
}

pub async fn get_record_sqlite(
    pool: &sqlx::SqlitePool,
    table: &str,
    id: &str,
) -> Result<Option<Value>> {
    get_record_typed_sqlite(pool, table, id).await
}

pub async fn create_record_sqlite(
    pool: &sqlx::SqlitePool,
    table: &str,
    content: Value,
) -> Result<Value> {
    let layout = StorageLayout::resolve_for_write(table, &content)?;
    create_record_typed_sqlite(pool, &layout, content).await
}

pub async fn update_record_sqlite(
    pool: &sqlx::SqlitePool,
    table: &str,
    id: &str,
    content: Value,
) -> Result<Value> {
    let layout = StorageLayout::resolve_for_write(table, &content)?;
    update_record_typed_sqlite(pool, &layout, id, content).await
}

pub async fn merge_record_sqlite(
    pool: &sqlx::SqlitePool,
    table: &str,
    id: &str,
    patch: Value,
) -> Result<Value> {
    let existing = get_record_sqlite(pool, table, id)
        .await?
        .ok_or_else(|| Error::NotFound(format!("{table}:{id}")))?;
    let mut base = existing.as_object().cloned().unwrap_or_default();
    base.remove("id");
    if let Some(patch_obj) = patch.as_object() {
        json_merge(&mut base, patch_obj);
    }
    update_record_sqlite(pool, table, id, Value::Object(base)).await
}

pub async fn delete_record_sqlite(pool: &sqlx::SqlitePool, table: &str, id: &str) -> Result<()> {
    let q = format!("DELETE FROM {table} WHERE id = ?");
    sqlx::query(&q)
        .bind(id)
        .execute(pool)
        .await
        .map_err(|e| Error::database(e.to_string()))?;
    Ok(())
}

pub async fn relate_edge_sqlite(
    pool: &sqlx::SqlitePool,
    from: &RecordId,
    edge_table: &str,
    to: &RecordId,
) -> Result<()> {
    ensure_edges_sqlite(pool).await?;
    sqlx::query(
        "INSERT OR IGNORE INTO valence_edges (from_table, from_id, edge_type, to_table, to_id) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(from.table())
    .bind(from.id())
    .bind(edge_table)
    .bind(to.table())
    .bind(to.id())
    .execute(pool)
    .await
    .map_err(|e| Error::database(e.to_string()))?;
    Ok(())
}

pub async fn unrelate_edge_sqlite(
    pool: &sqlx::SqlitePool,
    from: &RecordId,
    edge_table: &str,
    to: &RecordId,
) -> Result<()> {
    sqlx::query(
        "DELETE FROM valence_edges WHERE from_table = ? AND from_id = ? AND edge_type = ? \
         AND to_table = ? AND to_id = ?",
    )
    .bind(from.table())
    .bind(from.id())
    .bind(edge_table)
    .bind(to.table())
    .bind(to.id())
    .execute(pool)
    .await
    .map_err(|e| Error::database(e.to_string()))?;
    Ok(())
}

pub async fn get_edge_targets_sqlite(
    pool: &sqlx::SqlitePool,
    from: &RecordId,
    edge_table: &str,
) -> Result<Vec<RecordId>> {
    let rows = sqlx::query(
        "SELECT to_table, to_id FROM valence_edges \
         WHERE from_table = ? AND from_id = ? AND edge_type = ?",
    )
    .bind(from.table())
    .bind(from.id())
    .bind(edge_table)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::database(e.to_string()))?;
    Ok(rows
        .iter()
        .map(|r| RecordId::new(r.get::<String, _>(0), r.get::<String, _>(1)))
        .collect())
}

pub async fn get_edge_sources_sqlite(
    pool: &sqlx::SqlitePool,
    to: &RecordId,
    edge_table: &str,
) -> Result<Vec<RecordId>> {
    let rows = sqlx::query(
        "SELECT from_table, from_id FROM valence_edges \
         WHERE to_table = ? AND to_id = ? AND edge_type = ?",
    )
    .bind(to.table())
    .bind(to.id())
    .bind(edge_table)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::database(e.to_string()))?;
    Ok(rows
        .iter()
        .map(|r| RecordId::new(r.get::<String, _>(0), r.get::<String, _>(1)))
        .collect())
}

pub async fn define_unique_index_sqlite(
    pool: &sqlx::SqlitePool,
    table: &str,
    field: &str,
) -> Result<()> {
    assert_safe_table(table)?;
    valence_core::safe_ident::assert_safe_ident(field)?;
    // Ensure column exists: registry layout or placeholder table + column add via sync.
    if let Ok(layout) = StorageLayout::from_registry_table(table) {
        ensure_layout_for_write_sqlite(pool, &layout).await?;
    } else {
        ensure_table_sqlite(pool, table).await?;
        let layout = StorageLayout {
            table: table.to_string(),
            fields: vec![
                valence_core::storage_layout::LayoutField {
                    name: "id".into(),
                    storage: valence_core::storage_layout::FieldStorage::String,
                    primary_key: true,
                    nullable: false,
                    unique: true,
                    indexed: false,
                },
                valence_core::storage_layout::LayoutField {
                    name: field.to_string(),
                    storage: valence_core::storage_layout::FieldStorage::String,
                    primary_key: false,
                    nullable: true,
                    unique: true,
                    indexed: false,
                },
            ],
        };
        ensure_layout_for_write_sqlite(pool, &layout).await?;
    }
    define_unique_index_column_sqlite(pool, table, field).await
}

pub fn ttl_deferred() -> valence_core::ttl::BackendTtlCapability {
    valence_core::ttl::BackendTtlCapability::Deferred
}

/// Idempotent non-unique index on `__valence_expire_at` for platform TTL sweep discovery.
pub async fn apply_ttl_policy_sqlite(
    pool: &sqlx::SqlitePool,
    table: &str,
    _policy: &valence_core::ttl::SchemaTtlPolicy,
) -> Result<()> {
    assert_safe_table(table)?;
    if let Ok(layout) = StorageLayout::from_registry_table(table) {
        ensure_layout_for_write_sqlite(pool, &layout).await?;
    } else {
        ensure_table_sqlite(pool, table).await?;
        let field = valence_core::ttl::EXPIRE_AT_FIELD;
        let layout = StorageLayout {
            table: table.to_string(),
            fields: vec![
                valence_core::storage_layout::LayoutField {
                    name: "id".into(),
                    storage: valence_core::storage_layout::FieldStorage::String,
                    primary_key: true,
                    nullable: false,
                    unique: true,
                    indexed: false,
                },
                valence_core::storage_layout::LayoutField {
                    name: field.to_string(),
                    storage: valence_core::storage_layout::FieldStorage::String,
                    primary_key: false,
                    nullable: true,
                    unique: false,
                    indexed: true,
                },
            ],
        };
        ensure_layout_for_write_sqlite(pool, &layout).await?;
    }
    let field = valence_core::ttl::EXPIRE_AT_FIELD;
    valence_core::safe_ident::assert_safe_ident(field)?;
    let idx = format!("valence_ttl_expire_at_{table}");
    let q = format!("CREATE INDEX IF NOT EXISTS {idx} ON {table} ({field})");
    sqlx::query(&q)
        .execute(pool)
        .await
        .map_err(|e| Error::database(e.to_string()))?;
    Ok(())
}

pub const fn sql_capabilities(label: &'static str) -> valence_core::BackendCapabilities {
    valence_core::BackendCapabilities {
        supports_merge: true,
        supports_graph_edges: true,
        telemetry_label: label,
    }
}
