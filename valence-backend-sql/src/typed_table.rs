//! Typed SQL table DDL and row encode/decode (schema-driven columns).

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use serde_json::{Map, Value};
use sqlx::{Column, Row};
use valence_core::error::{Error, Result};
use valence_core::schema::SchemaRegistry;
use valence_core::storage_layout::{
    decode_sql_cell, layout_diff, postgres_add_column, postgres_drop_default, postgres_set_default,
    postgres_set_not_null, postgres_set_nullable, row_from_columns, split_record_fields,
    sql_bind_text, sqlite_add_column, validate_write_types, AdditiveOp, FieldStorage, LayoutField,
    SafeTweak, StorageLayout,
};
use valence_core::KnownEngines;

use crate::sqlite_ops::assert_safe_table;

/// Edge junction table (unchanged).
pub use crate::document::{ensure_edges_table_ddl, EDGES_TABLE, ID_COLUMN};

/// Per-backend cache of tables/fields already ensured for writes.
///
/// Skips inspect when every field in the requested layout was already ensured for that table.
#[derive(Debug, Default, Clone)]
pub struct WriteEnsureCache {
    /// table → field names covered by a prior ensure.
    inner: Arc<Mutex<HashMap<String, HashSet<String>>>>,
}

impl WriteEnsureCache {
    /// Empty cache for a new backend connection.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn covers(&self, table: &str, layout: &StorageLayout) -> bool {
        let guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let Some(known) = guard.get(table) else {
            return false;
        };
        layout.fields.iter().all(|f| known.contains(&f.name))
    }

    fn record(&self, layout: &StorageLayout) {
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let entry = guard.entry(layout.table.clone()).or_default();
        for f in &layout.fields {
            entry.insert(f.name.clone());
        }
    }
}

fn registry_table(table: &str) -> bool {
    SchemaRegistry::global().get_full_schema(table).is_some()
}

/// Create-table DDL for SQLite.
pub fn ensure_typed_ddl_sqlite(layout: &StorageLayout) -> Result<String> {
    layout.to_ddl(KnownEngines::SQLITE)
}

/// Create-table DDL for Postgres.
pub fn ensure_typed_ddl_postgres(layout: &StorageLayout) -> Result<String> {
    layout.to_ddl(KnownEngines::POSTGRES)
}

/// Ensure typed table exists (SQLite).
pub async fn ensure_typed_table_sqlite(
    pool: &sqlx::SqlitePool,
    layout: &StorageLayout,
) -> Result<()> {
    assert_safe_table(&layout.table)?;
    let ddl = ensure_typed_ddl_sqlite(layout)?;
    sqlx::query(&ddl)
        .execute(pool)
        .await
        .map_err(|e| Error::database(e.to_string()))?;
    for f in &layout.fields {
        if f.unique && f.name != "id" {
            define_unique_index_column_sqlite(pool, &layout.table, &f.name).await?;
        } else if f.indexed && f.name != "id" {
            define_index_column_sqlite(pool, &layout.table, &f.name).await?;
        }
    }
    Ok(())
}

/// Ensure typed table exists (Postgres).
pub async fn ensure_typed_table_postgres(
    pool: &sqlx::PgPool,
    layout: &StorageLayout,
) -> Result<()> {
    assert_safe_table(&layout.table)?;
    let ddl = ensure_typed_ddl_postgres(layout)?;
    sqlx::query(&ddl)
        .execute(pool)
        .await
        .map_err(|e| Error::database(e.to_string()))?;
    for f in &layout.fields {
        if f.unique && f.name != "id" {
            define_unique_index_column_postgres(pool, &layout.table, &f.name).await?;
        } else if f.indexed && f.name != "id" {
            define_index_column_postgres(pool, &layout.table, &f.name).await?;
        }
    }
    Ok(())
}

