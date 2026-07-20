//! End-to-end embedded bootstrap: connect → inventory router → [`valence::ValenceBuilder`].

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::print_stderr
)]

use std::sync::Arc;
use valence::prelude::*;
use valence::{
    bootstrap_embedded_router_from_inventory, connect_embedded_at_path, EmbeddedEngine,
    RegisterEmbeddedLogicalNamesOptions, RouterValenceFactory, RouterValenceFactoryConfig, Valence,
    SURREAL_ENGINE_ID,
};

pub const DEMO_DB: DatabaseFromEngine = Database::from_engine("default", SURREAL_ENGINE_ID);

valence_schema! {
    DemoItem {
        table: "demo_item",
        version: "0.1.0",
        database: DEMO_DB,
        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
        ],
    }
}

#[tokio::main]
async fn main() {
    let db = connect_embedded_at_path(EmbeddedEngine::Mem, "", "demo", "demo")
        .await
        .expect("connect");
    let router = bootstrap_embedded_router_from_inventory(
        db,
        RegisterEmbeddedLogicalNamesOptions::default(),
    )
    .expect("bootstrap router");

    let default_key = valence::router_key("default", SURREAL_ENGINE_ID);
    let valence = Valence::builder()
        .database_router(Arc::clone(&router))
        .default_backend_key(default_key.clone())
        .build()
        .expect("valence");

    let background =
        RouterValenceFactory::arc(router, RouterValenceFactoryConfig::new(default_key))
            .build(&serde_json::json!({"role": "system"}))
            .expect("factory build");

    assert!(valence.active_backend().is_ok());
    assert!(background.active_backend().is_ok());
    println!("embedded-bootstrap: inventory router + ValenceFactory OK");
}
