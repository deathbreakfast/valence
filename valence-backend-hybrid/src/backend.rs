//! Thin [`DatabaseBackend`] dispatch for the hybrid IndraDB + SQL adapter.

use std::sync::Arc;

use serde_json::Value;
use valence_backend_indradb::IndradbBackend;
use valence_core::compiled_query::CompiledQuery;
use valence_core::error::Result;
use valence_core::record_id::RecordId;
use valence_core::ttl::{BackendTtlCapability, SchemaTtlPolicy};
use valence_core::{
    BackendCapabilities, Database, DatabaseBackend, DatabaseFromEngine, KnownEngines,
};

use crate::builder::HybridBackendBuilder;
use crate::cache_policy::CachePolicy;
use crate::edge_cache::EdgeCache;
use crate::hop_exec::{execute_hop_plan, parse_hop_plan};
use crate::record_cache::RecordCache;
use crate::write_through;

/// Stable engine slug for router keys (`hybrid_indra_sql:logical_name`).
pub const ENGINE_ID: &str = KnownEngines::HYBRID_INDRA_SQL;

/// Schema evaluator const for `database:` routing (`primary` is the logical name in the key).
///
/// Hosts typically also register the same [`HybridBackend`] under application logical names
/// such as `default`, `billing`, or `jobs` via
/// [`valence_core::register_backend_logical_names_slices`]. Sharing one backend across those
/// keys is intentional: one Indra mirror, one primary, and one shared cache.
pub const PRIMARY: DatabaseFromEngine = Database::from_engine("primary", ENGINE_ID);

/// Postgres/SQL primary with an embedded IndraDB read-through cache for records and edges.
///
/// # Examples
///
/// Read-through get (miss populates the mirror):
///
/// ```no_run
/// use std::sync::Arc;
/// use valence_backend_hybrid::HybridBackend;
/// use valence_backend_mem::InMemoryBackend;
/// use valence_core::DatabaseBackend;
///
/// # async fn demo() -> valence_core::Result<()> {
/// let primary: Arc<dyn DatabaseBackend> = Arc::new(InMemoryBackend::new());
/// primary
///     .create_record("counter", serde_json::json!({"id": "1", "value": 7}))
///     .await?;
/// let hybrid = HybridBackend::builder().primary(primary).warm_edges(false).build().await?;
/// let row = hybrid.get_record("counter", "1").await?;
/// assert!(row.is_some());
/// # Ok(())
/// # }
/// ```
pub struct HybridBackend {
    pub(crate) primary: Arc<dyn DatabaseBackend>,
    pub(crate) mirror: IndradbBackend,
    pub(crate) records: RecordCache,
    pub(crate) edges: EdgeCache,
    pub(crate) policy: CachePolicy,
}

impl std::fmt::Debug for HybridBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HybridBackend")
            .field("primary_engine", &self.primary.engine_id())
            .field("policy", &self.policy)
            .finish_non_exhaustive()
    }
}

impl HybridBackend {
    /// Start a fluent builder.
    #[must_use]
    pub fn builder() -> HybridBackendBuilder {
        HybridBackendBuilder::new()
    }

    /// Resolved cache policy (capacities and rules).
    #[must_use]
    pub fn policy(&self) -> &CachePolicy {
        &self.policy
    }

    /// Read-through get: mirror hit, else primary populate.
    async fn get_record_inner(&self, table: &str, id: &str) -> Result<Option<Value>> {
        if let Some(hit) = self
            .records
            .get(&self.mirror, &self.policy, table, id)
            .await?
        {
            crate::telemetry::record_cache_hit("record");
            return Ok(Some(hit));
        }
        crate::telemetry::record_cache_miss("record");
        let row = self.primary.get_record(table, id).await?;
        if let Some(ref body) = row {
            let _ = self
                .records
                .put(&self.mirror, &self.policy, table, id, body.clone())
                .await;
        }
        Ok(row)
    }
}

