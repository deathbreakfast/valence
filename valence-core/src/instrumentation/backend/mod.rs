//! [`DatabaseBackend`] decorator for read/write/error telemetry.

mod reads;
mod writes;

use std::sync::Arc;

use async_trait::async_trait;

use crate::backend::DatabaseBackend;
use crate::error::Result;
use crate::ttl::{BackendTtlCapability, SchemaTtlPolicy};

use super::metrics;

/// Wraps an inner backend and emits instrumentation telemetry on every I/O call.
#[derive(Debug)]
pub struct InstrumentedBackend {
    pub(super) inner: Arc<dyn DatabaseBackend>,
}

impl InstrumentedBackend {
    #[must_use]
    pub fn new(inner: Arc<dyn DatabaseBackend>) -> Self {
        Self { inner }
    }

    pub(super) fn telemetry_label(&self) -> &'static str {
        self.inner.capabilities().telemetry_label
    }

    pub(super) fn on_err(&self, operation: &str, err: &crate::error::Error) {
        metrics::record_db_error(operation, self.telemetry_label(), &err.to_string());
    }

    pub(super) fn record_io_timing(
        &self,
        operation: &str,
        table: &str,
        op: &str,
        wall_ms: f64,
        record_id: Option<&str>,
    ) {
        let label = self.telemetry_label();
        metrics::record_db_wall_ms(table, label, op, wall_ms);
        metrics::maybe_record_slow_op(operation, table, op, label, wall_ms, record_id);
    }
}

/// Wrap `inner` with [`InstrumentedBackend`].
pub fn wrap_backend(inner: Arc<dyn DatabaseBackend>) -> Arc<dyn DatabaseBackend> {
    Arc::new(InstrumentedBackend::new(inner))
}

#[async_trait]
impl DatabaseBackend for InstrumentedBackend {
    fn engine_id(&self) -> &'static str {
        self.inner.engine_id()
    }

    fn capabilities(&self) -> crate::backend::BackendCapabilities {
        self.inner.capabilities()
    }

    fn as_any_local(&self) -> Option<&dyn std::any::Any> {
        self.inner.as_any_local()
    }

    async fn use_namespace(&self, ns: &str, db_name: &str) -> Result<()> {
        self.inner.use_namespace(ns, db_name).await
    }

    async fn ensure_schemaless_table(&self, table: &str) -> Result<()> {
        self.inner.ensure_schemaless_table(table).await
    }

    async fn define_unique_index(&self, table: &str, field: &str) -> Result<()> {
        self.inner.define_unique_index(table, field).await
    }

    fn ttl_capability(&self) -> BackendTtlCapability {
        self.inner.ttl_capability()
    }

    async fn apply_ttl_policy(&self, table: &str, policy: &SchemaTtlPolicy) -> Result<()> {
        self.inner.apply_ttl_policy(table, policy).await
    }

    async fn execute_compiled_query(
        &self,
        compiled: &crate::compiled_query::CompiledQuery,
    ) -> Result<Vec<serde_json::Value>> {
        self.measured_execute_compiled_query(compiled).await
    }

    async fn get_record(&self, table: &str, id: &str) -> Result<Option<serde_json::Value>> {
        self.measured_get_record(table, id).await
    }

    async fn get_edge_targets(
        &self,
        from: &crate::RecordId,
        edge_table: &str,
    ) -> Result<Vec<crate::RecordId>> {
        self.measured_get_edge_targets(from, edge_table).await
    }

    async fn create_record(
        &self,
        table: &str,
        content: serde_json::Value,
    ) -> Result<serde_json::Value> {
        self.measured_create_record(table, content).await
    }

    async fn update_record(
        &self,
        table: &str,
        id: &str,
        content: serde_json::Value,
    ) -> Result<serde_json::Value> {
        self.measured_update_record(table, id, content).await
    }

    async fn merge_record(
        &self,
        table: &str,
        id: &str,
        patch: serde_json::Value,
    ) -> Result<serde_json::Value> {
        self.measured_merge_record(table, id, patch).await
    }

    async fn upsert_record(
        &self,
        table: &str,
        id: &str,
        content: serde_json::Value,
    ) -> Result<serde_json::Value> {
        self.measured_upsert_record(table, id, content).await
    }

    async fn delete_record(&self, table: &str, id: &str) -> Result<()> {
        self.measured_delete_record(table, id).await
    }

    async fn relate_edge(
        &self,
        from: &crate::RecordId,
        edge_table: &str,
        to: &crate::RecordId,
    ) -> Result<()> {
        self.measured_relate_edge(from, edge_table, to).await
    }

    async fn unrelate_edge(
        &self,
        from: &crate::RecordId,
        edge_table: &str,
        to: &crate::RecordId,
    ) -> Result<()> {
        self.measured_unrelate_edge(from, edge_table, to).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiled_query::CompiledQuery;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Debug)]
    struct MockBackend {
        gets: AtomicUsize,
    }

    #[async_trait]
    impl DatabaseBackend for MockBackend {
        fn engine_id(&self) -> &'static str {
            "mem"
        }

        fn capabilities(&self) -> crate::backend::BackendCapabilities {
            crate::backend::BackendCapabilities::mem()
        }

        async fn execute_compiled_query(
            &self,
            _compiled: &CompiledQuery,
        ) -> Result<Vec<serde_json::Value>> {
            Ok(vec![])
        }

        async fn get_record(&self, _table: &str, _id: &str) -> Result<Option<serde_json::Value>> {
            self.gets.fetch_add(1, Ordering::SeqCst);
            Ok(None)
        }

        async fn create_record(
            &self,
            _table: &str,
            _content: serde_json::Value,
        ) -> Result<serde_json::Value> {
            Ok(serde_json::json!({}))
        }

        async fn update_record(
            &self,
            _table: &str,
            _id: &str,
            _content: serde_json::Value,
        ) -> Result<serde_json::Value> {
            Ok(serde_json::json!({}))
        }

        async fn upsert_record(
            &self,
            _table: &str,
            _id: &str,
            _content: serde_json::Value,
        ) -> Result<serde_json::Value> {
            Ok(serde_json::json!({}))
        }

        async fn delete_record(&self, _table: &str, _id: &str) -> Result<()> {
            Ok(())
        }

        async fn relate_edge(
            &self,
            _from: &crate::RecordId,
            _edge_table: &str,
            _to: &crate::RecordId,
        ) -> Result<()> {
            Ok(())
        }

        async fn unrelate_edge(
            &self,
            _from: &crate::RecordId,
            _edge_table: &str,
            _to: &crate::RecordId,
        ) -> Result<()> {
            Ok(())
        }

        async fn get_edge_targets(
            &self,
            _from: &crate::RecordId,
            _edge_table: &str,
        ) -> Result<Vec<crate::RecordId>> {
            Ok(vec![])
        }
    }

    #[tokio::test]
    async fn decorator_delegates_to_inner() {
        let inner = Arc::new(MockBackend {
            gets: AtomicUsize::new(0),
        });
        let wrapped = wrap_backend(Arc::<MockBackend>::clone(&inner));
        let _ = wrapped.get_record("t", "id").await;
        assert_eq!(inner.gets.load(Ordering::SeqCst), 1);
    }
}
