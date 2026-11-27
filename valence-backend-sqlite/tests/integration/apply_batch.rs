//! `apply_batch` on SQLite commits every write or none, through `Valence` too.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use serde_json::json;
use valence_backend_sqlite::SqliteBackend;
use valence_core::{DatabaseBackend, Valence, WriteOp};
use valence_testkit::run_apply_batch_contract;

#[tokio::test]
async fn sqlite_memory_apply_batch_contract() {
    // The in-memory pool has one connection: every op in the batch must reuse the
    // transaction's connection or this hangs.
    let backend = SqliteBackend::connect_memory().await.expect("connect");
    run_apply_batch_contract(Arc::new(backend))
        .await
        .expect("apply_batch contract");
}

#[tokio::test]
async fn sqlite_file_apply_batch_contract() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("batch.db");
    let backend = SqliteBackend::connect(path.to_str().expect("utf-8 path"))
        .await
        .expect("connect");
    run_apply_batch_contract(Arc::new(backend))
        .await
        .expect("apply_batch contract");
}

#[tokio::test]
async fn valence_apply_batch_routes_to_sqlite() {
    let backend = Arc::new(SqliteBackend::connect_memory().await.expect("connect"));
    let valence = Valence::builder()
        .add_backend("default", backend.clone())
        .build()
        .expect("build");
    let rows = valence
        .apply_batch(vec![
            WriteOp::Create {
                table: "batch_routed".into(),
                content: json!({"id": "v1", "n": 1}),
            },
            WriteOp::Create {
                table: "batch_routed".into(),
                content: json!({"id": "v2", "n": 2}),
            },
        ])
        .await
        .expect("batch through Valence");
    assert_eq!(rows.len(), 2);
    assert!(backend
        .get_record("batch_routed", "v2")
        .await
        .unwrap()
        .is_some());
}
