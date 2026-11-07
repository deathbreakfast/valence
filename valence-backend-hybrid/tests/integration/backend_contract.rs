//! Hybrid adapter satisfies the shared [`valence_testkit::run_backend_contract`].

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::print_stderr
)]

use std::sync::Arc;

use valence_backend_hybrid::HybridBackend;
use valence_backend_mem::InMemoryBackend;
use valence_testkit::run_backend_contract;

#[tokio::test]
async fn hybrid_backend_contract() {
    let hybrid = HybridBackend::builder()
        .primary(Arc::new(InMemoryBackend::new()))
        .warm_edges(false)
        .build()
        .await
        .expect("build hybrid");
    let backend = Arc::new(hybrid) as Arc<dyn valence_core::DatabaseBackend>;
    run_backend_contract(backend)
        .await
        .expect("hybrid port contract");
}
