//! Valence runtime handle and builder.

mod builder;
mod factory;

pub use builder::ValenceBuilder;
pub use factory::{RouterValenceFactory, RouterValenceFactoryConfig, ValenceFactory};

use crate::actor::Actor;
use crate::backend::DatabaseBackend;
use crate::error::Result;
use crate::owner_ref::OwnerRef;
use crate::ports::actor::ActorFactory;
use crate::ports::endpoints::DatabaseEndpointResolver;
use crate::ports::secrets::SecretProvider;
use crate::record_id::RecordId;
use crate::request_cache::RequestPermissionCache;
use crate::router::DatabaseRouter;
use crate::router_key::router_key;
use crate::schema::SchemaRegistry;
use std::sync::Arc;
use valence_telemetry::TelemetrySink;

/// Host-assembled Valence handle.
#[derive(Clone)]
pub struct Valence {
    pub(crate) router: Arc<DatabaseRouter>,
    pub(crate) active_backend_key: String,
    telemetry_sink: Arc<dyn TelemetrySink>,
    secret_provider: Arc<dyn SecretProvider>,
    actor_factory: Arc<dyn ActorFactory>,
    endpoint_resolver: Arc<dyn DatabaseEndpointResolver>,
    actor: Actor,
    owner_override: Option<OwnerRef>,
    permission_cache: Option<RequestPermissionCache>,
}

impl Valence {
    pub fn builder() -> ValenceBuilder {
        ValenceBuilder::new()
    }

    pub fn database_router(&self) -> &Arc<DatabaseRouter> {
        &self.router
    }

    /// # Errors
    ///
    /// Returns an error when the requested operation cannot be completed.
    pub fn active_backend(&self) -> Result<Arc<dyn DatabaseBackend>> {
        self.router.resolve(&self.active_backend_key)
    }

    /// # Errors
    ///
    /// Returns an error when the requested operation cannot be completed.
    pub fn backend_for_table(&self, table: &str) -> Result<Arc<dyn DatabaseBackend>> {
        let Some(meta) = SchemaRegistry::global().get_schema(table) else {
            return self.active_backend();
        };
        let eval = meta.schema.database_evaluator;
        let key = router_key(eval.logical_name(), eval.engine_id());
        self.router
            .resolve(&key)
            .or_else(|_| self.active_backend())
            .inspect_err(|_e| {
                crate::instrumentation::metrics::record_router_resolve_error(table);
            })
    }

    pub fn telemetry_sink(&self) -> &Arc<dyn TelemetrySink> {
        &self.telemetry_sink
    }

    pub fn secret_provider(&self) -> &Arc<dyn SecretProvider> {
        &self.secret_provider
    }

    pub fn actor_factory(&self) -> &Arc<dyn ActorFactory> {
        &self.actor_factory
    }

    pub fn endpoint_resolver(&self) -> &Arc<dyn DatabaseEndpointResolver> {
        &self.endpoint_resolver
    }

    pub fn actor(&self) -> &Actor {
        &self.actor
    }

    #[must_use]
    pub fn with_actor(&self, actor: Actor) -> Self {
        Self {
            actor,
            ..self.clone()
        }
    }

    #[must_use]
    pub fn with_owner_override(&self, owner: OwnerRef) -> Self {
        Self {
            owner_override: Some(owner),
            ..self.clone()
        }
    }

    pub fn owner_override(&self) -> Option<&OwnerRef> {
        self.owner_override.as_ref()
    }

    /// Request-scoped permission check cache, when enabled on the builder.
    pub fn permission_cache(&self) -> Option<&RequestPermissionCache> {
        self.permission_cache.as_ref()
    }

    /// Navigate ManyToMany edges and return target [`RecordId`] values.
    /// # Errors
    ///
    /// Returns an error when the requested operation cannot be completed.
    pub async fn get_many_to_many_target_record_ids(
        &self,
        from: &RecordId,
        edge_table: &str,
    ) -> Result<Vec<RecordId>> {
        let backend = self.active_backend()?;
        backend.get_edge_targets(from, edge_table).await
    }

