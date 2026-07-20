//! Standalone host integrator smoke: registries + admin read query (no UI crate).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::print_stderr
)]
use std::sync::Arc;

use valence::{
    register_noop_deletion_dispatcher_for_tests, Actor, DatabaseBackend, InMemoryBackend,
    QueryCore, SchemaRegistry, TraitRegistry, Valence,
};

#[tokio::main]
async fn main() {
    register_noop_deletion_dispatcher_for_tests();

    let backend: Arc<dyn DatabaseBackend> = Arc::new(InMemoryBackend::new());
    backend
        .create_record(
            "smoke",
            serde_json::json!({"id": "demo", "note": "admin-runtime-host"}),
        )
        .await
        .expect("seed smoke row");

    let valence = Valence::builder()
        .add_backend("default", Arc::clone(&backend))
        .with_actor(Actor::System {
            operation: "admin-runtime-host".to_string(),
        })
        .build()
        .expect("build valence");

    let schemas = SchemaRegistry::global().list_schemas();
    let traits = TraitRegistry::global().list_traits();
    println!("schemas={schemas:?} traits={traits:?}");

    let row = QueryCore::get_record_json("smoke", "demo", &valence)
        .await
        .expect("read")
        .expect("row exists");
    println!("entity={row}");

    let ids = QueryCore::latest_ids("smoke", 10, &valence)
        .await
        .expect("latest_ids");
    println!("latest_ids={ids:?}");

    println!("admin-runtime-host: OK");
}
