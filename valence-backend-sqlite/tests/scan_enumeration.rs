#![allow(clippy::unwrap_used, clippy::expect_used)]
use std::sync::Arc;

use valence_backend_sqlite::SqliteBackend;
use valence_testkit::run_scan_contract;

#[tokio::test]
async fn sqlite_scan_contract() {
    let backend = Arc::new(
        SqliteBackend::connect_memory()
            .await
            .expect("connect sqlite"),
    ) as Arc<dyn valence_core::DatabaseBackend>;
    run_scan_contract(backend).await.expect("scan contract");
}
