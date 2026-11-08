//! SQLite → Postgres transfer: parity and resume after an injected failure.
//!
//! Skips when `DATABASE_URL` is unset (local CI without Postgres).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stderr)]
use std::sync::Arc;

use valence_backend_postgres::PostgresBackend;
use valence_backend_sqlite::SqliteBackend;
use valence_core::DatabaseBackend;
use valence_testkit::{run_transfer_contract, run_transfer_resume_contract};

async fn pair() -> Option<(Arc<dyn DatabaseBackend>, Arc<dyn DatabaseBackend>)> {
    let target = match PostgresBackend::builder().from_env_defaults().build().await {
        Ok(b) => Arc::new(b) as Arc<dyn DatabaseBackend>,
        Err(e) => {
            eprintln!("postgres builder resolve/connect failed: {e} — skipping");
            return None;
        }
    };
    let source = Arc::new(
        SqliteBackend::connect_memory()
            .await
            .expect("connect sqlite"),
    ) as Arc<dyn DatabaseBackend>;
    Some((source, target))
}

#[tokio::test]
async fn sqlite_to_postgres_transfer() {
    let Some((source, target)) = pair().await else {
        return;
    };
    run_transfer_contract(source.clone(), target.clone())
        .await
        .expect("transfer contract");
    run_transfer_resume_contract(source, target)
        .await
        .expect("transfer resume contract");
}

#[tokio::test]
async fn sqlite_to_postgres_platform_set_parity() {
    let Some((source, target)) = pair().await else {
        return;
    };
    let counts = valence_testkit::run_platform_transfer_contract(source, target)
        .await
        .expect("platform transfer contract");
    assert_eq!(counts.len(), 6);
    assert!(counts
        .iter()
        .all(|c| c.source_rows == c.target_rows && c.source_rows > 0));
}
