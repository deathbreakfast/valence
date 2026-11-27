//! All-or-nothing write batches for SQLite and Postgres (`DatabaseBackend::apply_batch`).

use serde_json::Value;
use valence_core::backend::WriteOp;
use valence_core::error::{Error, Result};

use crate::postgres_ops::{
    create_record_postgres, delete_record_postgres, get_record_postgres, merge_record_postgres,
    update_record_postgres,
};
use crate::sqlite_ops::{
    create_record_sqlite, delete_record_sqlite, get_record_sqlite, merge_record_sqlite,
    update_record_sqlite,
};
use crate::typed_table::WriteEnsureCache;

/// Content for an upsert that turns into a create: the explicit id stamped in.
fn with_record_id(table: &str, id: &str, content: Value) -> Value {
    let mut content = content;
    if let Some(obj) = content.as_object_mut() {
        obj.insert("id".into(), serde_json::json!({ "table": table, "id": id }));
    }
    content
}

/// Apply `ops` in one SQLite transaction; returns one row per op (`Null` for deletes).
///
/// Each op behaves like the matching single-row backend method. When any op fails the
/// transaction rolls back and nothing is visible. Layout DDL a write needs runs inside the
/// transaction; `ensured` learns about it only after commit.
pub async fn apply_batch_sqlite(
    pool: &sqlx::SqlitePool,
    ops: Vec<WriteOp>,
    ensured: &WriteEnsureCache,
) -> Result<Vec<Value>> {
    if ops.is_empty() {
        return Ok(Vec::new());
    }
    let staged = ensured.snapshot();
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| Error::database(e.to_string()))?;
    let mut results = Vec::with_capacity(ops.len());
    for op in ops {
        let row = match op {
            WriteOp::Create { table, content } => {
                create_record_sqlite(&mut tx, &table, content, &staged).await?
            }
            WriteOp::Update { table, id, content } => {
                update_record_sqlite(&mut tx, &table, &id, content, &staged).await?
            }
            WriteOp::Merge { table, id, patch } => {
                merge_record_sqlite(&mut tx, &table, &id, patch, &staged).await?
            }
            WriteOp::Upsert { table, id, content } => {
                if get_record_sqlite(&mut tx, &table, &id).await?.is_some() {
                    update_record_sqlite(&mut tx, &table, &id, content, &staged).await?
                } else {
                    let content = with_record_id(&table, &id, content);
                    create_record_sqlite(&mut tx, &table, content, &staged).await?
                }
            }
            WriteOp::Delete { table, id } => {
                delete_record_sqlite(&mut tx, &table, &id).await?;
                Value::Null
            }
        };
        results.push(row);
    }
    tx.commit()
        .await
        .map_err(|e| Error::database(e.to_string()))?;
    ensured.merge_from(&staged);
    Ok(results)
}

/// Apply `ops` in one Postgres transaction; same contract as [`apply_batch_sqlite`].
pub async fn apply_batch_postgres(
    pool: &sqlx::PgPool,
    ops: Vec<WriteOp>,
    ensured: &WriteEnsureCache,
) -> Result<Vec<Value>> {
    if ops.is_empty() {
        return Ok(Vec::new());
    }
    let staged = ensured.snapshot();
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| Error::database(e.to_string()))?;
    let mut results = Vec::with_capacity(ops.len());
    for op in ops {
        let row = match op {
            WriteOp::Create { table, content } => {
                create_record_postgres(&mut tx, &table, content, &staged).await?
            }
            WriteOp::Update { table, id, content } => {
                update_record_postgres(&mut tx, &table, &id, content, &staged).await?
            }
            WriteOp::Merge { table, id, patch } => {
                merge_record_postgres(&mut tx, &table, &id, patch, &staged).await?
            }
            WriteOp::Upsert { table, id, content } => {
                if get_record_postgres(&mut tx, &table, &id).await?.is_some() {
                    update_record_postgres(&mut tx, &table, &id, content, &staged).await?
                } else {
                    let content = with_record_id(&table, &id, content);
                    create_record_postgres(&mut tx, &table, content, &staged).await?
                }
            }
            WriteOp::Delete { table, id } => {
                delete_record_postgres(&mut tx, &table, &id).await?;
                Value::Null
            }
        };
        results.push(row);
    }
    tx.commit()
        .await
        .map_err(|e| Error::database(e.to_string()))?;
    ensured.merge_from(&staged);
    Ok(results)
}
