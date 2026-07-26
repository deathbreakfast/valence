//! Process-global [`ValenceFactory`] from a pinned [`DatabaseRouter`].
//!
//! [`RouterValenceFactory::build`] deserializes `actor_json` into a typed [`crate::actor::Actor`]
//! and attaches it to the returned [`Valence`]. Pass only host-trusted JSON (session-derived);
//! never deserialize untrusted client payloads as [`Actor::System`](crate::actor::Actor::System).
//! Install [`crate::actor_policy::RejectExternalSystemActor`] on external paths.

use std::sync::Arc;

use serde_json::Value;

use crate::actor::Actor;
use crate::actor_policy::{ActorJsonPolicy, ActorTrust};
use crate::error::{Error, Result};
use crate::ports::actor::{ActorFactory, JsonActorFactory};
use crate::ports::endpoints::DatabaseEndpointResolver;
use crate::ports::secrets::SecretProvider;
use crate::router::DatabaseRouter;
use crate::runtime::{Valence, ValenceBuilder};
use valence_telemetry::TelemetrySink;

/// Factory for reconstructing [`Valence`] instances outside request context.
pub trait ValenceFactory: Send + Sync + 'static {
    /// Build a request-scoped [`Valence`] from a JSON actor payload.
    /// # Errors
    ///
    /// Returns an error when the requested operation cannot be completed.
    fn build(&self, actor_json: &Value) -> Result<Valence>;
}

/// Host wiring template applied when building from a shared router.
#[derive(Clone)]
pub struct RouterValenceFactoryConfig {
    /// Active backend key passed to [`ValenceBuilder::default_backend_key`].
    pub default_backend_key: String,
    /// Optional telemetry sink override (defaults to no-op).
    pub telemetry_sink: Option<Arc<dyn TelemetrySink>>,
    /// Optional secret provider override (defaults to no-op).
    pub secret_provider: Option<Arc<dyn SecretProvider>>,
    /// Optional actor factory override (defaults to JSON factory).
    pub actor_factory: Option<Arc<dyn ActorFactory>>,
    /// Optional endpoint resolver override (defaults to no-op).
    pub endpoint_resolver: Option<Arc<dyn DatabaseEndpointResolver>>,
    /// Optional policy for validating `actor_json` (see [`RejectExternalSystemActor`](crate::actor_policy::RejectExternalSystemActor)).
    pub actor_json_policy: Option<Arc<dyn ActorJsonPolicy>>,
    /// Trust level passed to [`ActorJsonPolicy`] (default [`ActorTrust::External`]).
    pub actor_trust: ActorTrust,
}

impl RouterValenceFactoryConfig {
    /// Create a config with only the required default backend key.
    #[must_use]
    pub fn new(default_backend_key: impl Into<String>) -> Self {
        Self {
            default_backend_key: default_backend_key.into(),
            telemetry_sink: None,
            secret_provider: None,
            actor_factory: None,
            endpoint_resolver: None,
            actor_json_policy: None,
            actor_trust: ActorTrust::External,
        }
    }

    /// Install an [`ActorJsonPolicy`] for factory `actor_json` validation.
    #[must_use]
    pub fn actor_json_policy(mut self, policy: impl ActorJsonPolicy + 'static) -> Self {
        self.actor_json_policy = Some(Arc::new(policy));
        self
    }
}

/// [`ValenceFactory`] backed by a shared [`DatabaseRouter`].
#[derive(Clone)]
pub struct RouterValenceFactory {
    router: Arc<DatabaseRouter>,
    config: RouterValenceFactoryConfig,
}

impl RouterValenceFactory {
    /// Wrap a shared router and host wiring template.
    #[must_use]
    pub fn new(router: Arc<DatabaseRouter>, config: RouterValenceFactoryConfig) -> Self {
        Self { router, config }
    }

    /// Return an [`Arc`] factory suitable for dependency injection.
    pub fn arc(
        router: Arc<DatabaseRouter>,
        config: RouterValenceFactoryConfig,
    ) -> Arc<dyn ValenceFactory> {
        Arc::new(Self::new(router, config))
    }
}

impl ValenceFactory for RouterValenceFactory {
    fn build(&self, actor_json: &Value) -> Result<Valence> {
        if let Some(policy) = &self.config.actor_json_policy {
            policy.validate(self.config.actor_trust, actor_json)?;
        }

        let actor_factory = self
            .config
            .actor_factory
            .clone()
            .unwrap_or_else(|| Arc::new(JsonActorFactory));
        let _actor_ctx = actor_factory.build(actor_json)?;

        // Bind the typed actor into the runtime. Hosts must only pass trusted JSON
        // (e.g. session-derived); never deserialize untrusted client payloads as `Actor::System`.
        let actor: Actor = serde_json::from_value(actor_json.clone()).map_err(|e| {
            Error::Validation(format!("actor_json is not a valid valence::Actor: {e}"))
        })?;

        let mut builder = ValenceBuilder::new()
            .database_router(Arc::clone(&self.router))
            .default_backend_key(self.config.default_backend_key.clone())
            .actor_factory(actor_factory)
            .with_actor(actor);

        if let Some(sink) = &self.config.telemetry_sink {
            builder = builder.telemetry_sink(Arc::clone(sink));
        }
        if let Some(secrets) = &self.config.secret_provider {
            builder = builder.secret_provider(Arc::clone(secrets));
        }
        if let Some(endpoints) = &self.config.endpoint_resolver {
            builder = builder.endpoint_resolver(Arc::clone(endpoints));
        }

        builder.build()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actor::Actor;
    use crate::backend::DatabaseBackend;
    use async_trait::async_trait;
    use std::sync::Arc;

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

    fn factory() -> RouterValenceFactory {
        let valence = Valence::builder()
            .add_backend("default", Arc::new(MockBackend))
            .build()
            .expect("build");
        let router = Arc::clone(valence.database_router());
        RouterValenceFactory::new(router, RouterValenceFactoryConfig::new("default"))
    }

    #[test]
    fn build_binds_user_actor_from_json() {
        let f = factory();
        let actor_json = serde_json::to_value(Actor::User {
            user_id: "u1".into(),
        })
        .expect("json");
        let v = f.build(&actor_json).expect("build");
        assert_eq!(v.actor().user_id(), Some("u1"));
    }

    #[test]
    fn build_rejects_invalid_actor_json() {
        let f = factory();
        let err = f
            .build(&serde_json::json!({"kind": "not-an-actor"}))
            .expect_err("invalid");
        assert!(err.to_string().contains("actor_json"));
    }

    #[test]
    fn build_rejects_external_system_when_policy_installed() {
        use crate::actor_policy::RejectExternalSystemActor;
        let valence = Valence::builder()
            .add_backend("default", Arc::new(MockBackend))
            .build()
            .expect("build");
        let router = Arc::clone(valence.database_router());
        let config =
            RouterValenceFactoryConfig::new("default").actor_json_policy(RejectExternalSystemActor);
        let f = RouterValenceFactory::new(router, config);
        let system = serde_json::to_value(Actor::System {
            operation: "x".into(),
        })
        .expect("json");
        let err = f.build(&system).expect_err("system");
        assert!(err.to_string().contains("System"));
    }
}
