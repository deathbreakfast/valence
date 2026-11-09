#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::print_stderr
)]
use std::sync::Arc;

use valence_backend_sqlite::SqliteBackend;
use valence_testkit::run_backend_contract;

#[tokio::test]
async fn sqlite_backend_contract() {
    let backend = Arc::new(
        SqliteBackend::connect_memory()
            .await
            .expect("connect sqlite"),
    ) as Arc<dyn valence_core::DatabaseBackend>;
    run_backend_contract(backend).await.expect("contract");
}
