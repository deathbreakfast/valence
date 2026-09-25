//! Free-function declared Unscoped wrappers for [`DatabaseBackend`] product call sites.
//!
//! The trait stays object-safe without purpose args; these wrappers discard purpose
//! at runtime and forward to the bare port methods for catalog transparency.
//!
//! Callers that hold an <code>[Arc]&lt;dyn DatabaseBackend&gt;</code> (from
//! [`crate::runtime::Valence::backend_for_table`]) should pass `backend.as_ref()`.

use crate::data_use::DataUsePurpose;
use crate::error::Result;

use super::DatabaseBackend;

/// Declared Unscoped `get_record` (same as [`DatabaseBackend::get_record`]).
pub async fn get_record(
    backend: &dyn DatabaseBackend,
    table: &str,
    id: &str,
    purpose: DataUsePurpose,
) -> Result<Option<serde_json::Value>> {
    let _ = purpose;
    backend.get_record(table, id).await
}

/// Declared Unscoped `create_record`.
pub async fn create_record(
    backend: &dyn DatabaseBackend,
    table: &str,
    content: serde_json::Value,
    purpose: DataUsePurpose,
) -> Result<serde_json::Value> {
    let _ = purpose;
    backend.create_record(table, content).await
}

/// Declared Unscoped `update_record`.
pub async fn update_record(
    backend: &dyn DatabaseBackend,
    table: &str,
    id: &str,
    content: serde_json::Value,
    purpose: DataUsePurpose,
) -> Result<serde_json::Value> {
    let _ = purpose;
    backend.update_record(table, id, content).await
}

/// Declared Unscoped `merge_record`.
pub async fn merge_record(
    backend: &dyn DatabaseBackend,
    table: &str,
    id: &str,
    patch: serde_json::Value,
    purpose: DataUsePurpose,
) -> Result<serde_json::Value> {
    let _ = purpose;
    backend.merge_record(table, id, patch).await
}

/// Declared Unscoped `upsert_record`.
pub async fn upsert_record(
    backend: &dyn DatabaseBackend,
    table: &str,
    id: &str,
    content: serde_json::Value,
    purpose: DataUsePurpose,
) -> Result<serde_json::Value> {
    let _ = purpose;
    backend.upsert_record(table, id, content).await
}

/// Declared Unscoped `delete_record`.
pub async fn delete_record(
    backend: &dyn DatabaseBackend,
    table: &str,
    id: &str,
    purpose: DataUsePurpose,
) -> Result<()> {
    let _ = purpose;
    backend.delete_record(table, id).await
}