/// Inspect live SQLite columns → [`StorageLayout`] (storage kinds inferred coarsely).
pub async fn inspect_typed_layout_sqlite(
    pool: &sqlx::SqlitePool,
    table: &str,
) -> Result<Option<StorageLayout>> {
    assert_safe_table(table)?;
    let rows = sqlx::query(&format!("PRAGMA table_info({table})"))
        .fetch_all(pool)
        .await
        .map_err(|e| Error::database(e.to_string()))?;
    if rows.is_empty() {
        return Ok(None);
    }
    let mut fields = Vec::new();
    for r in rows {
        let name: String = r
            .try_get("name")
            .map_err(|e| Error::database(e.to_string()))?;
        let ctype: String = r
            .try_get::<String, _>("type")
            .unwrap_or_default()
            .to_ascii_uppercase();
        let notnull: i64 = r.try_get("notnull").unwrap_or(0);
        let pk: i64 = r.try_get("pk").unwrap_or(0);
        let storage = match ctype.as_str() {
            "INTEGER" => FieldStorage::Integer,
            "REAL" => FieldStorage::Decimal,
            "BOOLEAN" => FieldStorage::Boolean,
            _ => FieldStorage::String,
        };
        fields.push(LayoutField {
            name,
            storage,
            primary_key: pk != 0,
            nullable: notnull == 0 && pk == 0,
            unique: false,
            indexed: false,
            default: None,
        });
    }
    Ok(Some(StorageLayout {
        table: table.to_string(),
        fields,
    }))
}

/// Inspect live Postgres columns.
pub async fn inspect_typed_layout_postgres(
    pool: &sqlx::PgPool,
    table: &str,
) -> Result<Option<StorageLayout>> {
    assert_safe_table(table)?;
    let rows = sqlx::query(
        "SELECT column_name, data_type, is_nullable \
         FROM information_schema.columns \
         WHERE table_schema = 'public' AND table_name = $1 \
         ORDER BY ordinal_position",
    )
    .bind(table)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::database(e.to_string()))?;
    if rows.is_empty() {
        return Ok(None);
    }
    let mut fields = Vec::new();
    for r in rows {
        let name: String = r
            .try_get("column_name")
            .map_err(|e| Error::database(e.to_string()))?;
        let dtype: String = r.try_get("data_type").unwrap_or_default();
        let nullable: String = r.try_get("is_nullable").unwrap_or_else(|_| "YES".into());
        let storage = match dtype.as_str() {
            "bigint" | "integer" | "smallint" => FieldStorage::Integer,
            "boolean" => FieldStorage::Boolean,
            "double precision" | "real" | "numeric" => FieldStorage::Decimal,
            "jsonb" | "json" => FieldStorage::Json,
            _ => FieldStorage::String,
        };
        fields.push(LayoutField {
            primary_key: name == "id",
            name,
            storage,
            nullable: nullable == "YES",
            unique: false,
            indexed: false,
            default: None,
        });
    }
    Ok(Some(StorageLayout {
        table: table.to_string(),
        fields,
    }))
}

/// Additive sync for SQLite.
pub async fn sync_typed_table_sqlite(
    pool: &sqlx::SqlitePool,
    layout: &StorageLayout,
) -> Result<()> {
    let live = inspect_typed_layout_sqlite(pool, &layout.table).await?;
    if live.is_none() {
        return ensure_typed_table_sqlite(pool, layout).await;
    }
    let live = live.unwrap();
    // Inspect does not know unique/index flags — treat live unique/indexed as false so
    // desired unique/index still emit CREATE INDEX IF NOT EXISTS.
    let diff = layout_diff(layout, &live, KnownEngines::SQLITE)?;
    for op in diff.ops {
        match op {
            AdditiveOp::AddField(f) => {
                let ddl = sqlite_add_column(&layout.table, &f)?;
                sqlx::query(&ddl)
                    .execute(pool)
                    .await
                    .map_err(|e| Error::database(e.to_string()))?;
                if f.unique {
                    define_unique_index_column_sqlite(pool, &layout.table, &f.name).await?;
                } else if f.indexed {
                    define_index_column_sqlite(pool, &layout.table, &f.name).await?;
                }
            }
            AdditiveOp::AddUniqueIndex { field } => {
                define_unique_index_column_sqlite(pool, &layout.table, &field).await?;
            }
            AdditiveOp::AddIndex { field } => {
                define_index_column_sqlite(pool, &layout.table, &field).await?;
            }
        }
    }
    if !diff.tweaks.is_empty() {
        return Err(Error::Validation(format!(
            "sqlite sync produced {} safe tweaks (unsupported)",
            diff.tweaks.len()
        )));
    }
    Ok(())
}

