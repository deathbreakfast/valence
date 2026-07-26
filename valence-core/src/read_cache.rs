//! Valence-wide read-through LRU for `Model::get`-shaped point reads.
//!
//! **Enable:** default on; set `VALENCE_READ_CACHE=0` to disable. **Capacity:** `VALENCE_READ_CACHE_MAX` (default 10_000).
//!
//! Stores raw rows pre-privacy; callers still run `check_read_privacy` per request actor.
//! When [`ownership_unified_fetch_enabled`] is on, cache entries bundle the main row and ownership
//! gate status so warm reads avoid a second ownership trip.
//! Write paths invalidate via generated `Model` ops and [`OwnershipService`].
//!
//! Cache keys include the backend allocation identity so distinct in-memory stores (parallel
//! tests, multiple routers) do not share point-get rows.

use std::sync::{Arc, OnceLock};

use quick_cache::sync::Cache;
use serde_json::Value;

use crate::backend::DatabaseBackend;
use crate::error::Result;
use crate::instrumentation;
use crate::ownership::{
    ownership_unified_fetch_enabled, OwnershipGateStatus, OwnershipService, RecordOwnershipBundle,
};

type CacheKey = (usize, String, String);

fn cache_key(backend: &Arc<dyn DatabaseBackend>, table: &str, id: &str) -> CacheKey {
    // Fat pointer → data pointer address; isolates distinct Arc backends (e.g. test DBs).
    let backend_id = Arc::as_ptr(backend).cast::<()>() as usize;
    (backend_id, table.to_string(), id.to_string())
}

/// Whether the read-through record cache is active (default on; `VALENCE_READ_CACHE=0` disables).
pub fn read_cache_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        !matches!(
            std::env::var("VALENCE_READ_CACHE").as_deref(),
            Ok("0") | Ok("false") | Ok("FALSE")
        )
    })
}

fn read_cache_capacity() -> usize {
    static CAPACITY: OnceLock<usize> = OnceLock::new();
    *CAPACITY.get_or_init(|| {
        std::env::var("VALENCE_READ_CACHE_MAX")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(10_000)
            .max(1)
    })
}

fn global_cache() -> &'static Cache<CacheKey, RecordOwnershipBundle> {
    static CACHE: OnceLock<Cache<CacheKey, RecordOwnershipBundle>> = OnceLock::new();
    CACHE.get_or_init(|| Cache::new(read_cache_capacity()))
}

/// Point-get with optional read-through cache (raw row, pre-privacy).
/// # Errors
///
/// Returns an error when the requested operation cannot be completed.
pub async fn get_record_via_cache(
    backend: &Arc<dyn DatabaseBackend>,
    table: &str,
    id: &str,
) -> Result<Option<Value>> {
    // Hybrid owns its IndraDB record cache; skip the process-wide LRU.
    if backend.engine_id() == crate::KnownEngines::HYBRID_INDRA_SQL {
        return backend.get_record(table, id).await;
    }
    if read_cache_enabled() {
        let key = cache_key(backend, table, id);
        if let Some(cached) = global_cache().get(&key) {
            instrumentation::record_ownership_fetch_mode("cache_hit_row");
            return Ok(cached.row);
        }
        let row = backend.get_record(table, id).await?;
        if row.is_some() || ownership_unified_fetch_enabled() {
            global_cache().insert(
                key,
                RecordOwnershipBundle {
                    row: row.clone(),
                    ownership_status: OwnershipGateStatus::NotFetched,
                },
            );
        }
        return Ok(row);
    }
    backend.get_record(table, id).await
}

/// Unified `Model::get` fetch: one backend round trip for row + ownership gate status (when cache cold).
/// # Errors
///
/// Returns an error when the requested operation cannot be completed.
pub async fn get_record_with_ownership_bundle_via_cache(
    backend: &Arc<dyn DatabaseBackend>,
    table: &str,
    id: &str,
    valence_model: &str,
    v: &crate::runtime::Valence,
) -> Result<RecordOwnershipBundle> {
    // Hybrid tables: still bundle ownership at the runtime layer, but do not store
    // the row in the process-wide LRU (IndraDB is the row cache).
    let hybrid = backend.engine_id() == crate::KnownEngines::HYBRID_INDRA_SQL;

    if read_cache_enabled() && !hybrid {
        let key = cache_key(backend, table, id);
        if let Some(cached) = global_cache().get(&key) {
            if cached.ownership_status != OwnershipGateStatus::NotFetched {
                instrumentation::record_ownership_fetch_mode("cache_hit");
                return Ok(cached);
            }
        }
    }

    let bundle = OwnershipService::fetch_record_with_ownership_gate_uncached(
        backend,
        table,
        id,
        valence_model,
        v,
    )
    .await?;
    instrumentation::record_ownership_fetch_mode(if hybrid { "unified_hybrid" } else { "unified" });

    if read_cache_enabled() && !hybrid {
        global_cache().insert(cache_key(backend, table, id), bundle.clone());
    }
    Ok(bundle)
}

/// Drop cached rows after a write or ownership status change.
///
/// Without a backend handle (codegen call sites), this clears the process-wide cache.
/// Prefer [`invalidate_for_backend`] when the backend is available.
pub fn invalidate(table: &str, id: &str) {
    let _ = (table, id);
    if read_cache_enabled() {
        // Coarse: write paths lack a backend pointer in generated code. Clearing avoids
        // serving stale rows after upsert/delete across any backend.
        global_cache().clear();
    }
}

/// Drop a cached row for a specific backend after a write.
pub fn invalidate_for_backend(backend: &Arc<dyn DatabaseBackend>, table: &str, id: &str) {
    if read_cache_enabled() {
        global_cache().remove(&cache_key(backend, table, id));
    }
}

/// Clear the global cache (tests / integration tests).
#[doc(hidden)]
pub fn clear_for_test() {
    global_cache().clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_cache_enabled_by_default() {
        assert!(read_cache_enabled());
    }

    #[test]
    fn read_cache_capacity_defaults_to_10k() {
        assert_eq!(read_cache_capacity(), 10_000);
    }
}
