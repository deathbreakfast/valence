//! Named backend registry (heterogeneous engines per router).
//!
//! One [`DatabaseRouter`] per [`crate::Valence`]:
//! - [`DatabaseRouter::register`] at build time (or inject via [`crate::ValenceBuilder::database_router`])
//! - [`DatabaseRouter::resolve`] per operation / evaluator hop
//!
//! Cross-storage semantics:
//! - Per-table `database:` evaluator → compound [`crate::router_key()`] → backend
//! - Batch operations stay on a **single** backend
//! - **No** cross-backend transactions

use crate::backend::DatabaseBackend;
use crate::error::{Error, Result};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Maps compound router keys to concrete [`DatabaseBackend`] implementations.
///
/// Keys look like `inmemory_mem:default` or `acme_vault:billing` — see [`crate::router_key()`].
///
/// # Examples
///
/// ```
/// use std::sync::Arc;
/// use valence_backend_mem::{InMemoryBackend, ENGINE_ID};
/// use valence_core::{router_key, DatabaseRouter};
///
/// let mut router = DatabaseRouter::new();
/// let key = router_key("default", ENGINE_ID);
/// router.register(key.clone(), Arc::new(InMemoryBackend::new()));
/// assert_eq!(router.resolve(&key).unwrap().engine_id(), ENGINE_ID);
/// ```
#[derive(Debug, Default)]
pub struct DatabaseRouter {
    backends: RwLock<HashMap<String, Arc<dyn DatabaseBackend>>>,
}

impl DatabaseRouter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, key: impl Into<String>, backend: Arc<dyn DatabaseBackend>) {
        self.backends
            .get_mut()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(key.into(), backend);
    }

    pub fn register_runtime(
        &self,
        key: impl Into<String>,
        backend: Arc<dyn DatabaseBackend>,
    ) -> Result<()> {
        self.backends
            .write()
            .map_err(|_| Error::Internal("router lock poisoned".into()))?
            .insert(key.into(), backend);
        Ok(())
    }

    pub fn resolve(&self, key: &str) -> Result<Arc<dyn DatabaseBackend>> {
        self.backends
            .read()
            .map_err(|_| Error::Internal("router lock poisoned".into()))?
            .get(key)
            .cloned()
            .ok_or_else(|| Error::Internal(format!("unknown database backend: {key}")))
    }

    pub fn len(&self) -> Result<usize> {
        Ok(self
            .backends
            .read()
            .map_err(|_| Error::Internal("router lock poisoned".into()))?
            .len())
    }

    pub fn is_empty(&self) -> Result<bool> {
        Ok(self.len()? == 0)
    }
}