/// Additive sync for Postgres (includes safe nullability/default tweaks).
pub async fn sync_typed_table_postgres(pool: &sqlx::PgPool, layout: &StorageLayout) -> Result<()> {
    let live = inspect_typed_layout_postgres(pool, &layout.table).await?;
    if live.is_none() {
        return ensure_typed_table_postgres(pool, layout).await;
    }
    let live = live.unwrap();
    let diff = layout_diff(layout, &live, KnownEngines::POSTGRES)?;
    for op in diff.ops {
        match op {
            AdditiveOp::AddField(f) => {
                let ddl = postgres_add_column(&layout.table, &f)?;
                sqlx::query(&ddl)
                    .execute(pool)
                    .await
                    .map_err(|e| Error::database(e.to_string()))?;
                if f.unique {
                    define_unique_index_column_postgres(pool, &layout.table, &f.name).await?;
                } else if f.indexed {
                    define_index_column_postgres(pool, &layout.table, &f.name).await?;
                }
            }
            AdditiveOp::AddUniqueIndex { field } => {
                define_unique_index_column_postgres(pool, &layout.table, &field).await?;
            }
            AdditiveOp::AddIndex { field } => {
                define_index_column_postgres(pool, &layout.table, &field).await?;
            }
        }
    }
    for tweak in diff.tweaks {
        let ddl = match &tweak {
            SafeTweak::SetNullable { field } => postgres_set_nullable(&layout.table, field)?,
            SafeTweak::SetNotNull { field } => postgres_set_not_null(&layout.table, field)?,
            SafeTweak::SetDefault { field, value } => {
                postgres_set_default(&layout.table, field, value)?
            }
            SafeTweak::DropDefault { field } => postgres_drop_default(&layout.table, field)?,
        };
        tracing::info!(
            target: "valence_storage",
            table = layout.table.as_str(),
            ?tweak,
            "valence.storage.safe_tweak"
        );
        sqlx::query(&ddl)
            .execute(pool)
            .await
            .map_err(|e| Error::database(e.to_string()))?;
    }
    Ok(())
}

pub async fn define_unique_index_column_sqlite(
    pool: &sqlx::SqlitePool,
    table: &str,
    field: &str,
) -> Result<()> {
    assert_safe_table(table)?;
    valence_core::safe_ident::assert_safe_ident(field)?;
    let idx = format!("valence_unique_{table}_{field}");
    let q = format!("CREATE UNIQUE INDEX IF NOT EXISTS {idx} ON {table} ({field})");
    sqlx::query(&q)
        .execute(pool)
        .await
        .map_err(|e| Error::database(e.to_string()))?;
    Ok(())
}

pub async fn define_index_column_sqlite(
    pool: &sqlx::SqlitePool,
    table: &str,
    field: &str,
) -> Result<()> {
    assert_safe_table(table)?;
    valence_core::safe_ident::assert_safe_ident(field)?;
    let idx = format!("valence_idx_{table}_{field}");
    let q = format!("CREATE INDEX IF NOT EXISTS {idx} ON {table} ({field})");
    sqlx::query(&q)
        .execute(pool)
        .await
        .map_err(|e| Error::database(e.to_string()))?;
    Ok(())
}

pub async fn define_unique_index_column_postgres(
    pool: &sqlx::PgPool,
    table: &str,
    field: &str,
) -> Result<()> {
    assert_safe_table(table)?;
    valence_core::safe_ident::assert_safe_ident(field)?;
    let idx = format!("valence_unique_{table}_{field}");
    let q = format!("CREATE UNIQUE INDEX IF NOT EXISTS {idx} ON {table} ({field})");
    sqlx::query(&q)
        .execute(pool)
        .await
        .map_err(|e| Error::database(e.to_string()))?;
    Ok(())
}

