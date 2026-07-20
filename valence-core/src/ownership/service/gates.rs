//! Pending-deletion gates and unified fetch paths for ownership-aware reads.

use std::sync::Arc;

use serde_json::Value;

use crate::backend::DatabaseBackend;
use crate::error::{Error, Result};
use crate::runtime::Valence;
use crate::schema::SchemaRegistry;

use super::helpers::{
    ownership_colocate_enabled, ownership_row_id, ownership_unified_fetch_enabled,
    skip_ownership_for_table, OwnershipGateStatus, RecordOwnershipBundle,
};
use super::OwnershipService;

impl OwnershipService {
    /// Pending-deletion gate for `Model::get`: absent ownership row passes; lookup errors pass.
    /// # Errors
    ///
    /// Returns an error when the requested operation cannot be completed.
    pub async fn pending_deletion_gate(
        valence_model: &str,
        bare_id: &str,
        v: &Valence,
    ) -> Result<()> {
        if skip_ownership_for_table(valence_model) {
            return Ok(());
        }
        match Self::get_ownership_json(valence_model, bare_id, v).await {
            Ok(Some(ownership)) => {
                let status = ownership
                    .get("status")
                    .and_then(|s| s.as_str())
                    .map(str::to_string)
                    .map_or(OwnershipGateStatus::Absent, OwnershipGateStatus::Status);
                Self::apply_pending_deletion_gate(valence_model, bare_id, status)
            }
            Ok(None) | Err(_) => Ok(()),
        }
    }

    /// Apply the pending-deletion gate from a pre-fetched [`OwnershipGateStatus`] (no I/O).
    /// # Errors
    ///
    /// Returns an error when the requested operation cannot be completed.
    #[allow(
        clippy::needless_pass_by_value,
        reason = "the public API intentionally owns the status value for forward compatibility"
    )]
    pub fn apply_pending_deletion_gate(
        valence_model: &str,
        bare_id: &str,
        status: OwnershipGateStatus,
    ) -> Result<()> {
        if skip_ownership_for_table(valence_model) {
            return Ok(());
        }
        if status.is_pending_deletion() {
            return Err(Error::PendingDeletion(format!(
                "{valence_model}:{bare_id} is pending deletion"
            )));
        }
        Ok(())
    }

    /// Unified `Model::get` fetch: one compiled query for row + ownership gate status (cache-aware).
    /// # Errors
    ///
    /// Returns an error when the requested operation cannot be completed.
    pub async fn fetch_record_with_ownership_gate(
        valence_model: &str,
        record_id: &str,
        v: &Valence,
    ) -> Result<RecordOwnershipBundle> {
        if skip_ownership_for_table(valence_model) {
            let backend = v.backend_for_table(valence_model)?;
            let row =
                crate::read_cache::get_record_via_cache(&backend, valence_model, record_id).await?;
            return Ok(RecordOwnershipBundle {
                row,
                ownership_status: OwnershipGateStatus::Absent,
            });
        }

        if !ownership_unified_fetch_enabled()
            || !ownership_colocate_enabled()
            || !SchemaRegistry::global().has_schema(valence_model)
        {
            crate::instrumentation::record_ownership_fetch_mode("legacy");
            let backend = v.backend_for_table(valence_model)?;
            let row =
                crate::read_cache::get_record_via_cache(&backend, valence_model, record_id).await?;
            return Ok(RecordOwnershipBundle {
                row,
                ownership_status: OwnershipGateStatus::NotFetched,
            });
        }

        let backend = v.backend_for_table(valence_model)?;
        crate::read_cache::get_record_with_ownership_bundle_via_cache(
            &backend,
            valence_model,
            record_id,
            valence_model,
            v,
        )
        .await
    }

    /// Single-trip backend fetch for row + ownership status (no read cache).
    /// # Errors
    ///
    /// Returns an error when the requested operation cannot be completed.
    pub async fn fetch_record_with_ownership_gate_uncached(
        backend: &Arc<dyn DatabaseBackend>,
        table: &str,
        record_id: &str,
        valence_model: &str,
        _v: &Valence,
    ) -> Result<RecordOwnershipBundle> {
        let ownership_id = ownership_row_id(valence_model, record_id);
        let q = concat!(
            "RETURN { ",
            "row: (SELECT * FROM type::record($table, $record_id))[0], ",
            "ownership_status: (SELECT VALUE status FROM type::record('valence_data_ownership', $ownership_id))[0] ",
            "};"
        );
        let compiled = crate::compiled_query::CompiledQuery::new(
            q.to_string(),
            vec![
                ("table".to_string(), Value::String(table.to_string())),
                (
                    "record_id".to_string(),
                    Value::String(record_id.to_string()),
                ),
                ("ownership_id".to_string(), Value::String(ownership_id)),
            ],
        );
        let rows = backend
            .execute_compiled_query(&compiled)
            .await
            .map_err(|e| Error::Database(e.to_string()))?;
        let payload = rows.into_iter().next().unwrap_or(Value::Null);
        let obj = payload.as_object();
        let row = obj
            .and_then(|o| o.get("row"))
            .filter(|v| !v.is_null())
            .cloned();
        let ownership_status = OwnershipGateStatus::from_optional_status(
            obj.and_then(|o| o.get("ownership_status")).cloned(),
        );
        Ok(RecordOwnershipBundle {
            row,
            ownership_status,
        })
    }

    /// Privacy check plus pending-deletion gate using a bundled ownership status.
    /// # Errors
    ///
    /// Returns an error when the requested operation cannot be completed.
    pub async fn check_privacy_with_bundled_gate<F>(
        privacy_fut: F,
        valence_model: &str,
        bare_id: &str,
        status: OwnershipGateStatus,
    ) -> Result<()>
    where
        F: std::future::Future<Output = Result<()>>,
    {
        privacy_fut.await?;
        Self::apply_pending_deletion_gate(valence_model, bare_id, status)
    }

    /// Run `check_read_privacy` and the pending-deletion gate with identical semantics to the
    /// legacy sequential path: privacy deny first, then `PendingDeletion` over privacy allow.
    /// # Errors
    ///
    /// Returns an error when the requested operation cannot be completed.
    pub async fn check_privacy_with_pending_gate<F>(
        privacy_fut: F,
        valence_model: &str,
        bare_id: &str,
        v: &Valence,
    ) -> Result<()>
    where
        F: std::future::Future<Output = Result<()>>,
    {
        if !super::helpers::ownership_get_join_enabled() {
            privacy_fut.await?;
            return Self::pending_deletion_gate(valence_model, bare_id, v).await;
        }
        let (privacy_res, gate_res) = tokio::join!(
            privacy_fut,
            Self::pending_deletion_gate(valence_model, bare_id, v)
        );
        privacy_res?;
        gate_res?;
        Ok(())
    }
}
