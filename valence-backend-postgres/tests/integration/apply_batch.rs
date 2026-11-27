//! Same `apply_batch` contract as SQLite.
//!
//! Skips when `DATABASE_URL` is unset (local CI without Postgres).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stderr)]
use std::sync::Arc;

use valence_backend_postgres::PostgresBackend;
use valence_testkit::run_apply_batch_contract;

#[tokio::test]
async fn postgres_apply_batch_contract() {
    let backend = match PostgresBackend::builder().from_env_defaults().build().await {
        Ok(b) => b,
        Err(e) => {
            eprintln!("postgres builder resolve/connect failed: {e} — skipping");
            return;
        }
    };
    run_apply_batch_contract(Arc::new(backend))
        .await
        .expect("apply_batch contract");
}
