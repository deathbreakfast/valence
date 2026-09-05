//! Table-keyed queued delete for admin tooling.

use crate::deletion::{
    dispatch, prepare_deletion, DeletionMode, DeletionRequest, DeletionService, PreparedDeletion,
};
use crate::error::Result;
use crate::ownership::OwnershipService;
use crate::runtime::Valence;

/// Queue a privacy-checked deletion run for `table`/`id`.
///
/// # Errors
///
/// Returns an error when the requested operation cannot be completed.
pub async fn queue_delete_entity(table: &str, id: &str, v: &Valence) -> Result<()> {
    let _ = queue_delete_entity_returning_run_id(table, id, v).await?;
    Ok(())
}

/// Like [`queue_delete_entity`], but returns the new `valence_deletion_run` id when a run
/// was created (`None` when the row was already missing or already `pending_deletion`).
///
/// # Errors
///
/// Returns an error when the requested operation cannot be completed.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "deletion metrics require a usize count after clamping negative values"
)]
pub async fn queue_delete_entity_returning_run_id(
    table: &str,
    id: &str,
    v: &Valence,
) -> Result<Option<String>> {
    match prepare_deletion(table, id, DeletionMode::Queued, v).await? {
        PreparedDeletion::Missing | PreparedDeletion::Pending { .. } => Ok(None),
        PreparedDeletion::Ready { bare_id, dag } => {
            OwnershipService::mark_pending_deletion(table, &bare_id, v).await?;

            let actor_json = serde_json::to_value(v.actor()).unwrap_or(serde_json::Value::Null);
            let run_id =
                DeletionService::create_run(table, &bare_id, actor_json.clone(), v).await?;
            #[cfg(feature = "instrumentation")]
            {
                let max_depth = dag.nodes.iter().map(|n| n.depth).max().unwrap_or(0) as usize;
                crate::instrumentation::record_run_queued(
                    table,
                    &bare_id,
                    dag.nodes.len(),
                    max_depth,
                );
            }
            let _ = dag;
            dispatch(DeletionRequest {
                run_id: run_id.clone(),
                root_table: table.to_string(),
                root_record_id: bare_id,
                actor_json,
            })
            .await?;

            Ok(Some(run_id))
        }
    }
}