pub async fn define_index_column_postgres(
    pool: &sqlx::PgPool,
    table: &str,
    field: &str,
) -> Result<()> {
    assert_safe_table(table)?;
    valence_core::safe_ident::assert_safe_ident(field)?;
    let idx = format!("valence_idx_{table}_{field}");
    let q = format!("CREATE INDEX IF NOT EXISTS {idx} ON {table} ({field})");
    sqlx::query(&q)
        .execute(pool)
        .await
        .map_err(|e| Error::database(e.to_string()))?;
    Ok(())
}

/// Ensure layout columns exist for a write (create table or add missing columns only).
///
/// Schema-backed (registry) tables: at most one inspect/create per process (boot sync owns
/// additive alters). Ad-hoc tables: same once-per-process cache, may add missing columns.
///
/// Unlike [`sync_typed_table_sqlite`], this does **not** refuse extra live columns —
/// ad-hoc writes often send a field subset.
pub async fn ensure_layout_for_write_sqlite(
    pool: &sqlx::SqlitePool,
    layout: &StorageLayout,
    ensured: &WriteEnsureCache,
) -> Result<()> {
    let registry = registry_table(&layout.table);
    if ensured.covers(&layout.table, layout) {
        return Ok(());
    }
    match inspect_typed_layout_sqlite(pool, &layout.table).await? {
        None => {
            ensure_typed_table_sqlite(pool, layout).await?;
        }
        Some(live) => {
            for f in &layout.fields {
                if live.fields.iter().any(|l| l.name == f.name) {
                    continue;
                }
                if registry {
                    return Err(Error::Validation(format!(
                        "missing column {}.{} — bump Schema.version and run sync_typed_tables_from_registry",
                        layout.table, f.name
                    )));
                }
                let ddl = sqlite_add_column(&layout.table, f)?;
                sqlx::query(&ddl)
                    .execute(pool)
                    .await
                    .map_err(|e| Error::database(e.to_string()))?;
                if f.unique {
                    define_unique_index_column_sqlite(pool, &layout.table, &f.name).await?;
                } else if f.indexed {
                    define_index_column_sqlite(pool, &layout.table, &f.name).await?;
                }
            }
        }
    }
    ensured.record(layout);
    Ok(())
}

pub async fn ensure_layout_for_write_postgres(
    pool: &sqlx::PgPool,
    layout: &StorageLayout,
    ensured: &WriteEnsureCache,
) -> Result<()> {
    let registry = registry_table(&layout.table);
    if ensured.covers(&layout.table, layout) {
        return Ok(());
    }
    match inspect_typed_layout_postgres(pool, &layout.table).await? {
        None => {
            ensure_typed_table_postgres(pool, layout).await?;
        }
        Some(live) => {
            for f in &layout.fields {
                if live.fields.iter().any(|l| l.name == f.name) {
                    continue;
                }
                if registry {
                    return Err(Error::Validation(format!(
                        "missing column {}.{} — bump Schema.version and run sync_typed_tables_from_registry",
                        layout.table, f.name
                    )));
                }
                let ddl = postgres_add_column(&layout.table, f)?;
                sqlx::query(&ddl)
                    .execute(pool)
                    .await
                    .map_err(|e| Error::database(e.to_string()))?;
                if f.unique {
                    define_unique_index_column_postgres(pool, &layout.table, &f.name).await?;
                } else if f.indexed {
                    define_index_column_postgres(pool, &layout.table, &f.name).await?;
                }
            }
        }
    }
    ensured.record(layout);
    Ok(())
}

