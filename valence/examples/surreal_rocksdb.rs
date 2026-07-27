//! On-disk embedded Surreal via RocksDB — durable single-node storage, no external server.
//!
//! ```bash
//! cargo run -p uf-valence --example surreal_rocksdb --features surreal-rocksdb
//! ```

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::print_stderr
)]
use std::sync::Arc;

use valence::{
    connect_embedded_at_path, router_key, valence_schema, Database, DatabaseFromEngine,
    EmbeddedEngine, SurrealEmbeddedBackend, Valence, SURREAL_ENGINE_ID,
};

const COUNTER_DB: DatabaseFromEngine = Database::from_engine("default", SURREAL_ENGINE_ID);

valence_schema! {
    Counter {
        table: "counter",
        version: "0.1.0",
        description: "Simple counter",
        database: COUNTER_DB,
        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
            value: { r#type: FieldType::Integer, required: true },
        ],
    }
}

#[tokio::main]
async fn main() -> valence::Result<()> {
    // Step 1 — RocksDB needs an on-disk directory; a tempdir keeps this example self-contained.
    // Swap in a stable path (e.g. from `VALENCE_EMBEDDED_PATH`) for real persistence across runs.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().to_string_lossy().to_string();

    let db = connect_embedded_at_path(EmbeddedEngine::RocksDb, &path, "demo", "demo")
        .await
        .expect("connect embedded rocksdb");

    let key = router_key("default", SURREAL_ENGINE_ID);
    let valence = Valence::builder()
        .add_backend("default", Arc::new(SurrealEmbeddedBackend::new(db)))
        .default_backend_key(key.clone())
        .build()?;

    assert_eq!(
        valence.backend_for_table("counter")?.engine_id(),
        SURREAL_ENGINE_ID
    );
    println!("surreal_rocksdb: Surreal RocksDB backend registered at {key} (path: {path})");
    Ok(())
}
