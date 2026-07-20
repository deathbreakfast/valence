//! Process-global [`ValenceFactory`] from a pinned [`DatabaseRouter`].

use std::sync::Arc;

use serde_json::Value;

use crate::error::Result;
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
        }
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
        let actor_factory = self
            .config
            .actor_factory
            .clone()
            .unwrap_or_else(|| Arc::new(JsonActorFactory));
        let _actor_ctx = actor_factory.build(actor_json)?;

        let mut builder = ValenceBuilder::new()
            .database_router(Arc::clone(&self.router))
            .default_backend_key(self.config.default_backend_key.clone())
            .actor_factory(actor_factory);

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