fn column_list(layout: &StorageLayout) -> String {
    layout
        .fields
        .iter()
        .map(|f| f.name.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

fn placeholders_sqlite(n: usize) -> String {
    (0..n).map(|_| "?").collect::<Vec<_>>().join(", ")
}

fn placeholders_postgres(n: usize) -> String {
    (1..=n)
        .map(|i| format!("${i}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Insert typed row (SQLite).
pub async fn create_record_typed_sqlite(
    pool: &sqlx::SqlitePool,
    layout: &StorageLayout,
    content: Value,
    ensured: &WriteEnsureCache,
) -> Result<Value> {
    validate_write_types(layout, &content)?;
    ensure_layout_for_write_sqlite(pool, layout, ensured).await?;
    let mut content = content;
    valence_core::ttl::prepare_create_content_with_capability(
        &layout.table,
        valence_core::ttl::BackendTtlCapability::Deferred,
        &mut content,
    )?;
    // Re-resolve layout after TTL stamp may add expire field.
    let layout = StorageLayout::resolve_for_write(&layout.table, &content)?;
    ensure_layout_for_write_sqlite(pool, &layout, ensured).await?;

    let (id_opt, fields) = split_record_fields(content);
    let id = id_opt.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let cols = column_list(&layout);
    let ph = placeholders_sqlite(layout.fields.len());
    let q = format!("INSERT INTO {} ({cols}) VALUES ({ph})", layout.table);
    let mut query = sqlx::query(&q);
    let mut out_fields = Map::new();
    for f in &layout.fields {
        if f.name == "id" {
            query = query.bind(&id);
            continue;
        }
        let val = fields.get(&f.name).cloned().unwrap_or(Value::Null);
        out_fields.insert(f.name.clone(), val.clone());
        query = bind_sqlite(query, f.storage, &val)?;
    }
    query
        .execute(pool)
        .await
        .map_err(|e| Error::database(e.to_string()))?;
    Ok(row_from_columns(&layout.table, &id, out_fields))
}

/// Insert typed row (Postgres).
pub async fn create_record_typed_postgres(
    pool: &sqlx::PgPool,
    layout: &StorageLayout,
    content: Value,
    ensured: &WriteEnsureCache,
) -> Result<Value> {
    validate_write_types(layout, &content)?;
    ensure_layout_for_write_postgres(pool, layout, ensured).await?;
    let mut content = content;
    valence_core::ttl::prepare_create_content_with_capability(
        &layout.table,
        valence_core::ttl::BackendTtlCapability::Deferred,
        &mut content,
    )?;
    let layout = StorageLayout::resolve_for_write(&layout.table, &content)?;
    ensure_layout_for_write_postgres(pool, &layout, ensured).await?;

    let (id_opt, fields) = split_record_fields(content);
    let id = id_opt.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let cols = column_list(&layout);
    let ph = placeholders_postgres(layout.fields.len());
    let q = format!("INSERT INTO {} ({cols}) VALUES ({ph})", layout.table);
    let mut query = sqlx::query(&q);
    let mut out_fields = Map::new();
    for f in &layout.fields {
        if f.name == "id" {
            query = query.bind(&id);
            continue;
        }
        let val = fields.get(&f.name).cloned().unwrap_or(Value::Null);
        out_fields.insert(f.name.clone(), val.clone());
        query = bind_postgres(query, f.storage, &val)?;
    }
    query
        .execute(pool)
        .await
        .map_err(|e| Error::database(e.to_string()))?;
    Ok(row_from_columns(&layout.table, &id, out_fields))
}

fn bind_sqlite<'q>(
    query: sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments<'q>>,
    storage: FieldStorage,
    value: &Value,
) -> Result<sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments<'q>>> {
    if value.is_null() {
        return Ok(query.bind(None::<String>));
    }
    match storage {
        FieldStorage::Integer => {
            let v = value
                .as_i64()
                .ok_or_else(|| Error::Validation(format!("expected integer, got {value}")))?;
            Ok(query.bind(v))
        }
        FieldStorage::Boolean => Ok(query.bind(value.as_bool().unwrap_or(false))),
        FieldStorage::Decimal => {
            let v = value
                .as_f64()
                .ok_or_else(|| Error::Validation(format!("expected decimal, got {value}")))?;
            Ok(query.bind(v))
        }
        FieldStorage::Json | FieldStorage::Currency => {
            let s = serde_json::to_string(value).map_err(Error::serialization)?;
            Ok(query.bind(s))
        }
        FieldStorage::String | FieldStorage::Date => {
            let s = sql_bind_text(storage, value)?.unwrap_or_default();
            Ok(query.bind(s))
        }
    }
}

fn bind_postgres<'q>(
    query: sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>,
    storage: FieldStorage,
    value: &Value,
) -> Result<sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>> {
    if value.is_null() {
        return Ok(query.bind(None::<String>));
    }
    match storage {
        FieldStorage::Integer => {
            let v = value
                .as_i64()
                .ok_or_else(|| Error::Validation(format!("expected integer, got {value}")))?;
            Ok(query.bind(v))
        }
        FieldStorage::Boolean => Ok(query.bind(value.as_bool().unwrap_or(false))),
        FieldStorage::Decimal => {
            let v = value
                .as_f64()
                .ok_or_else(|| Error::Validation(format!("expected decimal, got {value}")))?;
            Ok(query.bind(v))
        }
        FieldStorage::Json | FieldStorage::Currency => Ok(query.bind(value.clone())),
        FieldStorage::String | FieldStorage::Date => {
            let s = sql_bind_text(storage, value)?.unwrap_or_default();
            Ok(query.bind(s))
        }
    }
}

/// Fetch one typed row (SQLite).
pub async fn get_record_typed_sqlite(
    pool: &sqlx::SqlitePool,
    table: &str,
    id: &str,
) -> Result<Option<Value>> {
    assert_safe_table(table)?;
    let layout = match inspect_typed_layout_sqlite(pool, table).await? {
        Some(l) if !l.fields.is_empty() => l,
        _ => {
            // Table missing — try registry layout ensure then miss.
            if let Ok(layout) = StorageLayout::from_registry_table(table) {
                ensure_typed_table_sqlite(pool, &layout).await?;
            }
            return Ok(None);
        }
    };
    let cols = column_list(&layout);
    let q = format!("SELECT {cols} FROM {table} WHERE id = ?");
    let row = sqlx::query(&q)
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(|e| Error::database(e.to_string()))?;
    Ok(row.map(|r| row_from_sqlx_sqlite(table, &layout, &r)))
}

/// Fetch one typed row (Postgres).
pub async fn get_record_typed_postgres(
    pool: &sqlx::PgPool,
    table: &str,
    id: &str,
) -> Result<Option<Value>> {
    assert_safe_table(table)?;
    let layout = match inspect_typed_layout_postgres(pool, table).await? {
        Some(l) if !l.fields.is_empty() => l,
        _ => {
            if let Ok(layout) = StorageLayout::from_registry_table(table) {
                ensure_typed_table_postgres(pool, &layout).await?;
            }
            return Ok(None);
        }
    };
    let cols = column_list(&layout);
    let q = format!("SELECT {cols} FROM {table} WHERE id = $1");
    let row = sqlx::query(&q)
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(|e| Error::database(e.to_string()))?;
    Ok(row.map(|r| row_from_sqlx_postgres(table, &layout, &r)))
}

fn row_from_sqlx_sqlite(
    table: &str,
    layout: &StorageLayout,
    row: &sqlx::sqlite::SqliteRow,
) -> Value {
    let mut fields = Map::new();
    let mut id = String::new();
    for (i, f) in layout.fields.iter().enumerate() {
        if f.name == "id" {
            id = row.try_get::<String, _>(i).unwrap_or_default();
            continue;
        }
        let val = match f.storage {
            FieldStorage::Integer => {
                let n: Option<i64> = row.try_get(i).unwrap_or(None);
                n.map(|x| Value::Number(x.into())).unwrap_or(Value::Null)
            }
            FieldStorage::Boolean => {
                let n: Option<i64> = row.try_get(i).unwrap_or(None);
                n.map(|x| Value::Bool(x != 0)).unwrap_or(Value::Null)
            }
            FieldStorage::Decimal => {
                let n: Option<f64> = row.try_get(i).unwrap_or(None);
                n.and_then(serde_json::Number::from_f64)
                    .map(Value::Number)
                    .unwrap_or(Value::Null)
            }
            FieldStorage::Json | FieldStorage::Currency => {
                let s: Option<String> = row.try_get(i).unwrap_or(None);
                match s.as_deref() {
                    None | Some("") => Value::Null,
                    Some("null") => Value::Null,
                    Some(t) => serde_json::from_str(t).unwrap_or(Value::String(t.to_string())),
                }
            }
            FieldStorage::String | FieldStorage::Date => {
                let s: Option<String> = row.try_get(i).unwrap_or(None);
                match s {
                    None => Value::Null,
                    Some(t) if t.is_empty() => Value::Null,
                    Some(t) => Value::String(t),
                }
            }
        };
        fields.insert(f.name.clone(), val);
    }
    row_from_columns(table, &id, fields)
}

fn row_from_sqlx_postgres(
    table: &str,
    layout: &StorageLayout,
    row: &sqlx::postgres::PgRow,
) -> Value {
    let mut fields = Map::new();
    let mut id = String::new();
    for (i, f) in layout.fields.iter().enumerate() {
        if f.name == "id" {
            id = row.try_get::<String, _>(i).unwrap_or_default();
            continue;
        }
        let val = match f.storage {
            FieldStorage::Integer => {
                let n: Option<i64> = row.try_get(i).ok();
                n.map(|x| Value::Number(x.into())).unwrap_or(Value::Null)
            }
            FieldStorage::Boolean => {
                let n: Option<bool> = row.try_get(i).ok();
                n.map(Value::Bool).unwrap_or(Value::Null)
            }
            FieldStorage::Decimal => {
                let n: Option<f64> = row.try_get(i).ok();
                n.and_then(serde_json::Number::from_f64)
                    .map(Value::Number)
                    .unwrap_or(Value::Null)
            }
            FieldStorage::Json | FieldStorage::Currency => {
                let v: Option<Value> = row.try_get(i).ok();
                v.unwrap_or(Value::Null)
            }
            FieldStorage::String | FieldStorage::Date => {
                let s: Option<String> = row.try_get(i).ok();
                s.map(Value::String).unwrap_or(Value::Null)
            }
        };
        fields.insert(f.name.clone(), val);
    }
    row_from_columns(table, &id, fields)
}

/// Update replaces all non-id columns from content.
pub async fn update_record_typed_sqlite(
    pool: &sqlx::SqlitePool,
    layout: &StorageLayout,
    id: &str,
    content: Value,
    ensured: &WriteEnsureCache,
) -> Result<Value> {
    if get_record_typed_sqlite(pool, &layout.table, id)
        .await?
        .is_none()
    {
        return Err(Error::NotFound(format!("{}:{id}", layout.table)));
    }
    ensure_layout_for_write_sqlite(pool, layout, ensured).await?;
    let (_, fields) = split_record_fields(content);
    let data: Vec<&LayoutField> = layout.fields.iter().filter(|f| f.name != "id").collect();
    if data.is_empty() {
        return get_record_typed_sqlite(pool, &layout.table, id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("{}:{id}", layout.table)));
    }
    let sets = data
        .iter()
        .map(|f| format!("{} = ?", f.name))
        .collect::<Vec<_>>()
        .join(", ");
    let q = format!("UPDATE {} SET {sets} WHERE id = ?", layout.table);
    let mut query = sqlx::query(&q);
    let mut out = Map::new();
    for f in &data {
        let val = fields.get(&f.name).cloned().unwrap_or(Value::Null);
        out.insert(f.name.clone(), val.clone());
        query = bind_sqlite(query, f.storage, &val)?;
    }
    query = query.bind(id);
    query
        .execute(pool)
        .await
        .map_err(|e| Error::database(e.to_string()))?;
    Ok(row_from_columns(&layout.table, id, out))
}

pub async fn update_record_typed_postgres(
    pool: &sqlx::PgPool,
    layout: &StorageLayout,
    id: &str,
    content: Value,
    ensured: &WriteEnsureCache,
) -> Result<Value> {
    if get_record_typed_postgres(pool, &layout.table, id)
        .await?
        .is_none()
    {
        return Err(Error::NotFound(format!("{}:{id}", layout.table)));
    }
    ensure_layout_for_write_postgres(pool, layout, ensured).await?;
    let (_, fields) = split_record_fields(content);
    let data: Vec<&LayoutField> = layout.fields.iter().filter(|f| f.name != "id").collect();
    if data.is_empty() {
        return get_record_typed_postgres(pool, &layout.table, id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("{}:{id}", layout.table)));
    }
    let sets = data
        .iter()
        .enumerate()
        .map(|(i, f)| format!("{} = ${}", f.name, i + 1))
        .collect::<Vec<_>>()
        .join(", ");
    let id_ph = data.len() + 1;
    let q = format!("UPDATE {} SET {sets} WHERE id = ${id_ph}", layout.table);
    let mut query = sqlx::query(&q);
    let mut out = Map::new();
    for f in &data {
        let val = fields.get(&f.name).cloned().unwrap_or(Value::Null);
        out.insert(f.name.clone(), val.clone());
        query = bind_postgres(query, f.storage, &val)?;
    }
    query = query.bind(id);
    query
        .execute(pool)
        .await
        .map_err(|e| Error::database(e.to_string()))?;
    Ok(row_from_columns(&layout.table, id, out))
}

