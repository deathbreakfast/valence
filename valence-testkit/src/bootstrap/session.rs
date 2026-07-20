//! [`BootstrapSession`] install and Valence build helpers.

use std::sync::Arc;

use valence_core::instrumentation::wrap_backend;
use valence_core::ports::endpoints::{DatabaseEndpointResolver, EnvEndpointResolver};
use valence_core::router::DatabaseRouter;
use valence_core::router_key::router_key;
use valence_core::{
    DatabaseBackend, Result, RouterValenceFactory, RouterValenceFactoryConfig, Valence,
    ValenceBuilder, ValenceFactory,
};
use valence_telemetry::{
    install_telemetry_sink, ConsoleSink, NoOpSink, RecordingSink, TelemetrySink,
};

use crate::matrix::{MatrixSpec, StorageAdapter, TelemetryAdapter, Topology};

use super::env_guard::EnvGuard;
use super::wire::WireBackendOptions;

/// How embedded Surreal logical names are registered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BootstrapMode {
    /// Explicit logical name list (default `["default"]`).
    #[default]
    ExplicitLogicalNames,
    /// Discover logical names from linked schema inventory (Surreal only).
    #[cfg(feature = "surreal-inventory")]
    FromInventory,
}

/// Bootstraps a Valence stack for one matrix row.
pub struct BootstrapSession {
    matrix: MatrixSpec,
    bootstrap_mode: BootstrapMode,
    logical_names: Vec<String>,
    router: Option<Arc<DatabaseRouter>>,
    default_backend_key: Option<String>,
    pub(crate) valence: Option<Valence>,
    factory: Option<Arc<dyn ValenceFactory>>,
    recording: Arc<RecordingSink>,
    telemetry_sink: Arc<dyn TelemetrySink>,
    env_guard: Option<EnvGuard>,
    wire_options: Option<WireBackendOptions>,
    ready: bool,
    temp_dir: Option<tempfile::TempDir>,
}

impl BootstrapSession {
    /// Create a session for one matrix row (call [`BootstrapSession::spawn`] before use).
    pub fn new(matrix: MatrixSpec) -> Self {
        Self {
            matrix,
            bootstrap_mode: BootstrapMode::default(),
            logical_names: vec!["default".to_string()],
            router: None,
            default_backend_key: None,
            valence: None,
            factory: None,
            recording: Arc::new(RecordingSink::new()),
            telemetry_sink: Arc::new(NoOpSink),
            env_guard: None,
            wire_options: None,
            ready: false,
            temp_dir: None,
        }
    }

    /// Override embedded logical names (Surreal multi-logical scenarios).
    pub fn with_logical_names(mut self, names: &[&str]) -> Self {
        self.logical_names = names.iter().map(|s| (*s).to_string()).collect();
        self
    }

    /// Set explicit wire-backend builder options (Redis, MongoDB, Postgres, fleet).
    pub fn with_wire_options(mut self, opts: WireBackendOptions) -> Self {
        self.wire_options = Some(opts);
        self
    }

    /// Use schema inventory discovery for Surreal bootstrap.
    #[cfg(feature = "surreal-inventory")]
    pub fn with_inventory_bootstrap(mut self) -> Self {
        self.bootstrap_mode = BootstrapMode::FromInventory;
        self
    }

    /// Wire options for remote adapters when set.
    pub fn wire_options(&self) -> Option<&WireBackendOptions> {
        self.wire_options.as_ref()
    }

    /// Matrix row this session was constructed for.
    pub fn matrix(&self) -> &MatrixSpec {
        &self.matrix
    }

    /// Whether [`BootstrapSession::spawn`] completed successfully.
    pub fn is_ready(&self) -> bool {
        self.ready
    }

    /// Shared recording sink for telemetry assertions.
    pub fn recording(&self) -> Arc<RecordingSink> {
        Arc::clone(&self.recording)
    }

    /// Pinned router after spawn.
    pub fn router(&self) -> Option<&Arc<DatabaseRouter>> {
        self.router.as_ref()
    }

    /// Default backend compound key for this session.
    pub fn default_backend_key(&self) -> Option<&str> {
        self.default_backend_key.as_deref()
    }

    /// Built [`Valence`] handle when a scenario requested it.
    pub fn valence(&self) -> Option<&Valence> {
        self.valence.as_ref()
    }

    /// Factory backed by the shared router.
    pub fn factory(&self) -> Option<&Arc<dyn ValenceFactory>> {
        self.factory.as_ref()
    }

