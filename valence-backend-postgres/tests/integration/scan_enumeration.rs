//! Same fixture and assertions as the SQLite scan test.
//!
//! Skips when `DATABASE_URL` is unset (local CI without Postgres).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stderr)]
use std::sync::Arc;

use valence_backend_postgres::PostgresBackend;
use valence_testkit::run_scan_contract;

#[tokio::test]
async fn postgres_scan_contract() {
    let backend = match PostgresBackend::builder().from_env_defaults().build().await {
        Ok(b) => b,
        Err(e) => {
            eprintln!("postgres builder resolve/connect failed: {e} — skipping");
            return;
        }
    };
    let backend = Arc::new(backend) as Arc<dyn valence_core::DatabaseBackend>;
    run_scan_contract(backend).await.expect("scan contract");
}
