//! TTL policy types and legacy adapter trait.

use crate::error::Result;
use serde::{Deserialize, Serialize};

/// Table-level time-to-live policy from the schema DSL (`ttl: { seconds, mode }`).
///
/// Expiry is **create-only**: set when a row is created (or creating upsert);
/// updates and merges do not refresh the clock.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaTtlPolicy {
    /// Lifetime in seconds from create time.
    pub seconds: u64,
    /// Policy mode string; default from the DSL is `"backend_capability"`.
    pub mode: String,
}

/// Whether a storage adapter can enforce schema TTL natively.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendTtlCapability {
    /// Engine deletes expired rows (Redis `EXPIRE`, Mongo TTL index).
    SupportedNative,
    /// Rows are stamped with [`super::EXPIRE_AT_FIELD`]; host must wire a platform sweeper (Future).
    Deferred,
    /// No native TTL and no stamp path for this engine.
    Unsupported,
}

/// Legacy dual surface — prefer [`crate::DatabaseBackend::ttl_capability`] /
/// [`crate::DatabaseBackend::apply_ttl_policy`]. Do not add new implementors.
#[async_trait::async_trait]
pub trait BackendTtlAdapter: Send + Sync {
    /// Capability for this adapter.
    fn capability(&self) -> BackendTtlCapability;
    /// Apply a table TTL policy when supported.
    async fn apply_table_policy(&self, table: &str, policy: &SchemaTtlPolicy) -> Result<()>;
}