/// Map a sqlx SQLite row with arbitrary columns into Valence JSON (SELECT *).
pub fn map_select_row_sqlite(table: &str, row: &sqlx::sqlite::SqliteRow) -> Value {
    let cols = row.columns();
    let mut fields = Map::new();
    let mut id = String::new();
    for col in cols {
        let name = col.name();
        if name.eq_ignore_ascii_case("id") {
            id = row.try_get::<String, _>(name).unwrap_or_default();
            continue;
        }
        // Prefer string, then i64, then f64.
        if let Ok(s) = row.try_get::<String, _>(name) {
            if let Ok(v) = serde_json::from_str::<Value>(&s) {
                if v.is_object() || v.is_array() {
                    fields.insert(name.to_string(), v);
                    continue;
                }
            }
            fields.insert(name.to_string(), Value::String(s));
        } else if let Ok(i) = row.try_get::<i64, _>(name) {
            fields.insert(name.to_string(), Value::Number(i.into()));
        } else if let Ok(f) = row.try_get::<f64, _>(name) {
            if let Some(n) = serde_json::Number::from_f64(f) {
                fields.insert(name.to_string(), Value::Number(n));
            }
        } else if let Ok(b) = row.try_get::<bool, _>(name) {
            fields.insert(name.to_string(), Value::Bool(b));
        }
    }
    if id.is_empty() {
        return Value::Object(fields);
    }
    row_from_columns(table, &id, fields)
}

