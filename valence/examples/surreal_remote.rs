//! Connect to a remote SurrealDB server (WebSocket/HTTP) via `Surreal<Any>`.
//!
//! Skips cleanly when `VALENCE_SURREAL_URL` is unset.
//!
//! ```bash
//! docker run --rm -d --name valence-surreal -p 8000:8000 \
//!   surrealdb/surrealdb:latest start --user root --pass root memory
//! VALENCE_SURREAL_URL=ws://127.0.0.1:8000/rpc \
//!   cargo run -p uf-valence --example surreal_remote --features surreal-remote
//! ```

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::print_stderr
)]
use std::sync::Arc;

use valence::{
    router_key, valence_schema, Database, DatabaseFromEngine, SurrealRemoteBackend, Valence,
    SURREAL_ENGINE_ID,
};

const COUNTER_DB: DatabaseFromEngine = Database::from_engine("default", SURREAL_ENGINE_ID);

valence_schema! {
    Counter {
        table: "counter",
        version: "0.1.0",
        description: "Simple counter",
        database: COUNTER_DB,
        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
            value: { r#type: FieldType::Integer, required: true },
        ],
    }
}

#[tokio::main]
async fn main() -> valence::Result<()> {
    let Ok(url) = std::env::var("VALENCE_SURREAL_URL") else {
        eprintln!("skip: set VALENCE_SURREAL_URL to run this example");
        return Ok(());
    };

    let db = surrealdb::engine::any::connect(url)
        .await
        .expect("connect remote surreal");
    db.use_ns("demo")
        .use_db("demo")
        .await
        .expect("select ns/db");

    let key = router_key("default", SURREAL_ENGINE_ID);
    let valence = Valence::builder()
        .add_backend("default", Arc::new(SurrealRemoteBackend::new(db)))
        .default_backend_key(key.clone())
        .build()?;

    assert_eq!(
        valence.backend_for_table("counter")?.engine_id(),
        SURREAL_ENGINE_ID
    );
    println!("surreal_remote: Surreal remote backend registered at {key}");
    Ok(())
}
