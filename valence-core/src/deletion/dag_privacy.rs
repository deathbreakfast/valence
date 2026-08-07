//! Pre-queue Delete authorization over a computed [`DeletionDag`].

use crate::deletion::dag::DeletionDag;
use crate::error::{Error, Result};
use crate::privacy::{PrivacyEvaluator, PrivacyOperation};
use crate::query::QueryCore;
use crate::runtime::Valence;
use crate::schema::SchemaRegistry;

/// Authorize [`PrivacyOperation::Delete`] for every **CascadeDelete** node in `dag` under
/// `valence`'s actor.
///
/// SetNull / RemoveEdge nodes are skipped (deletion-scoped clears; no Update/Delete on
/// referrers). Call after [`DeletionDag::compute`] succeeds with no Restrict violations and
/// **before** `mark_pending_deletion` / `create_run` / dispatch.
pub async fn check_dag_delete_privacy(dag: &DeletionDag, valence: &Valence) -> Result<()> {
    check_dag_delete_privacy_with_registry(dag, valence, SchemaRegistry::global()).await
}

/// Like [`check_dag_delete_privacy`], but uses `registry` (tests / tooling).
///
/// # Errors
///
/// Returns the first privacy denial (or backend error) encountered while walking nodes.
pub async fn check_dag_delete_privacy_with_registry(
    dag: &DeletionDag,
    valence: &Valence,
    registry: &SchemaRegistry,
) -> Result<()> {
    for node in &dag.nodes {
        if !matches!(
            node.action,
            crate::deletion::dag::DeletionAction::CascadeDelete
        ) {
            continue;
        }
        let Some(existing) =
            QueryCore::get_record_json(node.table.as_str(), node.record_id.as_str(), valence)
                .await?
        else {
            continue;
        };
        let Some(schema) = registry.get_schema(node.table.as_str()) else {
            continue;
        };
        PrivacyEvaluator::check_entity_access(schema, PrivacyOperation::Delete, &existing, valence)
            .await
            .map_err(|e| match e {
                Error::Privacy(msg) => {
                    Error::Privacy(format!("delete denied for table {}: {msg}", node.table))
                }
                other => other,
            })?;
    }
    Ok(())
}
