//! SQLite → SQLite transfer: parity and resume after an injected failure.

#![allow(clippy::unwrap_used, clippy::expect_used)]
use std::sync::Arc;

use valence_backend_sqlite::SqliteBackend;
use valence_core::DatabaseBackend;
use valence_testkit::{run_transfer_contract, run_transfer_resume_contract};

async fn memory() -> Arc<dyn DatabaseBackend> {
    Arc::new(
        SqliteBackend::connect_memory()
            .await
            .expect("connect sqlite"),
    )
}

#[tokio::test]
async fn sqlite_to_sqlite_transfer_reads_back_identically() {
    run_transfer_contract(memory().await, memory().await)
        .await
        .expect("transfer contract");
}

#[tokio::test]
async fn sqlite_to_sqlite_transfer_resumes_after_failure() {
    run_transfer_resume_contract(memory().await, memory().await)
        .await
        .expect("transfer resume contract");
}

#[tokio::test]
async fn sqlite_to_sqlite_platform_set_parity() {
    let counts = valence_testkit::run_platform_transfer_contract(memory().await, memory().await)
        .await
        .expect("platform transfer contract");
    assert_eq!(counts.len(), 6);
    assert!(counts
        .iter()
        .all(|c| c.source_rows == c.target_rows && c.source_rows > 0));
}
