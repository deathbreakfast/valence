//! Shared physical apply for one [`DeletionNode`] (sync executor + platform worker).

use crate::deletion::dag::{DeletionAction, DeletionNode};
use crate::deletion::dispatch_queued_delete_side_effects;
use crate::error::Result;
use crate::ownership::OwnershipService;
use crate::privacy::{PrivacyEvaluator, PrivacyOperation};
use crate::query::QueryCore;
use crate::read_cache;
use crate::record_id::RecordId;
use crate::runtime::Valence;
use crate::schema::SchemaRegistry;

/// Apply one deletion DAG node under `valence` (deletion-scoped; no Update on SetNull).
///
/// - [`DeletionAction::CascadeDelete`]: Delete privacy (when schema exists), `delete_record`,
///   cache invalidation, best-effort ownership completion, then queued Delete side effects.
/// - [`DeletionAction::SetNull`]: `merge_record` with the FK field set to JSON `null`, then
///   cache invalidation.
/// - [`DeletionAction::RemoveEdge`]: `unrelate_edge` between this endpoint and the peer.
///
/// Missing rows / missing edges are treated as success (idempotent).
///
/// # Errors
///
/// Returns privacy denials for CascadeDelete, or backend errors for merge/unrelate/delete.
pub async fn apply_deletion_node(node: &DeletionNode, valence: &Valence) -> Result<()> {
    match &node.action {
        DeletionAction::CascadeDelete => {
            apply_cascade_delete(&node.table, &node.record_id, valence).await
        }
        DeletionAction::SetNull { field } => {
            apply_set_null(&node.table, &node.record_id, field, valence).await
        }
        DeletionAction::RemoveEdge { edge_table } => {
            apply_remove_edge(&node.table, &node.record_id, edge_table, valence).await
        }
    }
}

async fn apply_cascade_delete(table: &str, record_id: &str, valence: &Valence) -> Result<()> {
    let Some(existing) = QueryCore::get_record_json(table, record_id, valence).await? else {
        read_cache::invalidate(table, record_id);
        return Ok(());
    };
    if let Some(schema) = SchemaRegistry::global().get_schema(table) {
        PrivacyEvaluator::check_entity_access(schema, PrivacyOperation::Delete, &existing, valence)
            .await?;
    }
    let backend = valence.backend_for_table(table)?;
    backend.delete_record(table, record_id).await?;
    read_cache::invalidate(table, record_id);
    let _ = OwnershipService::mark_deleted_ownership(table, record_id, valence).await;
    dispatch_queued_delete_side_effects(table, existing, valence).await;
    Ok(())
}

async fn apply_set_null(
    table: &str,
    record_id: &str,
    field: &str,
    valence: &Valence,
) -> Result<()> {
    let Some(_existing) = QueryCore::get_record_json(table, record_id, valence).await? else {
        read_cache::invalidate(table, record_id);
        return Ok(());
    };
    let backend = valence.backend_for_table(table)?;
    let patch = serde_json::json!({ field: serde_json::Value::Null });
    backend.merge_record(table, record_id, patch).await?;
    read_cache::invalidate(table, record_id);
    Ok(())
}

async fn apply_remove_edge(
    table: &str,
    record_id: &str,
    edge_table: &str,
    valence: &Valence,
) -> Result<()> {
    let endpoint = RecordId::new(table, record_id);
    let backend = valence
        .backend_for_table(table)
        .or_else(|_| valence.active_backend())?;
    for to in backend.get_edge_targets(&endpoint, edge_table).await? {
        valence.unrelate_edge(edge_table, &endpoint, &to).await?;
    }
    for from in backend.get_edge_sources(&endpoint, edge_table).await? {
        valence.unrelate_edge(edge_table, &from, &endpoint).await?;
    }
    Ok(())
}
