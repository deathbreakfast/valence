//! Inventory bootstrap smoke test.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::print_stderr
)]
use valence_core::router_key::router_key;

use valence_backend_surreal::{
    bootstrap_embedded_router_from_inventory, connect_embedded_at_path, EmbeddedEngine,
    RegisterEmbeddedLogicalNamesOptions, ENGINE_ID,
};

#[tokio::test]
async fn inventory_bootstrap_registers_default_logical() {
    let db = connect_embedded_at_path(EmbeddedEngine::Mem, "", "test", "test")
        .await
        .expect("connect");
    let router = bootstrap_embedded_router_from_inventory(
        db,
        RegisterEmbeddedLogicalNamesOptions::default(),
    )
    .expect("bootstrap");
    let key = router_key("default", ENGINE_ID);
    assert!(router.resolve(&key).is_ok());
}
