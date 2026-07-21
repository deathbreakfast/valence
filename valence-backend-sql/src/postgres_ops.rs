//! Postgres-specific SQL document operations.

use serde_json::{Map, Value};
use sqlx::postgres::PgPool;
use sqlx::Row;
use valence_core::error::{Error, Result};
use valence_core::record_id::RecordId;

use crate::query::prepare_compiled_postgres;
use crate::sqlite_ops::{assert_safe_table, expand_body_rows, storage_id};
use crate::{json_merge, row_from_body, upsert_body_fields};

pub fn ensure_table_ddl_postgres(table: &str) -> String {
    format!(
        "CREATE TABLE IF NOT EXISTS {table} (\
         id TEXT PRIMARY KEY NOT NULL, \
         body JSONB NOT NULL DEFAULT '{{}}'::jsonb)"
    )
}

pub async fn ensure_edges_postgres(pool: &PgPool) -> Result<()> {
    sqlx::query(&crate::document::ensure_edges_table_ddl())
        .execute(pool)
        .await
        .map_err(|e| Error::Database(e.to_string()))?;
    Ok(())
}

pub async fn ensure_table_postgres(pool: &PgPool, table: &str) -> Result<()> {
    assert_safe_table(table)?;
    sqlx::query(&ensure_table_ddl_postgres(table))
        .execute(pool)
        .await
        .map_err(|e| Error::Database(e.to_string()))?;
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
        Err(e) => return Err(Error::Database(e.to_string())),
    };

    if sql.contains("COUNT(") {
        let count = rows
            .first()
            .and_then(|r| r.try_get::<i64, _>(0).ok())
            .unwrap_or(0);
        return Ok(vec![Value::Number(count.into())]);
    }

    if sql.contains("SELECT id") && !sql.contains("body") {
        // Return object rows so callers can deserialize as `IdOnlyRecord` / model types.
        return Ok(rows
            .iter()
            .filter_map(|r| r.try_get::<String, _>(0).ok())
            .map(|id| serde_json::json!({ "id": id }))
            .collect());
    }

    let table = compiled
        .query_string
        .split("FROM")
        .nth(1)
        .and_then(|s| s.split_whitespace().next())
        .unwrap_or(default_table);

    let pairs: Vec<(String, String)> = rows
        .iter()
        .map(|r| {
            let id: String = r.try_get(0).unwrap_or_default();
            let body: Value = r.try_get(1).unwrap_or_else(|_| Value::Object(Map::new()));
            (id, body.to_string())
        })
        .collect();
    expand_body_rows(table, pairs)
}

pub async fn get_record_postgres(pool: &PgPool, table: &str, id: &str) -> Result<Option<Value>> {
    ensure_table_postgres(pool, table).await?;
    let q = format!("SELECT id, body FROM {table} WHERE id = $1");
    let row = sqlx::query(&q)
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(|e| Error::Database(e.to_string()))?;
    Ok(row.map(|r| {
        let id: String = r.get(0);
        let body: Value = r.get(1);
        row_from_body(table, &id, body)
    }))
}

pub async fn create_record_postgres(pool: &PgPool, table: &str, content: Value) -> Result<Value> {
    ensure_table_postgres(pool, table).await?;
    let id = storage_id(&content).unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let mut body = upsert_body_fields(content);
    body.remove("id");
    let body_val = Value::Object(body);
    let q = format!("INSERT INTO {table} (id, body) VALUES ($1, $2)");
    sqlx::query(&q)
        .bind(&id)
        .bind(&body_val)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(e.to_string()))?;
    Ok(row_from_body(table, &id, body_val))
}

pub async fn update_record_postgres(
    pool: &PgPool,
    table: &str,
    id: &str,
    content: Value,
) -> Result<Value> {
    if get_record_postgres(pool, table, id).await?.is_none() {
        return Err(Error::NotFound(format!("{table}:{id}")));
    }
    let mut body = upsert_body_fields(content);
    body.remove("id");
    let body_val = Value::Object(body);
    let q = format!("UPDATE {table} SET body = $1 WHERE id = $2");
    sqlx::query(&q)
        .bind(&body_val)
        .bind(id)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(e.to_string()))?;
    Ok(row_from_body(table, id, body_val))
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
        .map_err(|e| Error::Database(e.to_string()))?;
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
    .map_err(|e| Error::Database(e.to_string()))?;
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
    .map_err(|e| Error::Database(e.to_string()))?;
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
    .map_err(|e| Error::Database(e.to_string()))?;
    Ok(rows
        .iter()
        .map(|r| RecordId::new(r.get::<String, _>(0), r.get::<String, _>(1)))
        .collect())
}

pub async fn define_unique_index_postgres(pool: &PgPool, table: &str, field: &str) -> Result<()> {
    assert_safe_table(table)?;
    ensure_table_postgres(pool, table).await?;
    let idx = format!("valence_unique_{table}_{field}");
    let q = format!("CREATE UNIQUE INDEX IF NOT EXISTS {idx} ON {table} ((body->>'{field}'))");
    sqlx::query(&q)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(e.to_string()))?;
    Ok(())
}
