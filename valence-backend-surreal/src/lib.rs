//! SurrealDB-backed [`DatabaseBackend`](valence_core::DatabaseBackend) reference adapter.
//!
//! All `surrealdb` dependencies stay in this crate only — never in `valence-core`.

#![deny(missing_docs)]

mod bootstrap;
mod connect;
mod embedded;
mod error;
mod query_exec;
mod record_id;
mod row_json;

#[cfg(feature = "inventory")]
mod inventory;

#[cfg(feature = "remote")]
mod remote;

pub use bootstrap::{
    bootstrap_embedded_router, register_embedded_logical_handle, register_embedded_logical_handles,
    register_embedded_logical_names, register_embedded_logical_names_slices,
    shared_router_with_embedded_logical_names, RegisterEmbeddedLogicalNamesOptions,
};
#[cfg(feature = "inventory")]
pub use bootstrap::{
    bootstrap_embedded_router_from_inventory, register_embedded_logical_names_from_inventory,
};
pub use connect::{connect_embedded_at_path, remove_stale_lock, EmbeddedEngine};
#[cfg(feature = "connect-env")]
pub use connect::{
    connect_embedded_from_env, database_from_env, embedded_engine_from_env, embedded_path_from_env,
    namespace_from_env,
};
pub use embedded::{SDb, SurrealEmbeddedBackend, SurrealMemBackend, ENGINE_ID};
#[cfg(feature = "inventory")]
pub use inventory::{
    collect_distinct_embedded_surreal_logical_names, DEFAULT_EMBEDDED_SURREAL_LOGICAL_NAMES,
};
pub use record_id::{
    extract_id_from_record_display, extract_id_from_select_value, extract_id_from_surreal_record,
    surreal_record_id_for,
};

#[cfg(feature = "remote")]
pub use remote::SurrealRemoteBackend;

#[cfg(all(test, feature = "embedded-mem"))]
mod instrumentation_smoke {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use std::sync::Arc;

    use surrealdb::engine::local::Mem;
    use valence_core::instrumentation::wrap_backend;
    use valence_telemetry::{install_telemetry_sink, RecordingSink};

    use super::{SDb, SurrealEmbeddedBackend};

    #[tokio::test]
    async fn wrapped_backend_emits_read_counter() {
        let sink = RecordingSink::new();
        install_telemetry_sink(Arc::new(sink.clone()));
        let db = SDb::init();
        db.connect::<Mem>(()).await.unwrap();
        db.use_ns("test").use_db("test").await.unwrap();
        let wrapped = wrap_backend(Arc::new(SurrealEmbeddedBackend::new(db)));
        let _ = wrapped.get_record("fixture", "id").await;
        assert!(!sink
            .recorded_counters_matching("valence_db_reads", &[("op", "get")])
            .is_empty());
    }
}
