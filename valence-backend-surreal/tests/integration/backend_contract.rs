#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::print_stderr
)]
use std::sync::Arc;

use valence_backend_surreal::{connect_embedded_at_path, EmbeddedEngine, SurrealEmbeddedBackend};
use valence_testkit::run_backend_contract;

#[tokio::test]
async fn surreal_mem_passes_port_contract() {
    let db = connect_embedded_at_path(EmbeddedEngine::Mem, "mem", "test", "test")
        .await
        .expect("connect");
    let backend = Arc::new(SurrealEmbeddedBackend::new(db));
    run_backend_contract(backend)
        .await
        .expect("backend contract");
}
