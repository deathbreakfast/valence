//! [`Valence`] builder — host wiring for storage, injectable ports, and actor context.

use super::Valence;
use crate::actor::Actor;
use crate::backend::DatabaseBackend;
use crate::error::{Error, Result};
use crate::ports::actor::{ActorFactory, JsonActorFactory};
use crate::ports::endpoints::{DatabaseEndpointResolver, NoopEndpointResolver};
use crate::ports::secrets::{NoOpSecretProvider, SecretProvider};
use crate::request_cache::RequestPermissionCache;
use crate::router::DatabaseRouter;
use crate::router_key::router_key;
use std::sync::Arc;
use valence_telemetry::{install_telemetry_sink, NoOpSink, TelemetrySink};

/// Builder for constructing a [`Valence`] runtime.
///
/// ## Storage
///
/// | Method | Use |
/// |--------|-----|
/// | [`Self::add_backend`] | Primary API; key from `backend.engine_id()` |
/// | [`Self::add_backend_key`] | Explicit compound key |
/// | [`Self::database_router`] | Inject a pre-built [`DatabaseRouter`] |
/// | [`Self::default_backend_key`] | Active backend when models omit per-table routing |
/// | [`Self::build`] | Requires ≥1 backend — **no** silent mem fallback |
///
/// ## Host ports
///
/// Optional: [`Self::telemetry_sink`], [`Self::secret_provider`], [`Self::actor_factory`],
/// [`Self::endpoint_resolver`]. See [`crate::ports`] for the port table and reference impls.
///
/// Privacy policies are schema-attached ([`crate::PolicyEvaluator`]), not registered here.
///
/// # Examples
///
/// ```
/// use std::sync::Arc;
/// use valence_backend_mem::InMemoryBackend;
/// use valence_core::Valence;
///
/// let valence = Valence::builder()
///     .add_backend("default", Arc::new(InMemoryBackend::new()))
///     .build()
///     .expect("build");
/// assert!(valence.active_backend().is_ok());
/// ```
#[derive(Default)]
pub struct ValenceBuilder {
    router: DatabaseRouter,
    injected_router: Option<Arc<DatabaseRouter>>,
    registered_keys: Vec<String>,
    default_backend_key: Option<String>,
    telemetry_sink: Option<Arc<dyn TelemetrySink>>,
    secret_provider: Option<Arc<dyn SecretProvider>>,
    actor_factory: Option<Arc<dyn ActorFactory>>,
    endpoint_resolver: Option<Arc<dyn DatabaseEndpointResolver>>,
    actor: Option<Actor>,
    permission_cache: Option<RequestPermissionCache>,
}

impl ValenceBuilder {
    /// Start with an empty router; call [`Self::add_backend`] before [`Self::build`].
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a backend under `router_key(logical_name, backend.engine_id())`.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::sync::Arc;
    /// use valence_backend_mem::InMemoryBackend;
    /// use valence_core::ValenceBuilder;
    ///
    /// let builder = ValenceBuilder::new()
    ///     .add_backend("default", Arc::new(InMemoryBackend::new()));
    /// let _ = builder.build().expect("build");
    /// ```
    #[must_use]
    pub fn add_backend(
        mut self,
        logical_name: impl AsRef<str>,
        backend: Arc<dyn DatabaseBackend>,
    ) -> Self {
        let key = router_key(logical_name.as_ref(), backend.engine_id());
        self.registered_keys.push(key.clone());
        self.router.register(key, backend);
        self
    }

    /// Register a backend under an explicit router key (for pre-built heterogeneous routers).
    #[must_use]
    pub fn add_backend_key(
        mut self,
        key: impl Into<String>,
        backend: Arc<dyn DatabaseBackend>,
    ) -> Self {
        let key = key.into();
        self.registered_keys.push(key.clone());
        self.router.register(key, backend);
        self
    }

    /// Inject a fully built router instead of registering backends on this builder.
    ///
    /// **Contract:** mutually exclusive with [`Self::add_backend`] / [`Self::add_backend_key`].
    #[must_use]
    pub fn database_router(mut self, router: Arc<DatabaseRouter>) -> Self {
        self.injected_router = Some(router);
        self
    }

