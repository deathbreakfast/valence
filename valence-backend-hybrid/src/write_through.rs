//! Primary-first write helpers with best-effort IndraDB mirror updates.

use std::sync::Arc;

use serde_json::Value;
use valence_backend_indradb::IndradbBackend;
use valence_core::error::Result;
use valence_core::record_id::RecordId;
use valence_core::DatabaseBackend;

use crate::cache_policy::CachePolicy;
use crate::edge_cache::EdgeCache;
use crate::record_cache::RecordCache;

/// Create on the primary, then upsert into the record cache.
///
/// # Errors
///
/// Returns primary create errors. Mirror failures are soft (invalidate + metric).
pub async fn create_record(
    primary: &Arc<dyn DatabaseBackend>,
    mirror: &IndradbBackend,
    records: &RecordCache,
    policy: &CachePolicy,
    table: &str,
    content: Value,
) -> Result<Value> {
    let row = primary.create_record(table, content).await?;
    soft_put_record(mirror, records, policy, table, &row).await;
    Ok(row)
}

/// Update on the primary, then refresh the record cache.
///
/// # Errors
///
/// Returns primary update errors. Mirror failures are soft.
pub async fn update_record(
    primary: &Arc<dyn DatabaseBackend>,
    mirror: &IndradbBackend,
    records: &RecordCache,
    policy: &CachePolicy,
    table: &str,
    id: &str,
    content: Value,
) -> Result<Value> {
    let row = primary.update_record(table, id, content).await?;
    soft_put_record(mirror, records, policy, table, &row).await;
    Ok(row)
}

/// Merge on the primary, then refresh the record cache.
///
/// # Errors
///
/// Returns primary merge errors. Mirror failures are soft.
pub async fn merge_record(
    primary: &Arc<dyn DatabaseBackend>,
    mirror: &IndradbBackend,
    records: &RecordCache,
    policy: &CachePolicy,
    table: &str,
    id: &str,
    patch: Value,
) -> Result<Value> {
    let row = primary.merge_record(table, id, patch).await?;
    soft_put_record(mirror, records, policy, table, &row).await;
    Ok(row)
}

/// Compare-and-set merge on the primary, then refresh the record cache.
///
/// # Errors
///
/// Returns primary errors, including [`valence_core::Error::Conflict`] when the guard field
/// changed. Mirror failures are soft.
#[allow(clippy::too_many_arguments)]
pub async fn compare_and_merge_record(
    primary: &Arc<dyn DatabaseBackend>,
    mirror: &IndradbBackend,
    records: &RecordCache,
    policy: &CachePolicy,
    table: &str,
    id: &str,
    field: &str,
    expected: &Value,
    patch: Value,
) -> Result<Value> {
    let row = primary
        .compare_and_merge_record(table, id, field, expected, patch)
        .await?;
    soft_put_record(mirror, records, policy, table, &row).await;
    Ok(row)
}

/// Upsert on the primary, then refresh the record cache.
///
/// # Errors
///
/// Returns primary upsert errors. Mirror failures are soft.
pub async fn upsert_record(
    primary: &Arc<dyn DatabaseBackend>,
    mirror: &IndradbBackend,
    records: &RecordCache,
    policy: &CachePolicy,
    table: &str,
    id: &str,
    content: Value,
) -> Result<Value> {
    let row = primary.upsert_record(table, id, content).await?;
    soft_put_record(mirror, records, policy, table, &row).await;
    Ok(row)
}

/// Delete on the primary, then invalidate the record cache.
///
/// # Errors
///
/// Returns primary delete errors. Mirror failures are soft.
pub async fn delete_record(
    primary: &Arc<dyn DatabaseBackend>,
    mirror: &IndradbBackend,
    records: &RecordCache,
    table: &str,
    id: &str,
) -> Result<()> {
    primary.delete_record(table, id).await?;
    if let Err(err) = records.invalidate(mirror, table, id).await {
        let _ = err;
        crate::telemetry::record_mirror_soft_failure("delete_record");
    }
    Ok(())
}

/// Relate on the primary, then dual-write into the edge cache.
///
/// # Errors
///
/// Returns primary relate errors. Mirror failures are soft.
pub async fn relate_edge(
    primary: &Arc<dyn DatabaseBackend>,
    mirror: &IndradbBackend,
    edges: &EdgeCache,
    policy: &CachePolicy,
    from: &RecordId,
    edge_table: &str,
    to: &RecordId,
) -> Result<()> {
    primary.relate_edge(from, edge_table, to).await?;
    if let Err(err) = edges.relate(mirror, policy, from, edge_table, to).await {
        let _ = err;
        crate::telemetry::record_mirror_soft_failure("relate_edge");
        edges.mark_incomplete(edge_table);
    }
    Ok(())
}

/// Unrelate on the primary, then remove from the edge cache.
///
/// # Errors
///
/// Returns primary unrelate errors. Mirror failures are soft.
pub async fn unrelate_edge(
    primary: &Arc<dyn DatabaseBackend>,
    mirror: &IndradbBackend,
    edges: &EdgeCache,
    from: &RecordId,
    edge_table: &str,
    to: &RecordId,
) -> Result<()> {
    primary.unrelate_edge(from, edge_table, to).await?;
    if let Err(err) = edges.unrelate(mirror, from, edge_table, to).await {
        let _ = err;
        crate::telemetry::record_mirror_soft_failure("unrelate_edge");
    }
    Ok(())
}

/// Best-effort mirror upsert using the row's storage id.
async fn soft_put_record(
    mirror: &IndradbBackend,
    records: &RecordCache,
    policy: &CachePolicy,
    table: &str,
    row: &Value,
) {
    let Some(id) = storage_id_from_content(row) else {
        return;
    };
    if let Err(err) = records.put(mirror, policy, table, &id, row.clone()).await {
        let _ = err;
        crate::telemetry::record_mirror_soft_failure("put_record");
        let _ = records.invalidate(mirror, table, &id).await;
    }
}

/// Extract a bare storage id from a record JSON body.
fn storage_id_from_content(content: &Value) -> Option<String> {
    let id_val = content.get("id")?;
    if let Some(id) = id_val.get("id").and_then(|v| v.as_str()) {
        return Some(id.to_string());
    }
    id_val.as_str().map(str::to_string)
}