    /// Create a graph edge between two records in `edge_table`.
    /// # Errors
    ///
    /// Returns an error when the requested operation cannot be completed.
    pub async fn relate_edge(
        &self,
        edge_table: &str,
        from: &RecordId,
        to: &RecordId,
    ) -> Result<()> {
        let backend = self.active_backend()?;
        backend.relate_edge(from, edge_table, to).await
    }

    /// Delete a graph edge between two records in `edge_table`.
    /// # Errors
    ///
    /// Returns an error when the requested operation cannot be completed.
    pub async fn unrelate_edge(
        &self,
        edge_table: &str,
        from: &RecordId,
        to: &RecordId,
    ) -> Result<()> {
        let backend = self.active_backend()?;
        backend.unrelate_edge(from, edge_table, to).await
    }

    /// # Errors
    ///
    /// Returns an error when the requested operation cannot be completed.
    pub async fn ensure_unique_field_index(&self, table: &str, field: &str) -> Result<()> {
        let backend = self.backend_for_table(table)?;
        backend.define_unique_index(table, field).await
    }

    pub fn is_system(&self) -> bool {
        self.actor.is_system()
    }
}

impl std::fmt::Debug for Valence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Valence")
            .field("active_backend_key", &self.active_backend_key)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::actor::JsonActorFactory;

    #[test]
    fn builder_requires_backend() {
        let err = Valence::builder()
            .actor_factory(Arc::new(JsonActorFactory))
            .build()
            .unwrap_err();
        assert!(err.to_string().contains("at least one backend"));
    }

    #[test]
    fn builder_accepts_actor_with_backend() {
        use crate::backend::DatabaseBackend;
        use async_trait::async_trait;
        use std::sync::atomic::{AtomicUsize, Ordering};

        #[derive(Debug)]
        struct MockBackend;

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
                _compiled: &crate::compiled_query::CompiledQuery,
            ) -> crate::error::Result<Vec<serde_json::Value>> {
                Ok(vec![])
            }

            async fn get_record(
                &self,
                _table: &str,
                _id: &str,
            ) -> crate::error::Result<Option<serde_json::Value>> {
                static GETS: AtomicUsize = AtomicUsize::new(0);
                GETS.fetch_add(1, Ordering::SeqCst);
                Ok(None)
            }

            async fn create_record(
                &self,
                _table: &str,
                _content: serde_json::Value,
            ) -> crate::error::Result<serde_json::Value> {
                Ok(serde_json::json!({}))
            }

            async fn update_record(
                &self,
                _table: &str,
                _id: &str,
                _content: serde_json::Value,
            ) -> crate::error::Result<serde_json::Value> {
                Ok(serde_json::json!({}))
            }

            async fn upsert_record(
                &self,
                _table: &str,
                _id: &str,
                _content: serde_json::Value,
            ) -> crate::error::Result<serde_json::Value> {
                Ok(serde_json::json!({}))
            }

            async fn delete_record(&self, _table: &str, _id: &str) -> crate::error::Result<()> {
                Ok(())
            }

            async fn relate_edge(
                &self,
                _from: &crate::record_id::RecordId,
                _edge_table: &str,
                _to: &crate::record_id::RecordId,
            ) -> crate::error::Result<()> {
                Ok(())
            }

            async fn unrelate_edge(
                &self,
                _from: &crate::record_id::RecordId,
                _edge_table: &str,
                _to: &crate::record_id::RecordId,
            ) -> crate::error::Result<()> {
                Ok(())
            }

            async fn get_edge_targets(
                &self,
                _from: &crate::record_id::RecordId,
                _edge_table: &str,
            ) -> crate::error::Result<Vec<crate::record_id::RecordId>> {
                Ok(vec![])
            }
        }

        let v = Valence::builder()
            .add_backend("default", Arc::new(MockBackend))
            .with_actor(Actor::System {
                operation: "test".into(),
            })
            .build()
            .expect("build");
        assert!(v.is_system());
        let _ = v.active_backend().expect("backend");
    }
}
