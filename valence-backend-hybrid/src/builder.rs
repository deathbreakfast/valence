//! [`HybridBackendBuilder`] — capacities, cache rules, and async warm-up.

use std::sync::Arc;

use valence_backend_indradb::IndradbBackend;
use valence_core::error::{Error, Result};
use valence_core::DatabaseBackend;

use crate::cache_policy::{
    CachePolicy, CacheRules, DEFAULT_EDGE_CAPACITY, DEFAULT_RECORD_CAPACITY,
};
use crate::edge_cache::EdgeCache;
use crate::record_cache::RecordCache;
use crate::HybridBackend;

/// Fluent builder for [`HybridBackend`].
///
/// # Examples
///
/// ```no_run
/// use std::sync::Arc;
/// use valence_backend_hybrid::{CacheRules, HybridBackend};
/// use valence_backend_mem::InMemoryBackend;
/// use valence_core::DatabaseBackend;
///
/// # async fn demo() -> valence_core::Result<()> {
/// let primary: Arc<dyn DatabaseBackend> = Arc::new(InMemoryBackend::new());
/// let hybrid = HybridBackend::builder()
///     .primary(primary)
///     .record_capacity(10_000)
///     .edge_capacity(100_000)
///     .record_rules(CacheRules::cache_all().exclude(["audit_log"]))
///     .edge_rules(CacheRules::cache_none().include(["project_members"]))
///     .warm_edges(true)
///     .build()
///     .await?;
/// assert_eq!(hybrid.engine_id(), valence_backend_hybrid::ENGINE_ID);
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct HybridBackendBuilder {
    primary: Option<Arc<dyn DatabaseBackend>>,
    record_capacity: usize,
    edge_capacity: usize,
    record_rules: CacheRules,
    edge_rules: CacheRules,
    warm_edges: bool,
}

impl Default for HybridBackendBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl HybridBackendBuilder {
    /// Empty builder with default capacities (10k records / 100k edges) and cache-all rules.
    #[must_use]
    pub fn new() -> Self {
        Self {
            primary: None,
            record_capacity: DEFAULT_RECORD_CAPACITY,
            edge_capacity: DEFAULT_EDGE_CAPACITY,
            record_rules: CacheRules::cache_all(),
            edge_rules: CacheRules::cache_all(),
            warm_edges: true,
        }
    }

    /// Set the durable SQL primary (Postgres or SQLite).
    #[must_use]
    pub fn primary(mut self, primary: Arc<dyn DatabaseBackend>) -> Self {
        self.primary = Some(primary);
        self
    }

    /// Maximum cached record bodies (`0` disables record caching).
    #[must_use]
    pub fn record_capacity(mut self, capacity: usize) -> Self {
        self.record_capacity = capacity;
        self
    }

    /// Maximum cached graph edges (`0` disables edge caching).
    #[must_use]
    pub fn edge_capacity(mut self, capacity: usize) -> Self {
        self.edge_capacity = capacity;
        self
    }

    /// Include/exclude rules for record tables.
    #[must_use]
    pub fn record_rules(mut self, rules: CacheRules) -> Self {
        self.record_rules = rules;
        self
    }

    /// Include/exclude rules for edge tables.
    #[must_use]
    pub fn edge_rules(mut self, rules: CacheRules) -> Self {
        self.edge_rules = rules;
        self
    }

    /// Whether to bulk-load `valence_edges` from the primary at build time.
    #[must_use]
    pub fn warm_edges(mut self, warm: bool) -> Self {
        self.warm_edges = warm;
        self
    }

    /// Validate policy, construct the backend, and optionally warm edges.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Internal`] when the primary is missing or cache rules conflict.
    /// Propagates edge warm-up failures from the primary/mirror.
    pub async fn build(self) -> Result<HybridBackend> {
        let primary = self
            .primary
            .ok_or_else(|| Error::Internal("HybridBackend requires a primary backend".into()))?;
        let policy = CachePolicy::new(
            self.record_capacity,
            self.edge_capacity,
            self.record_rules,
            self.edge_rules,
        )?;
        let mirror = IndradbBackend::new();
        let records = RecordCache::new();
        let edges = EdgeCache::new();
        if self.warm_edges {
            edges.warm_from_primary(&primary, &mirror, &policy).await?;
        }
        Ok(HybridBackend {
            primary,
            mirror,
            records,
            edges,
            policy,
        })
    }
}