    /// Install storage, telemetry, and router for the matrix row.
    pub async fn spawn(&mut self) -> Result<()> {
        if matches!(self.matrix.topology, Topology::RemoteStub) {
            return Err(valence_core::Error::Internal(
                "remote topology stub — host wiring required".into(),
            ));
        }

        self.telemetry_sink = telemetry_for_matrix(self.matrix, &self.recording);
        if matches!(self.matrix.telemetry, TelemetryAdapter::Recording) {
            install_telemetry_sink(Arc::clone(&self.telemetry_sink));
        }
        let (router, default_key) = self.build_router().await?;
        self.default_backend_key = Some(default_key.clone());
        self.router = Some(Arc::clone(&router));

        let mut config = RouterValenceFactoryConfig::new(default_key);
        config.telemetry_sink = Some(Arc::clone(&self.telemetry_sink));
        self.factory = Some(RouterValenceFactory::arc(router, config));
        self.ready = true;
        Ok(())
    }

    /// Build or return cached [`Valence`].
    pub fn ensure_valence(&mut self) -> Result<&Valence> {
        if self.valence.is_none() {
            self.build_valence(None)?;
        }
        Ok(self.valence.as_ref().expect("valence built"))
    }

    /// Build [`Valence`] with optional endpoint resolver override.
    pub fn build_valence(
        &mut self,
        endpoint_resolver: Option<
            Arc<dyn valence_core::ports::endpoints::DatabaseEndpointResolver>,
        >,
    ) -> Result<&Valence> {
        let router = self
            .router
            .clone()
            .ok_or_else(|| valence_core::Error::Internal("spawn before build_valence".into()))?;
        let default_key = self
            .default_backend_key
            .clone()
            .ok_or_else(|| valence_core::Error::Internal("missing default backend key".into()))?;

        let mut builder = ValenceBuilder::new()
            .database_router(router)
            .default_backend_key(default_key)
            .telemetry_sink(Arc::clone(&self.telemetry_sink));

        if let Some(resolver) = endpoint_resolver {
            builder = builder.endpoint_resolver(resolver);
        }

        self.valence = Some(builder.build()?);
        Ok(self.valence.as_ref().expect("valence"))
    }

    async fn build_router(&mut self) -> Result<(Arc<DatabaseRouter>, String)> {
        match self.matrix.storage {
            StorageAdapter::Mem => self.build_mem_router(),
            StorageAdapter::Sqlite => self.build_sqlite_router().await,
            StorageAdapter::Postgres => self.build_postgres_router().await,
            StorageAdapter::MongoDb => self.build_mongodb_router().await,
            StorageAdapter::IndraDb => self.build_indradb_router(),
            StorageAdapter::Redis => self.build_redis_router().await,
            StorageAdapter::SurrealMem => self.build_surreal_router(false).await,
            StorageAdapter::SurrealRocksdb => self.build_surreal_router(true).await,
            StorageAdapter::AcmeStub => self.build_acme_router(),
        }
    }

    fn build_mem_router(&self) -> Result<(Arc<DatabaseRouter>, String)> {
        use valence_backend_mem::{InMemoryBackend, ENGINE_ID};

        let backend: Arc<dyn DatabaseBackend> = Arc::new(InMemoryBackend::new());
        let backend = maybe_wrap_backend(backend, self.matrix.telemetry);
        let mut router = DatabaseRouter::new();
        let default_logical = self.logical_names.first().map_or("default", |s| s.as_str());
        for name in &self.logical_names {
            let key = router_key(name, ENGINE_ID);
            router.register(key, Arc::clone(&backend));
        }
        let default_key = router_key(default_logical, ENGINE_ID);
        Ok((Arc::new(router), default_key))
    }

    async fn build_sqlite_router(&self) -> Result<(Arc<DatabaseRouter>, String)> {
        #[cfg(not(feature = "sqlite"))]
        {
            return Err(valence_core::Error::Internal(
                "enable valence-testkit/sqlite".into(),
            ));
        }

        #[cfg(feature = "sqlite")]
        {
            use valence_backend_sqlite::{SqliteBackend, ENGINE_ID};

            let backend: Arc<dyn DatabaseBackend> = Arc::new(
                SqliteBackend::connect_memory()
                    .await
                    .map_err(|e| valence_core::Error::Internal(e.to_string()))?,
            );
            let backend = maybe_wrap_backend(backend, self.matrix.telemetry);
            let mut router = DatabaseRouter::new();
            let default_logical = self.logical_names.first().map_or("default", |s| s.as_str());
            for name in &self.logical_names {
                let key = router_key(name, ENGINE_ID);
                router.register(key, Arc::clone(&backend));
            }
            let default_key = router_key(default_logical, ENGINE_ID);
            Ok((Arc::new(router), default_key))
        }
    }

