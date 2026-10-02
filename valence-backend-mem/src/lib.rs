//! In-memory [`DatabaseBackend`](valence_core::DatabaseBackend) reference adapter.

#![deny(missing_docs)]

mod backend;
mod query_filter;

pub use backend::{InMemoryBackend, ENGINE_ID};
pub use valence_core::{router_key, DatabaseRouter, KnownEngines, DEFAULT_IN_MEMORY_ROUTER_KEY};

use std::sync::Arc;

/// Register the default in-memory backend under [`DEFAULT_IN_MEMORY_ROUTER_KEY`].
pub fn install_default_mem_router() -> Arc<DatabaseRouter> {
    let router = Arc::new(DatabaseRouter::new());
    let backend = Arc::new(InMemoryBackend::new());
    let key = router_key("default", ENGINE_ID);
    router
        .register_runtime(key, backend)
        .expect("register default mem backend");
    router
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_default_registers_key() {
        let router = install_default_mem_router();
        let backend = router.resolve(DEFAULT_IN_MEMORY_ROUTER_KEY).unwrap();
        assert_eq!(backend.engine_id(), ENGINE_ID);
    }

    #[tokio::test]
    async fn instrumented_wrap_emits_read_counter() {
        use std::sync::Arc;

        use valence_core::instrumentation::wrap_backend;
        use valence_telemetry::{install_telemetry_sink, RecordingSink};

        let sink = RecordingSink::new();
        install_telemetry_sink(Arc::new(sink.clone()));
        let wrapped = wrap_backend(Arc::new(InMemoryBackend::new()));
        let _ = wrapped.get_record("fixture", "id").await.expect("get");
        assert_ne!(
            sink.recorded_counters_matching("valence_db_reads", &[("op", "get")]),
            Vec::<valence_telemetry::RecordedCounter>::new()
        );
    }
}
