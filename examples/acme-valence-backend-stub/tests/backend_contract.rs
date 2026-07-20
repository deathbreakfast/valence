#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::print_stderr
)]
use std::sync::Arc;

use acme_valence_backend_stub::AcmeStubBackend;
use valence_testkit::run_backend_contract;

#[tokio::test]
async fn acme_stub_passes_port_contract() {
    let backend = Arc::new(AcmeStubBackend::new());
    run_backend_contract(backend)
        .await
        .expect("backend contract");
}
