//! [`DatabaseBackend`] port and capability metadata.

use crate::compiled_query::CompiledQuery;
use crate::error::{Error, Result};
use crate::record_id::RecordId;
use crate::ttl::{BackendTtlCapability, SchemaTtlPolicy};
use std::any::Any;

/// Capabilities advertised by a storage adapter (telemetry labels and contract tests).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackendCapabilities {
    /// Whether [`DatabaseBackend::merge_record`] is supported (not the default error stub).
    pub supports_merge: bool,
    /// Whether graph edge methods (`relate_edge`, `unrelate_edge`, `get_edge_targets`) are supported.
    pub supports_graph_edges: bool,
    /// Short label attached to instrumentation counters (e.g. `"mem"`, `"surrealdb"`).
    pub telemetry_label: &'static str,
}

impl BackendCapabilities {
    /// Capabilities for the in-memory reference adapter.
    #[must_use]
    pub const fn mem() -> Self {
        Self {
            supports_merge: true,
            supports_graph_edges: true,
            telemetry_label: "mem",
        }
    }
}

/// Storage engine behind Valence: CRUD, compiled queries, and graph edges.
///
/// `valence-core` defines this trait only — no engine SDKs. Third-party adapters
/// implement it in separate crates and register instances on [`crate::ValenceBuilder`].
///
/// | Concern | Contract |
/// |---------|----------|
/// | [`engine_id`](Self::engine_id) | **Open** slug (not a closed enum) |
/// | [`capabilities`](Self::capabilities) | Merge/graph support + telemetry label |
/// | CRUD / queries / edges / TTL | Per-method contracts below |
///
/// Router keys combine a logical name with [`engine_id`](Self::engine_id) via
/// [`crate::router_key()`].
///
/// # Implementing a published adapter
///
/// 1. Depend on `valence-core` only (+ `async-trait`, etc.).
/// 2. `impl DatabaseBackend` and export `pub const ENGINE_ID: &str`.
/// 3. Export a schema evaluator, e.g. `pub const PRIMARY: DatabaseFromEngine =
///    Database::from_engine("primary", ENGINE_ID)`.
/// 4. Optional: `XBackend::builder()` with explicit setters; `from_env_defaults()` fills
///    **unset** fields only.
/// 5. Host wires with `.add_backend("primary", Arc::new(adapter))` — no public crate feature.
///
/// Reference: `examples/acme-valence-backend-stub`.
///
/// # Examples
///
/// ```
/// use std::sync::Arc;
/// use valence_backend_mem::{InMemoryBackend, ENGINE_ID};
/// use valence_core::{DatabaseBackend, Valence};
///
/// let backend = Arc::new(InMemoryBackend::new());
/// assert_eq!(backend.engine_id(), ENGINE_ID);
/// let valence = Valence::builder()
///     .add_backend("default", backend)
///     .build()
///     .expect("build");
/// assert_eq!(
///     valence.active_backend().unwrap().engine_id(),
///     ENGINE_ID
/// );
/// ```
#[async_trait::async_trait]
pub trait DatabaseBackend: Send + Sync + std::fmt::Debug + 'static {
    /// Stable **open** engine slug for router keys (see [`crate::router_key()`]).
    ///
    /// First-party constants live in [`crate::KnownEngines`] for ergonomics — that is not
    /// a closed set; third-party crates define their own slugs.
    fn engine_id(&self) -> &'static str;

    /// Adapter capabilities for contract tests and telemetry.
    fn capabilities(&self) -> BackendCapabilities;

    #[doc(hidden)]
    fn as_any_local(&self) -> Option<&dyn Any> {
        None
    }

    /// Select namespace/database on engines that support multi-tenant routing.
    ///
    /// **Contract:** default implementation is a no-op; remote adapters may override.
    async fn use_namespace(&self, ns: &str, db_name: &str) -> Result<()> {
        let _ = (ns, db_name);
        Ok(())
    }

    /// Execute a compiled admin/query statement and return JSON rows.
    ///
    /// **Contract:** must honor parameter binding in `compiled`; empty result sets return `Ok(vec![])`.
    async fn execute_compiled_query(
        &self,
        compiled: &CompiledQuery,
    ) -> Result<Vec<serde_json::Value>>;

    /// Ensure a schemaless table exists before first write.
    ///
    /// **Contract:** default implementation is a no-op; adapters may create tables lazily elsewhere.
    async fn ensure_schemaless_table(&self, table: &str) -> Result<()> {
        let _ = table;
        Ok(())
    }

    /// Fetch one record by primary key.
    ///
    /// **Contract:** returns `Ok(None)` when the row does not exist.
    async fn get_record(&self, table: &str, id: &str) -> Result<Option<serde_json::Value>>;

    /// Insert a new record; content must include any required fields.
    ///
    /// **Contract:** returns the persisted row (including server-assigned fields when applicable).
    async fn create_record(
        &self,
        table: &str,
        content: serde_json::Value,
    ) -> Result<serde_json::Value>;

    /// Replace an existing record by id.
    ///
    /// **Contract:** returns the updated row; errors when the id is missing unless the adapter
    /// supports upsert semantics via [`Self::upsert_record`].
    async fn update_record(
        &self,
        table: &str,
        id: &str,
        content: serde_json::Value,
    ) -> Result<serde_json::Value>;

    /// Patch an existing record with a partial JSON object.
    ///
    /// **Contract:** default returns `Error::Internal` — override when [`BackendCapabilities::supports_merge`]
    /// is `true`.
    async fn merge_record(
        &self,
        table: &str,
        id: &str,
        patch: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let _ = (table, id, patch);
        Err(Error::Internal(
            "merge_record is not supported by this database backend".into(),
        ))
    }

    /// Create or replace a record by explicit id.
    async fn upsert_record(
        &self,
        table: &str,
        id: &str,
        content: serde_json::Value,
    ) -> Result<serde_json::Value>;

    /// Delete one record by primary key.
    ///
    /// **Contract:** succeeds when the row is already absent (idempotent delete).
    async fn delete_record(&self, table: &str, id: &str) -> Result<()>;

    /// Create a directed graph edge from `from` to `to` through `edge_table`.
    async fn relate_edge(&self, from: &RecordId, edge_table: &str, to: &RecordId) -> Result<()>;

    /// Remove a directed graph edge.
    ///
    /// **Contract:** idempotent when the edge does not exist.
    async fn unrelate_edge(&self, from: &RecordId, edge_table: &str, to: &RecordId) -> Result<()>;

    /// List target record ids reachable via outgoing edges in `edge_table`.
    async fn get_edge_targets(&self, from: &RecordId, edge_table: &str) -> Result<Vec<RecordId>>;

    /// Define a unique index on `table.field` when the engine supports DDL.
    ///
    /// **Contract:** default returns `Error::Internal`.
    async fn define_unique_index(&self, table: &str, field: &str) -> Result<()> {
        let _ = (table, field);
        Err(Error::Internal(format!(
            "define_unique_index not supported for {}",
            self.engine_id()
        )))
    }

    /// Whether this adapter can apply schema TTL policies natively.
    fn ttl_capability(&self) -> BackendTtlCapability {
        BackendTtlCapability::Unsupported
    }

    /// Apply a schema TTL policy to `table` when supported.
    ///
    /// **Contract:** default is a no-op.
    async fn apply_ttl_policy(&self, _table: &str, _policy: &SchemaTtlPolicy) -> Result<()> {
        Ok(())
    }
}
