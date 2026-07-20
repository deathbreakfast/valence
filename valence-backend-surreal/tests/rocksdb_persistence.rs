//! RocksDB persistence smoke test.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::print_stderr
)]
use std::time::Duration;

use surrealdb::engine::local::RocksDb;
use surrealdb::opt::Config;
use surrealdb::Surreal;
use valence_backend_surreal::{SDb, SurrealEmbeddedBackend};
use valence_core::DatabaseBackend;

async fn open_rocksdb(path: &str) -> SurrealEmbeddedBackend {
    let db: SDb = Surreal::new::<RocksDb>((path.to_string(), Config::default()))
        .await
        .expect("rocksdb open");
    db.use_ns("test").use_db("test").await.unwrap();
    SurrealEmbeddedBackend::new(db)
}

#[tokio::test]
async fn rocksdb_persists_across_reopen() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().to_string_lossy().to_string();

    {
        let b = open_rocksdb(&path).await;
        b.upsert_record("widget", "r1", serde_json::json!({ "name": "persisted" }))
            .await
            .expect("upsert");
    }

    // Surreal embedded RocksDB releases LOCK asynchronously on Drop; brief wait avoids
    // "lock hold by current process" on immediate reopen in the same test process.
    tokio::time::sleep(Duration::from_millis(750)).await;

    let b = open_rocksdb(&path).await;
    let row = b
        .get_record("widget", "r1")
        .await
        .expect("get")
        .expect("row");
    assert_eq!(row.get("name").and_then(|v| v.as_str()), Some("persisted"));
}