pub fn map_select_row_postgres(table: &str, row: &sqlx::postgres::PgRow) -> Value {
    use sqlx::Column;
    let cols = row.columns();
    let mut fields = Map::new();
    let mut id = String::new();
    for col in cols {
        let name = col.name();
        if name.eq_ignore_ascii_case("id") {
            id = row.try_get::<String, _>(name).unwrap_or_default();
            continue;
        }
        if let Ok(v) = row.try_get::<Value, _>(name) {
            fields.insert(name.to_string(), v);
        } else if let Ok(s) = row.try_get::<String, _>(name) {
            fields.insert(name.to_string(), Value::String(s));
        } else if let Ok(i) = row.try_get::<i64, _>(name) {
            fields.insert(name.to_string(), Value::Number(i.into()));
        } else if let Ok(b) = row.try_get::<bool, _>(name) {
            fields.insert(name.to_string(), Value::Bool(b));
        } else if let Ok(f) = row.try_get::<f64, _>(name) {
            if let Some(n) = serde_json::Number::from_f64(f) {
                fields.insert(name.to_string(), Value::Number(n));
            }
        }
    }
    if id.is_empty() {
        return Value::Object(fields);
    }
    row_from_columns(table, &id, fields)
}

// silence unused import in some cfgs
#[allow(dead_code)]
fn _decode(storage: FieldStorage, raw: Option<&str>, i: Option<i64>) -> Value {
    decode_sql_cell(storage, raw, i)
}
