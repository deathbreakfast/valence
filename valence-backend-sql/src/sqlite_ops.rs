//! Shared SQL document backend logic for SQLite and Postgres.

use serde_json::{Map, Value};
use sqlx::Row;
use valence_core::compiled_query::CompiledQuery;
use valence_core::error::{Error, Result};
use valence_core::record_id::RecordId;

use crate::{ensure_table, json_merge, prepare_compiled, row_from_body, upsert_body_fields};

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

pub fn expand_body_rows(table: &str, rows: Vec<(String, String)>) -> Result<Vec<Value>> {
    rows.into_iter()
        .map(|(id, body_text)| {
            let body: Value =
                serde_json::from_str(&body_text).unwrap_or_else(|_| Value::Object(Map::new()));
            Ok(row_from_body(table, &id, body))
        })
        .collect()
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
            let body: String = r.try_get(1).unwrap_or_else(|_| "{}".into());
            (id, body)
        })
        .collect();
    expand_body_rows(table, pairs)
}

pub async fn ensure_edges_sqlite(pool: &sqlx::SqlitePool) -> Result<()> {
    sqlx::query(&crate::document::ensure_edges_table_ddl())
        .execute(pool)
        .await
        .map_err(|e| Error::Database(e.to_string()))?;
    Ok(())
}

pub async fn ensure_table_sqlite(pool: &sqlx::SqlitePool, table: &str) -> Result<()> {
    assert_safe_table(table)?;
    sqlx::query(&ensure_table(table))
        .execute(pool)
        .await
        .map_err(|e| Error::Database(e.to_string()))?;
    Ok(())
}

pub async fn get_record_sqlite(
    pool: &sqlx::SqlitePool,
    table: &str,
    id: &str,
) -> Result<Option<Value>> {
    ensure_table_sqlite(pool, table).await?;
    let q = format!("SELECT id, body FROM {table} WHERE id = ?");
    let row = sqlx::query(&q)
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(|e| Error::Database(e.to_string()))?;
    Ok(row.map(|r| {
        let id: String = r.get(0);
        let body: String = r.get(1);
        row_from_body(
            table,
            &id,
            serde_json::from_str(&body).unwrap_or_else(|_| Value::Object(Map::new())),
        )
    }))
}

pub async fn create_record_sqlite(
    pool: &sqlx::SqlitePool,
    table: &str,
    content: Value,
) -> Result<Value> {
    ensure_table_sqlite(pool, table).await?;
    let id = storage_id(&content).unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let mut body = upsert_body_fields(content);
    body.remove("id");
    let body_text = serde_json::to_string(&Value::Object(body))
        .map_err(|e| Error::Serialization(e.to_string()))?;
    let q = format!("INSERT INTO {table} (id, body) VALUES (?, ?)");
    sqlx::query(&q)
        .bind(&id)
        .bind(&body_text)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(e.to_string()))?;
    Ok(row_from_body(
        table,
        &id,
        serde_json::from_str(&body_text).unwrap_or_else(|_| Value::Object(Map::new())),
    ))
}

pub async fn update_record_sqlite(
    pool: &sqlx::SqlitePool,
    table: &str,
    id: &str,
    content: Value,
) -> Result<Value> {
    if get_record_sqlite(pool, table, id).await?.is_none() {
        return Err(Error::NotFound(format!("{table}:{id}")));
    }
    let mut body = upsert_body_fields(content);
    body.remove("id");
    let body_text = serde_json::to_string(&Value::Object(body))
        .map_err(|e| Error::Serialization(e.to_string()))?;
    let q = format!("UPDATE {table} SET body = ? WHERE id = ?");
    sqlx::query(&q)
        .bind(&body_text)
        .bind(id)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(e.to_string()))?;
    Ok(row_from_body(
        table,
        id,
        serde_json::from_str(&body_text).unwrap_or_else(|_| Value::Object(Map::new())),
    ))
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
        .map_err(|e| Error::Database(e.to_string()))?;
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
    .map_err(|e| Error::Database(e.to_string()))?;
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
    .map_err(|e| Error::Database(e.to_string()))?;
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
    .map_err(|e| Error::Database(e.to_string()))?;
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
    if !field.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(Error::Validation(format!("unsafe field: {field}")));
    }
    ensure_table_sqlite(pool, table).await?;
    let idx = format!("valence_unique_{table}_{field}");
    let q = format!(
        "CREATE UNIQUE INDEX IF NOT EXISTS {idx} ON {table} (json_extract(body, '$.{field}'))"
    );
    sqlx::query(&q)
        .execute(pool)
        .await
        .map_err(|e| Error::Database(e.to_string()))?;
    Ok(())
}

pub fn ttl_deferred() -> valence_core::ttl::BackendTtlCapability {
    valence_core::ttl::BackendTtlCapability::Deferred
}

#[allow(dead_code, clippy::unused_async)]
pub async fn apply_ttl_noop(
    _table: &str,
    _policy: &valence_core::ttl::SchemaTtlPolicy,
) -> Result<()> {
    Ok(())
}

pub const fn sql_capabilities(label: &'static str) -> valence_core::BackendCapabilities {
    valence_core::BackendCapabilities {
        supports_merge: true,
        supports_graph_edges: true,
        telemetry_label: label,
    }
}