    async fn build_postgres_router(&self) -> Result<(Arc<DatabaseRouter>, String)> {
        #[cfg(not(feature = "postgres"))]
        {
            return Err(valence_core::Error::Internal(
                "enable valence-testkit/postgres".into(),
            ));
        }

        #[cfg(feature = "postgres")]
        {
            use valence_backend_postgres::{PostgresBackendBuilder, ENGINE_ID};

            let builder = self
                .wire_options
                .as_ref()
                .and_then(|o| o.postgres.clone())
                .unwrap_or_else(PostgresBackendBuilder::new);
            let backend: Arc<dyn DatabaseBackend> = Arc::new(
                builder
                    .from_env_defaults()
                    .build()
                    .await
                    .map_err(|e| valence_core::Error::Internal(e.to_string()))?,
            );
            let backend = maybe_wrap_backend(backend, self.matrix.telemetry);
            let mut router = DatabaseRouter::new();
            let default_logical = self.logical_names.first().map_or("default", |s| s.as_str());
            for name in &self.logical_names {
                let key = router_key(name, ENGINE_ID);
                router.register(key, Arc::clone(&backend));
            }
            let default_key = router_key(default_logical, ENGINE_ID);
            Ok((Arc::new(router), default_key))
        }
    }

    async fn build_mongodb_router(&self) -> Result<(Arc<DatabaseRouter>, String)> {
        #[cfg(not(feature = "mongodb"))]
        {
            return Err(valence_core::Error::Internal(
                "enable valence-testkit/mongodb".into(),
            ));
        }

        #[cfg(feature = "mongodb")]
        {
            use valence_backend_mongodb::{MongoBackendBuilder, ENGINE_ID};

            let builder = self
                .wire_options
                .as_ref()
                .and_then(|o| o.mongodb.clone())
                .unwrap_or_else(MongoBackendBuilder::new);
            let backend: Arc<dyn DatabaseBackend> = Arc::new(
                builder
                    .from_env_defaults()
                    .build()
                    .await
                    .map_err(|e| valence_core::Error::Internal(e.to_string()))?,
            );
            let backend = maybe_wrap_backend(backend, self.matrix.telemetry);
            let mut router = DatabaseRouter::new();
            let default_logical = self.logical_names.first().map_or("default", |s| s.as_str());
            for name in &self.logical_names {
                let key = router_key(name, ENGINE_ID);
                router.register(key, Arc::clone(&backend));
            }
            let default_key = router_key(default_logical, ENGINE_ID);
            Ok((Arc::new(router), default_key))
        }
    }

    fn build_indradb_router(&self) -> Result<(Arc<DatabaseRouter>, String)> {
        #[cfg(not(feature = "indradb"))]
        {
            return Err(valence_core::Error::Internal(
                "enable valence-testkit/indradb".into(),
            ));
        }

        #[cfg(feature = "indradb")]
        {
            use valence_backend_indradb::{IndradbBackend, ENGINE_ID};

            let backend: Arc<dyn DatabaseBackend> = Arc::new(IndradbBackend::new());
            let backend = maybe_wrap_backend(backend, self.matrix.telemetry);
            let mut router = DatabaseRouter::new();
            let default_logical = self.logical_names.first().map_or("default", |s| s.as_str());
            for name in &self.logical_names {
                let key = router_key(name, ENGINE_ID);
                router.register(key, Arc::clone(&backend));
            }
            let default_key = router_key(default_logical, ENGINE_ID);
            Ok((Arc::new(router), default_key))
        }
    }

    async fn build_redis_router(&self) -> Result<(Arc<DatabaseRouter>, String)> {
        #[cfg(not(feature = "redis"))]
        {
            return Err(valence_core::Error::Internal(
                "enable valence-testkit/redis".into(),
            ));
        }

        #[cfg(feature = "redis")]
        {
            use valence_backend_redis::{RedisBackendBuilder, ENGINE_ID};

            let backend: Arc<dyn DatabaseBackend> = if let Some(fleet_builder) = self
                .wire_options
                .as_ref()
                .and_then(|o| o.redis_fleet.clone())
            {
                Arc::new(
                    fleet_builder
                        .from_env_defaults()
                        .build()
                        .await
                        .map_err(|e| valence_core::Error::Internal(e.to_string()))?,
                )
            } else {
                let builder = self
                    .wire_options
                    .as_ref()
                    .and_then(|o| o.redis.clone())
                    .unwrap_or_else(RedisBackendBuilder::new);
                Arc::new(
                    builder
                        .from_env_defaults()
                        .build()
                        .await
                        .map_err(|e| valence_core::Error::Internal(e.to_string()))?,
                )
            };
            let backend = maybe_wrap_backend(backend, self.matrix.telemetry);
            let mut router = DatabaseRouter::new();
            let default_logical = self.logical_names.first().map_or("default", |s| s.as_str());
            for name in &self.logical_names {
                let key = router_key(name, ENGINE_ID);
                router.register(key, Arc::clone(&backend));
            }
            let default_key = router_key(default_logical, ENGINE_ID);
            Ok((Arc::new(router), default_key))
        }
    }

