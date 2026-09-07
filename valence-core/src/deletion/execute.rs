//! Synchronous deletion executor (`delete_entity_now` / `Model::delete_now`).

use crate::deletion::apply_deletion_node;
use crate::deletion::dag::DeletionDag;
use crate::deletion::prepare::{prepare_deletion, DeletionMode, PreparedDeletion};
use crate::error::Result;
use crate::runtime::Valence;

/// Apply every node in DAG execution order under `valence`.
///
/// # Errors
///
/// Propagates the first [`apply_deletion_node`] failure. Earlier nodes may already
/// have been applied; retry is safe because missing rows are success.
pub async fn apply_deletion_dag(dag: &DeletionDag, valence: &Valence) -> Result<()> {
    for node in &dag.nodes {
        apply_deletion_node(node, valence).await?;
    }
    Ok(())
}

/// Physically delete `table`/`id` and its deletion DAG in the current future.
///
/// Computes and authorizes the full DAG before mutation, then applies every node
/// under the requesting actor. Missing rows succeed. A root already owned by a
/// queued deletion returns [`Error::PendingDeletion`](crate::error::Error::PendingDeletion).
///
/// This path is intentionally unbounded. Prefer queued [`crate::queue_delete_entity`]
/// for large or retry-heavy graphs; reserve `delete_entity_now` for bounded request work.
///
/// # Errors
///
/// Privacy, Restrict validation, pending coordination, unknown table, or apply failures.
///
/// # Examples
///
/// ```rust,ignore
/// use valence::delete_entity_now;
///
/// delete_entity_now("project", "project-42", &session_valence).await?;
/// ```
pub async fn delete_entity_now(table: &str, id: &str, valence: &Valence) -> Result<()> {
    match prepare_deletion(table, id, DeletionMode::Now, valence).await? {
        PreparedDeletion::Missing => Ok(()),
        PreparedDeletion::Pending { bare_id } => Err(crate::error::Error::PendingDeletion(
            format!("{table}:{bare_id} is pending deletion"),
        )),
        PreparedDeletion::Ready { dag, .. } => apply_deletion_dag(&dag, valence).await,
    }
}
