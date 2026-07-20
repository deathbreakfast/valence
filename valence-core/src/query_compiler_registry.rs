//! Registry mapping [`DatabaseBackend::engine_id`](crate::backend::DatabaseBackend::engine_id) to query compilers.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use crate::error::{Error, Result};
use crate::known_engines::KnownEngines;
use crate::query::QueryCore;
use crate::query_compiler::QueryCompiler;
use crate::CompiledQuery;

/// Resolves a [`QueryCompiler`] for a storage engine slug.
#[derive(Clone, Default)]
pub struct QueryCompilerRegistry {
    compilers: HashMap<&'static str, Arc<dyn QueryCompiler>>,
}

impl std::fmt::Debug for QueryCompilerRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QueryCompilerRegistry")
            .field("engines", &self.compilers.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl QueryCompilerRegistry {
    /// Build a registry from Cargo feature-enabled compilers.
    #[must_use]
    pub fn with_enabled_features() -> Self {
        let mut registry = Self::default();
        registry.register_builtins();
        registry
    }

    fn register_builtins(&mut self) {
        #[cfg(feature = "compiler-sql")]
        {
            let sql: Arc<dyn QueryCompiler> = Arc::new(crate::backend::SqlQueryCompiler);
            self.register(KnownEngines::SQLITE, Arc::clone(&sql));
            self.register(KnownEngines::POSTGRES, Arc::clone(&sql));
            self.register(KnownEngines::INMEMORY_MEM, sql);
        }
        #[cfg(feature = "compiler-surreal")]
        {
            self.register(
                KnownEngines::SURREALDB,
                Arc::new(crate::backend::SurrealQueryCompiler) as Arc<dyn QueryCompiler>,
            );
        }
        #[cfg(feature = "compiler-mongodb")]
        {
            self.register(
                KnownEngines::MONGODB,
                Arc::new(crate::backend::MongoQueryCompiler) as Arc<dyn QueryCompiler>,
            );
        }
        #[cfg(feature = "compiler-redis")]
        {
            self.register(
                KnownEngines::REDIS,
                Arc::new(crate::backend::RedisQueryCompiler) as Arc<dyn QueryCompiler>,
            );
        }
        #[cfg(feature = "compiler-indradb")]
        {
            self.register(
                KnownEngines::INDRADB,
                Arc::new(crate::backend::IndraQueryCompiler) as Arc<dyn QueryCompiler>,
            );
        }
    }

    /// Register a compiler for an open engine slug.
    pub fn register(&mut self, engine_id: &'static str, compiler: Arc<dyn QueryCompiler>) {
        self.compilers.insert(engine_id, compiler);
    }

    /// Look up a compiler by engine id.
    pub fn get(&self, engine_id: &str) -> Option<&Arc<dyn QueryCompiler>> {
        self.compilers.get(engine_id)
    }

    /// Compile `core` for `engine_id`, or return a clear error when the feature is disabled.
    /// # Errors
    ///
    /// Returns an error when the requested operation cannot be completed.
    pub fn compile(&self, engine_id: &str, core: &QueryCore) -> Result<CompiledQuery> {
        let compiler = self.get(engine_id).ok_or_else(|| {
            Error::Internal(format!(
                "no query compiler registered for engine `{engine_id}` — enable the matching \
                 valence-core `compiler-*` / valence facade feature"
            ))
        })?;
        compiler.compile(core)
    }
}

static GLOBAL_REGISTRY: OnceLock<QueryCompilerRegistry> = OnceLock::new();

/// Global registry populated from enabled compiler features.
pub fn global_compiler_registry() -> &'static QueryCompilerRegistry {
    GLOBAL_REGISTRY.get_or_init(QueryCompilerRegistry::with_enabled_features)
}

/// Compile `core` for the given backend engine id.
/// # Errors
///
/// Returns an error when the requested operation cannot be completed.
pub fn compile_for_engine(engine_id: &str, core: &QueryCore) -> Result<CompiledQuery> {
    global_compiler_registry().compile(engine_id, core)
}
