//! Ownership write paths: ensure, status transitions, and transfers.

use serde_json::json;

use crate::error::{Error, Result};
use crate::owner_ref::OwnerRef;
use crate::runtime::Valence;

use super::helpers::{
    append_transfer_history_row, ownership_row_id, skip_ownership_for_table, system_valence,
};
use super::OwnershipService;

impl OwnershipService {
    /// Insert or replace the active ownership row for `valence_model` / `record_id`.
    /// # Errors
    ///
    /// Returns an error when the requested operation cannot be completed.
    pub async fn ensure_active_ownership(
        valence_model: &str,
        record_id: &str,
        owner: OwnerRef,
        v: &Valence,
    ) -> Result<()> {
        if skip_ownership_for_table(valence_model) {
            return Ok(());
        }

        let id = ownership_row_id(valence_model, record_id);
        let sys = system_valence(v);
        let backend = Self::ownership_backend(valence_model, &sys)?;
        let row = json!({
            "id": id,
            "valence_model": valence_model,
            "record_id": record_id,
            "owner_id": owner.owner_id,
            "owner_type": owner.owner_kind.as_str(),
            "status": "active",
        });
        backend
            .upsert_record("valence_data_ownership", id.as_str(), row)
            .await
            .map_err(|e| Error::Database(e.to_string()))?;
        crate::read_cache::invalidate("valence_data_ownership", &id);
        crate::read_cache::invalidate(valence_model, record_id);
        Ok(())
    }

    /// Set ownership status to `deleted` after the physical row has been removed (audit trail).
    /// # Errors
    ///
    /// Returns an error when the requested operation cannot be completed.
    pub async fn mark_deleted_ownership(
        valence_model: &str,
        record_id: &str,
        v: &Valence,
    ) -> Result<()> {
        if skip_ownership_for_table(valence_model) {
            return Ok(());
        }
        let id = ownership_row_id(valence_model, record_id);
        let sys = system_valence(v);
        let backend = Self::ownership_backend(valence_model, &sys)?;
        let patch = json!({ "status": "deleted" });
        backend
            .merge_record("valence_data_ownership", &id, patch)
            .await
            .map_err(|e| Error::Database(e.to_string()))
            .map(|_| ())?;
        crate::read_cache::invalidate("valence_data_ownership", &id);
        crate::read_cache::invalidate(valence_model, record_id);
        Ok(())
    }

    /// Mark ownership as pending deletion (called before the row delete side-effects).
    /// # Errors
    ///
    /// Returns an error when the requested operation cannot be completed.
    pub async fn mark_pending_deletion(
        valence_model: &str,
        record_id: &str,
        v: &Valence,
    ) -> Result<()> {
        if skip_ownership_for_table(valence_model) {
            return Ok(());
        }
        let id = ownership_row_id(valence_model, record_id);
        let sys = system_valence(v);
        let backend = Self::ownership_backend(valence_model, &sys)?;
        let row = json!({
            "id": id,
            "valence_model": valence_model,
            "record_id": record_id,
            "status": "pending_deletion",
        });
        backend
            .upsert_record("valence_data_ownership", id.as_str(), row)
            .await
            .map_err(|e| Error::Database(e.to_string()))
            .map(|_| ())?;
        crate::read_cache::invalidate("valence_data_ownership", &id);
        crate::read_cache::invalidate(valence_model, record_id);
        Ok(())
    }

    /// Transfer ownership and append a history row.
    /// # Errors
    ///
    /// Returns an error when the requested operation cannot be completed.
    pub async fn transfer_ownership(
        valence_model: &str,
        record_id: &str,
        new_owner: OwnerRef,
        reason: Option<String>,
        v: &Valence,
    ) -> Result<()> {
        let existing = Self::get_ownership_json(valence_model, record_id, v)
            .await?
            .ok_or_else(|| {
                Error::NotFound(format!("ownership missing for {valence_model}:{record_id}"))
            })?;

        let from_owner_id = existing
            .get("owner_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let from_owner_type = existing
            .get("owner_type")
            .and_then(|v| v.as_str())
            .unwrap_or("system")
            .to_string();

        let oid = ownership_row_id(valence_model, record_id);
        let sys = system_valence(v);
        let backend = Self::ownership_backend(valence_model, &sys)?;
        let merged = json!({
            "owner_id": new_owner.owner_id,
            "owner_type": new_owner.owner_kind.as_str(),
            "status": "active",
        });
        backend
            .merge_record("valence_data_ownership", &oid, merged)
            .await
            .map_err(|e| Error::Database(e.to_string()))
            .map(|_| ())?;
        crate::read_cache::invalidate("valence_data_ownership", &oid);
        crate::read_cache::invalidate(valence_model, record_id);

        append_transfer_history_row(
            valence_model,
            record_id,
            &oid,
            &from_owner_id,
            &from_owner_type,
            &new_owner,
            reason,
            v,
        )
        .await
    }
}