#[async_trait::async_trait]
impl DatabaseBackend for HybridBackend {
    fn engine_id(&self) -> &'static str {
        ENGINE_ID
    }

    fn capabilities(&self) -> BackendCapabilities {
        let primary_caps = self.primary.capabilities();
        BackendCapabilities {
            supports_merge: primary_caps.supports_merge,
            supports_graph_edges: true,
            telemetry_label: "hybrid",
        }
    }

    async fn use_namespace(&self, ns: &str, db_name: &str) -> Result<()> {
        self.primary.use_namespace(ns, db_name).await
    }

    async fn execute_compiled_query(&self, compiled: &CompiledQuery) -> Result<Vec<Value>> {
        if let Some(plan) = parse_hop_plan(&compiled.query_string) {
            return execute_hop_plan(
                &self.primary,
                &self.mirror,
                &self.records,
                &self.edges,
                &self.policy,
                &plan,
            )
            .await;
        }
        self.primary.execute_compiled_query(compiled).await
    }

    async fn ensure_schemaless_table(&self, table: &str) -> Result<()> {
        self.primary.ensure_schemaless_table(table).await
    }

    async fn get_record(&self, table: &str, id: &str) -> Result<Option<Value>> {
        self.get_record_inner(table, id).await
    }

    async fn create_record(&self, table: &str, content: Value) -> Result<Value> {
        write_through::create_record(
            &self.primary,
            &self.mirror,
            &self.records,
            &self.policy,
            table,
            content,
        )
        .await
    }

    async fn update_record(&self, table: &str, id: &str, content: Value) -> Result<Value> {
        write_through::update_record(
            &self.primary,
            &self.mirror,
            &self.records,
            &self.policy,
            table,
            id,
            content,
        )
        .await
    }

    async fn merge_record(&self, table: &str, id: &str, patch: Value) -> Result<Value> {
        write_through::merge_record(
            &self.primary,
            &self.mirror,
            &self.records,
            &self.policy,
            table,
            id,
            patch,
        )
        .await
    }

    async fn upsert_record(&self, table: &str, id: &str, content: Value) -> Result<Value> {
        write_through::upsert_record(
            &self.primary,
            &self.mirror,
            &self.records,
            &self.policy,
            table,
            id,
            content,
        )
        .await
    }

    async fn delete_record(&self, table: &str, id: &str) -> Result<()> {
        write_through::delete_record(&self.primary, &self.mirror, &self.records, table, id).await
    }

    async fn relate_edge(&self, from: &RecordId, edge_table: &str, to: &RecordId) -> Result<()> {
        write_through::relate_edge(
            &self.primary,
            &self.mirror,
            &self.edges,
            &self.policy,
            from,
            edge_table,
            to,
        )
        .await
    }

    async fn unrelate_edge(&self, from: &RecordId, edge_table: &str, to: &RecordId) -> Result<()> {
        write_through::unrelate_edge(
            &self.primary,
            &self.mirror,
            &self.edges,
            from,
            edge_table,
            to,
        )
        .await
    }

    async fn get_edge_targets(&self, from: &RecordId, edge_table: &str) -> Result<Vec<RecordId>> {
        if self.policy.caches_edge(edge_table) && self.edges.is_complete(edge_table) {
            crate::telemetry::record_cache_hit("edge");
            return self.mirror.get_edge_targets(from, edge_table).await;
        }
        crate::telemetry::record_cache_miss("edge");
        self.primary.get_edge_targets(from, edge_table).await
    }

    async fn define_unique_index(&self, table: &str, field: &str) -> Result<()> {
        self.primary.define_unique_index(table, field).await
    }

    fn ttl_capability(&self) -> BackendTtlCapability {
        self.primary.ttl_capability()
    }

    async fn apply_ttl_policy(&self, table: &str, policy: &SchemaTtlPolicy) -> Result<()> {
        self.primary.apply_ttl_policy(table, policy).await
    }
}
