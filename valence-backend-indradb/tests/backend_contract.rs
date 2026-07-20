#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::print_stderr
)]
use std::sync::Arc;

use valence_backend_indradb::IndradbBackend;
use valence_testkit::run_backend_contract;

#[tokio::test]
async fn indradb_backend_passes_port_contract() {
    let backend = Arc::new(IndradbBackend::new());
    run_backend_contract(backend)
        .await
        .expect("backend contract");
}