    async fn build_surreal_router(
        &mut self,
        rocksdb: bool,
    ) -> Result<(Arc<DatabaseRouter>, String)> {
        #[cfg(not(feature = "surreal-mem"))]
        {
            let _ = rocksdb;
            return Err(valence_core::Error::Internal(
                "enable valence-testkit/surreal-mem".into(),
            ));
        }

        #[cfg(feature = "surreal-mem")]
        {
            use valence_backend_surreal::{
                bootstrap_embedded_router, connect_embedded_at_path, EmbeddedEngine,
                RegisterEmbeddedLogicalNamesOptions, ENGINE_ID,
            };

            let engine = if rocksdb {
                #[cfg(not(feature = "surreal-rocksdb"))]
                {
                    return Err(valence_core::Error::Internal(
                        "enable valence-testkit/surreal-rocksdb".into(),
                    ));
                }
                #[cfg(feature = "surreal-rocksdb")]
                {
                    EmbeddedEngine::RocksDb
                }
            } else {
                EmbeddedEngine::Mem
            };

            let path = if rocksdb {
                let temp = tempfile::tempdir()
                    .map_err(|e| valence_core::Error::Internal(format!("tempdir: {e}")))?;
                let path = temp.path().join("surreal").to_string_lossy().into_owned();
                self.temp_dir = Some(temp);
                path
            } else {
                "mem".to_string()
            };

            let db = connect_embedded_at_path(engine, &path, "testkit", "testkit").await?;
            let names: Vec<&str> = self.logical_names.iter().map(|s| s.as_str()).collect();

            let router = match self.bootstrap_mode {
                BootstrapMode::ExplicitLogicalNames => bootstrap_embedded_router(
                    db,
                    &names,
                    RegisterEmbeddedLogicalNamesOptions::default(),
                )?,
                #[cfg(feature = "surreal-inventory")]
                BootstrapMode::FromInventory => {
                    use valence_backend_surreal::bootstrap_embedded_router_from_inventory;
                    bootstrap_embedded_router_from_inventory(
                        db,
                        RegisterEmbeddedLogicalNamesOptions::default(),
                    )?
                }
            };

            if matches!(self.matrix.telemetry, TelemetryAdapter::Recording) {
                // Surreal bootstrap wraps when instrumentation feature is enabled on the adapter.
                let _ = self.matrix.telemetry;
            }

            let default_key = router_key(
                self.logical_names.first().map_or("default", |s| s.as_str()),
                ENGINE_ID,
            );
            Ok((router, default_key))
        }
    }

    fn build_acme_router(&self) -> Result<(Arc<DatabaseRouter>, String)> {
        #[cfg(not(feature = "acme-stub"))]
        {
            Err(valence_core::Error::Internal(
                "enable valence-testkit/acme-stub".into(),
            ))
        }

        #[cfg(feature = "acme-stub")]
        {
            use acme_valence_backend_stub::{AcmeStubBackend, ENGINE_ID};

            let backend: Arc<dyn DatabaseBackend> = Arc::new(AcmeStubBackend::new());
            let backend = maybe_wrap_backend(backend, self.matrix.telemetry);
            let mut router = DatabaseRouter::new();
            let default_logical = self.logical_names.first().map_or("primary", |s| s.as_str());
            for name in &self.logical_names {
                let key = router_key(name, ENGINE_ID);
                router.register(key, Arc::clone(&backend));
            }
            let default_key = router_key(default_logical, ENGINE_ID);
            Ok((Arc::new(router), default_key))
        }
    }

    /// Set env var for endpoint scenarios (restored on session drop).
    pub fn set_env(&mut self, key: &'static str, value: &str) {
        self.env_guard = Some(EnvGuard::set(key, value));
    }

    /// Resolve endpoint via [`EnvEndpointResolver`] (reads current process env).
    pub fn resolve_env_endpoint(&self, logical: &str) -> Result<Option<String>> {
        EnvEndpointResolver.resolve_url(logical)
    }
}

fn telemetry_for_matrix(matrix: MatrixSpec, recording: &RecordingSink) -> Arc<dyn TelemetrySink> {
    match matrix.telemetry {
        TelemetryAdapter::Off => Arc::new(NoOpSink),
        TelemetryAdapter::Console => Arc::new(ConsoleSink),
        TelemetryAdapter::Recording => Arc::new(recording.clone()),
    }
}

fn maybe_wrap_backend(
    backend: Arc<dyn DatabaseBackend>,
    telemetry: TelemetryAdapter,
) -> Arc<dyn DatabaseBackend> {
    if matches!(telemetry, TelemetryAdapter::Recording) {
        wrap_backend(backend)
    } else {
        backend
    }
}
