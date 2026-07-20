#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::print_stderr
)]
use std::sync::Arc;

use valence_backend_postgres::PostgresBackend;
use valence_testkit::run_backend_contract;

#[tokio::test]
async fn postgres_backend_contract() {
    let backend = match PostgresBackend::builder().from_env_defaults().build().await {
        Ok(b) => b,
        Err(e) => {
            eprintln!("postgres builder resolve/connect failed: {e} — skipping");
            return;
        }
    };
    let backend = Arc::new(backend) as Arc<dyn valence_core::DatabaseBackend>;
    run_backend_contract(backend).await.expect("contract");
}
