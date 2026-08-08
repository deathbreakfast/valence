//! Postgres-specific SQL document operations.

use serde_json::Value;
use sqlx::postgres::PgPool;
use sqlx::{Column, Row};
use valence_core::error::{Error, Result};
use valence_core::record_id::RecordId;
use valence_core::storage_layout::StorageLayout;

use crate::query::prepare_compiled_postgres;
use crate::sqlite_ops::assert_safe_table;
use crate::typed_table::{
    create_record_typed_postgres, define_unique_index_column_postgres,
    ensure_layout_for_write_postgres, get_record_typed_postgres, map_select_row_postgres,
    update_record_typed_postgres,
};
use crate::json_merge;

pub fn ensure_table_ddl_postgres(table: &str) -> String {
    format!("CREATE TABLE IF NOT EXISTS {table} (id TEXT PRIMARY KEY NOT NULL)")
}

pub async fn ensure_edges_postgres(pool: &PgPool) -> Result<()> {
    sqlx::query(&crate::document::ensure_edges_table_ddl())
        .execute(pool)
        .await
        .map_err(|e| Error::database(e.to_string()))?;
    Ok(())
}

pub async fn ensure_table_postgres(pool: &PgPool, table: &str) -> Result<()> {
    assert_safe_table(table)?;
    sqlx::query(&ensure_table_ddl_postgres(table))
        .execute(pool)
        .await
        .map_err(|e| Error::database(e.to_string()))?;
    Ok(())
}

fn bind_pg<'q>(
    query: sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>,
    value: &Value,
) -> sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments> {
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

pub async fn execute_select_postgres(
    pool: &PgPool,
    compiled: &valence_core::compiled_query::CompiledQuery,
    default_table: &str,
) -> Result<Vec<Value>> {
    let (sql, params) = prepare_compiled_postgres(compiled)?;
    let mut q = sqlx::query(&sql);
    for p in &params {
        q = bind_pg(q, p);
    }
    let rows = match q.fetch_all(pool).await {
        Ok(rows) => rows,
        // 42P01 undefined_table: read-only compiled queries on a missing table
        // return empty, matching the sqlite adapter's "no such table" behavior.
        Err(e)
            if e.as_database_error()
                .and_then(|d| d.code())
                .is_some_and(|code| code == "42P01") =>
        {
            return Ok(vec![]);
        }
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
        .map(|r| map_select_row_postgres(table, r))
        .collect())
}

pub async fn get_record_postgres(pool: &PgPool, table: &str, id: &str) -> Result<Option<Value>> {
    get_record_typed_postgres(pool, table, id).await
}

pub async fn create_record_postgres(pool: &PgPool, table: &str, content: Value) -> Result<Value> {
    let layout = StorageLayout::resolve_for_write(table, &content)?;
    create_record_typed_postgres(pool, &layout, content).await
}

pub async fn update_record_postgres(
    pool: &PgPool,
    table: &str,
    id: &str,
    content: Value,
) -> Result<Value> {
    let layout = StorageLayout::resolve_for_write(table, &content)?;
    update_record_typed_postgres(pool, &layout, id, content).await
}

pub async fn merge_record_postgres(
    pool: &PgPool,
    table: &str,
    id: &str,
    patch: Value,
) -> Result<Value> {
    let existing = get_record_postgres(pool, table, id)
        .await?
        .ok_or_else(|| Error::NotFound(format!("{table}:{id}")))?;
    let mut base = existing.as_object().cloned().unwrap_or_default();
    base.remove("id");
    if let Some(patch_obj) = patch.as_object() {
        json_merge(&mut base, patch_obj);
    }
    update_record_postgres(pool, table, id, Value::Object(base)).await
}

pub async fn delete_record_postgres(pool: &PgPool, table: &str, id: &str) -> Result<()> {
    let q = format!("DELETE FROM {table} WHERE id = $1");
    sqlx::query(&q)
        .bind(id)
        .execute(pool)
        .await
        .map_err(|e| Error::database(e.to_string()))?;
    Ok(())
}

pub async fn relate_edge_postgres(
    pool: &PgPool,
    from: &RecordId,
    edge_table: &str,
    to: &RecordId,
) -> Result<()> {
    ensure_edges_postgres(pool).await?;
    sqlx::query(
        "INSERT INTO valence_edges (from_table, from_id, edge_type, to_table, to_id) \
         VALUES ($1, $2, $3, $4, $5) ON CONFLICT DO NOTHING",
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

pub async fn unrelate_edge_postgres(
    pool: &PgPool,
    from: &RecordId,
    edge_table: &str,
    to: &RecordId,
) -> Result<()> {
    sqlx::query(
        "DELETE FROM valence_edges WHERE from_table = $1 AND from_id = $2 AND edge_type = $3 \
         AND to_table = $4 AND to_id = $5",
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

pub async fn get_edge_targets_postgres(
    pool: &PgPool,
    from: &RecordId,
    edge_table: &str,
) -> Result<Vec<RecordId>> {
    let rows = sqlx::query(
        "SELECT to_table, to_id FROM valence_edges \
         WHERE from_table = $1 AND from_id = $2 AND edge_type = $3",
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

pub async fn get_edge_sources_postgres(
    pool: &PgPool,
    to: &RecordId,
    edge_table: &str,
) -> Result<Vec<RecordId>> {
    let rows = sqlx::query(
        "SELECT from_table, from_id FROM valence_edges \
         WHERE to_table = $1 AND to_id = $2 AND edge_type = $3",
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

pub async fn define_unique_index_postgres(pool: &PgPool, table: &str, field: &str) -> Result<()> {
    assert_safe_table(table)?;
    valence_core::safe_ident::assert_safe_ident(field)?;
    if let Ok(layout) = StorageLayout::from_registry_table(table) {
        ensure_layout_for_write_postgres(pool, &layout).await?;
    } else {
        ensure_table_postgres(pool, table).await?;
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
        ensure_layout_for_write_postgres(pool, &layout).await?;
    }
    define_unique_index_column_postgres(pool, table, field).await
}

/// Idempotent non-unique index on `__valence_expire_at` for platform TTL sweep discovery.
pub async fn apply_ttl_policy_postgres(
    pool: &PgPool,
    table: &str,
    _policy: &valence_core::ttl::SchemaTtlPolicy,
) -> Result<()> {
    assert_safe_table(table)?;
    if let Ok(layout) = StorageLayout::from_registry_table(table) {
        ensure_layout_for_write_postgres(pool, &layout).await?;
    } else {
        ensure_table_postgres(pool, table).await?;
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
        ensure_layout_for_write_postgres(pool, &layout).await?;
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
