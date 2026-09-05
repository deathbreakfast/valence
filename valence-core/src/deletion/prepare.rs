//! Shared deletion preflight for queued and synchronous paths.

use crate::deletion::dag::{
    assert_safe_bare_thing_id, table_skips_pending_deletion_filter, DeletionDag,
};
use crate::error::{Error, Result};
use crate::ownership::OwnershipService;
use crate::privacy::{PrivacyEvaluator, PrivacyOperation};
use crate::query::QueryCore;
use crate::runtime::Valence;
use crate::schema::SchemaRegistry;

/// How the caller will use a prepared DAG.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeletionMode {
    /// Mark pending and dispatch a durable run. Already-pending roots are a no-op.
    Queued,
    /// Apply the DAG in the current future. Already-pending roots are refused.
    Now,
}

/// Outcome of shared deletion preparation (no mutation yet).
#[derive(Debug, Clone)]
pub enum PreparedDeletion {
    /// No row at the normalized id.
    Missing,
    /// Ownership already marks the root `pending_deletion`.
    Pending {
        /// Normalized bare record id.
        bare_id: String,
    },
    /// DAG is authorized and ready to queue or apply.
    Ready {
        /// Normalized bare record id used for ownership and the DAG root.
        bare_id: String,
        /// Fully planned deletion graph (Restrict empty; privacy checked).
        dag: DeletionDag,
    },
}

/// Strip `table:` only when the prefix matches `table`.
///
/// Colon-bearing primary keys whose prefix names a *different* registered table
/// (for example Gauge `permission_group:{group_id}` on `permission_group_principal`)
/// are left unchanged.
#[must_use]
pub fn normalize_record_id_for_deletion(table: &str, entity_id: &str) -> String {
    let s = entity_id.trim();
    let Some((head, rest)) = s.split_once(':') else {
        return s.to_string();
    };
    if head.is_empty() || rest.is_empty() {
        return s.to_string();
    }
    if head == table {
        return rest.to_string();
    }
    s.to_string()
}

/// Prepare a deletion: load without Read privacy, authorize Delete across the DAG.
///
/// # Errors
///
/// - [`Error::NotFound`] — unknown schema/table
/// - [`Error::Validation`] — unsafe id, skip-graph table used with DAG mode, or Restrict
/// - [`Error::Privacy`] — root or cascade Delete denied
/// - [`Error::PendingDeletion`] — only when `mode` is [`DeletionMode::Now`] and the root is pending
/// - [`Error::Database`] — backend/ownership lookup failures that are not soft-absent
pub async fn prepare_deletion(
    table: &str,
    id: &str,
    mode: DeletionMode,
    valence: &Valence,
) -> Result<PreparedDeletion> {
    let table = table.trim();
    if table.is_empty() {
        return Err(Error::Validation("empty table for deletion".into()));
    }

    let registry = SchemaRegistry::global();
    let schema = registry
        .get_schema(table)
        .ok_or_else(|| Error::NotFound(format!("unknown table {table}")))?;

    let bare_id = normalize_record_id_for_deletion(table, id);
    assert_safe_bare_thing_id(&bare_id)?;

    let Some(existing) = QueryCore::get_record_json(table, &bare_id, valence).await? else {
        return Ok(PreparedDeletion::Missing);
    };

    if let Ok(Some(ownership)) =
        OwnershipService::get_ownership_json(table, &bare_id, valence).await
    {
        if ownership.get("status").and_then(|s| s.as_str()) == Some("pending_deletion") {
            match mode {
                DeletionMode::Queued => {
                    return Ok(PreparedDeletion::Pending {
                        bare_id: bare_id.clone(),
                    });
                }
                DeletionMode::Now => {
                    return Err(Error::PendingDeletion(format!(
                        "{table}:{bare_id} is pending deletion"
                    )));
                }
            }
        }
    }

    PrivacyEvaluator::check_entity_access(schema, PrivacyOperation::Delete, &existing, valence)
        .await?;

    if table_skips_pending_deletion_filter(table) {
        match mode {
            DeletionMode::Queued => {
                return Err(Error::Validation(format!(
                    "queued delete is not supported for table {table:?}"
                )));
            }
            DeletionMode::Now => {
                // Platform tables: no cascade expansion — single CascadeDelete node.
                let dag = DeletionDag::from_nodes(
                    table,
                    &bare_id,
                    vec![crate::deletion::dag::DeletionNode {
                        table: table.to_string(),
                        record_id: bare_id.clone(),
                        action: crate::deletion::dag::DeletionAction::CascadeDelete,
                        depth: 0,
                        connection_name: String::new(),
                        from_table: String::new(),
                    }],
                    vec![],
                );
                return Ok(PreparedDeletion::Ready { bare_id, dag });
            }
        }
    }

    let dag = DeletionDag::compute(table, &bare_id, valence).await?;
    if !dag.restrict_violations.is_empty() {
        #[cfg(feature = "instrumentation")]
        for v in &dag.restrict_violations {
            crate::instrumentation::record_restrict_blocked(
                table,
                &bare_id,
                &v.connection_name,
                v.blocking_record_count.max(0) as usize,
            );
        }
        return Err(Error::Validation(format!(
            "delete restricted by schema connections: {:?}",
            dag.restrict_violations
        )));
    }
    crate::deletion::check_dag_delete_privacy(&dag, valence).await?;

    Ok(PreparedDeletion::Ready { bare_id, dag })
}

#[cfg(test)]
mod normalize_tests {
    use super::normalize_record_id_for_deletion;

    #[test]
    fn strips_matching_table_prefix() {
        assert_eq!(
            normalize_record_id_for_deletion("project", "project:42"),
            "42"
        );
        assert_eq!(
            normalize_record_id_for_deletion("project", "  project:42  "),
            "42"
        );
    }

    #[test]
    fn keeps_bare_and_foreign_qualified_ids() {
        assert_eq!(
            normalize_record_id_for_deletion("project", "bare-id"),
            "bare-id"
        );
        assert_eq!(
            normalize_record_id_for_deletion(
                "permission_group_principal",
                "permission_group:owners_xyz"
            ),
            "permission_group:owners_xyz"
        );
    }
}
