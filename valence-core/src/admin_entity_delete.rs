//! Table-keyed queued delete for admin tooling.

use crate::deletion::dag::table_skips_pending_deletion_filter;
use crate::deletion::dag::DeletionDag;
use crate::deletion::{dispatch, DeletionRequest, DeletionService};
use crate::error::{Error, Result};
use crate::ownership;
use crate::privacy::{PrivacyEvaluator, PrivacyOperation};
use crate::query::QueryCore;
use crate::runtime::Valence;
use crate::schema::SchemaRegistry;

/// # Errors
///
/// Returns an error when the requested operation cannot be completed.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "deletion metrics require a usize count after clamping negative values"
)]
pub async fn queue_delete_entity(table: &str, id: &str, v: &Valence) -> Result<()> {
    if table_skips_pending_deletion_filter(table) {
        return Err(Error::Validation(format!(
            "queued delete is not supported for table {table:?}"
        )));
    }

    let registry = SchemaRegistry::global();
    let schema = registry
        .get_schema(table)
        .ok_or_else(|| Error::NotFound(format!("unknown table {table}")))?;

    let Some(existing) = QueryCore::get_record_json(table, id, v).await? else {
        return Ok(());
    };

    PrivacyEvaluator::check_entity_read(schema, &existing, v).await?;
    PrivacyEvaluator::check_entity_access(schema, PrivacyOperation::Delete, &existing, v).await?;

    let bare = ownership::normalize_record_id_for_ownership(id);
    if let Ok(Some(ownership)) =
        ownership::OwnershipService::get_ownership_json(table, &bare, v).await
    {
        if ownership.get("status").and_then(|s| s.as_str()) == Some("pending_deletion") {
            return Ok(());
        }
    }

    let dag = DeletionDag::compute(table, &bare, v).await?;
    if !dag.restrict_violations.is_empty() {
        #[cfg(feature = "instrumentation")]
        for v in &dag.restrict_violations {
            crate::instrumentation::record_restrict_blocked(
                table,
                &bare,
                &v.connection_name,
                v.blocking_record_count.max(0) as usize,
            );
        }
        return Err(Error::Validation(format!(
            "delete restricted: {:?}",
            dag.restrict_violations
        )));
    }

    ownership::OwnershipService::mark_pending_deletion(table, &bare, v).await?;

    let actor_json = serde_json::to_value(v.actor()).unwrap_or(serde_json::Value::Null);
    let run_id = DeletionService::create_run(table, &bare, actor_json.clone(), v).await?;
    #[cfg(feature = "instrumentation")]
    {
        let max_depth = dag.nodes.iter().map(|n| n.depth).max().unwrap_or(0) as usize;
        crate::instrumentation::record_run_queued(table, &bare, dag.nodes.len(), max_depth);
    }
    dispatch(DeletionRequest {
        run_id,
        root_table: table.to_string(),
        root_record_id: bare,
        actor_json,
    })
    .await?;

    Ok(())
}
