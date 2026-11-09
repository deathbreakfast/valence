//! Batch create hooks for codegen `BatchCreatable` impls (full batch runtime is host-owned).

use async_trait::async_trait;
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::error::Result;
use crate::record_id::RecordId;
use crate::runtime::Valence;

/// Marker trait for models participating in batch create codegen.
#[async_trait]
pub trait BatchCreatable: Serialize + DeserializeOwned + Send + 'static {
    fn table_name() -> &'static str;

    fn set_id(&mut self, id: RecordId);

    /// Checks a row before the host inserts it.
    ///
    /// The host's batch runtime must call this on every row it serializes. Generated
    /// models override it to refuse a row that leaves out a required field whose
    /// `read` policy can hide it, with the same `<table>.<field> is required`
    /// validation error `Model::create` returns.
    ///
    /// # Errors
    ///
    /// [`Error::Validation`](crate::error::Error::Validation) when a required field is
    /// missing or null.
    fn validate_batch_row(_row: &serde_json::Value) -> Result<()> {
        Ok(())
    }

    async fn ensure_ownership_after_batch_create(
        _created_row: serde_json::Value,
        _valence: &Valence,
    ) -> Result<()> {
        Ok(())
    }
}