    /// Set the active backend key used by generated models without per-table routing.
    ///
    /// **Contract:** required when more than one backend is registered; optional when exactly one.
    #[must_use]
    pub fn default_backend_key(mut self, key: impl Into<String>) -> Self {
        self.default_backend_key = Some(key.into());
        self
    }

    /// Install a process-global telemetry sink (also stored on the built [`Valence`]).
    #[must_use]
    pub fn telemetry_sink(mut self, sink: Arc<dyn TelemetrySink>) -> Self {
        self.telemetry_sink = Some(sink);
        self
    }

    /// Provide secret lookup for host-owned credential resolution.
    #[must_use]
    pub fn secret_provider(mut self, provider: Arc<dyn SecretProvider>) -> Self {
        self.secret_provider = Some(provider);
        self
    }

    /// Build request-scoped [`crate::ActorContext`] values from JSON actor payloads.
    #[must_use]
    pub fn actor_factory(mut self, factory: Arc<dyn ActorFactory>) -> Self {
        self.actor_factory = Some(factory);
        self
    }

    /// Resolve physical database URLs from logical names at bootstrap.
    #[must_use]
    pub fn endpoint_resolver(mut self, resolver: Arc<dyn DatabaseEndpointResolver>) -> Self {
        self.endpoint_resolver = Some(resolver);
        self
    }

    /// Set the default actor for this runtime (defaults to anonymous).
    #[must_use]
    pub fn with_actor(mut self, actor: Actor) -> Self {
        self.actor = Some(actor);
        self
    }

    /// Enable a request-scoped permission check cache on the built [`Valence`].
    #[must_use]
    pub fn enable_permission_cache(mut self) -> Self {
        self.permission_cache = Some(RequestPermissionCache::new());
        self
    }

    /// Attach an existing permission check cache.
    #[must_use]
    pub fn with_permission_cache(mut self, cache: RequestPermissionCache) -> Self {
        self.permission_cache = Some(cache);
        self
    }

    /// Construct a [`Valence`] runtime from the configured router and ports.
    ///
    /// **Errors:** when no backends are registered, when both injected and local routers are used,
    /// or when multiple backends are registered without [`Self::default_backend_key`].
    /// # Errors
    ///
    /// Returns an error when the requested operation cannot be completed.
    pub fn build(self) -> Result<Valence> {
        let router = if let Some(injected) = self.injected_router {
            if self.registered_keys.is_empty() {
                injected
            } else {
                return Err(Error::Internal(
                    "ValenceBuilder: use database_router() or add_backend(), not both".into(),
                ));
            }
        } else {
            if self.router.len()? == 0 {
                return Err(Error::Internal(
                    "ValenceBuilder requires at least one backend — call add_backend() or add_backend_key()".into(),
                ));
            }
            Arc::new(self.router)
        };

        let active_backend_key = match self.default_backend_key {
            Some(key) => key,
            None if self.registered_keys.len() == 1 => self.registered_keys[0].clone(),
            None => {
                return Err(Error::Internal(
                    "ValenceBuilder requires default_backend_key() when multiple backends are registered".into(),
                ));
            }
        };

        let telemetry_sink = match self.telemetry_sink {
            Some(sink) => {
                install_telemetry_sink(Arc::clone(&sink));
                sink
            }
            None => Arc::new(NoOpSink),
        };

        let actor = self.actor.unwrap_or(Actor::Anonymous);

        Ok(Valence {
            router,
            active_backend_key,
            telemetry_sink,
            secret_provider: self
                .secret_provider
                .unwrap_or_else(|| Arc::new(NoOpSecretProvider)),
            actor_factory: self
                .actor_factory
                .unwrap_or_else(|| Arc::new(JsonActorFactory)),
            endpoint_resolver: self
                .endpoint_resolver
                .unwrap_or_else(|| Arc::new(NoopEndpointResolver)),
            actor,
            owner_override: None,
            permission_cache: self.permission_cache,
        })
    }
}
