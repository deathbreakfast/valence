//! Connect an embedded Surreal engine and wire it through [`ValenceBuilder`].
//!
//! ```bash
//! cargo run -p valence --example surreal_embedded --features surreal
//! ```

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::print_stderr
)]
use std::sync::Arc;

use valence::{
    router_key, valence_schema, Database, DatabaseFromEngine, SDb, SurrealEmbeddedBackend, Valence,
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
    let db = SDb::init();
    db.connect::<surrealdb::engine::local::Mem>(())
        .await
        .expect("connect embedded mem engine");
    db.use_ns("demo")
        .use_db("demo")
        .await
        .expect("select ns/db");

    let key = router_key("default", SURREAL_ENGINE_ID);
    let valence = Valence::builder()
        .add_backend("default", Arc::new(SurrealEmbeddedBackend::new(db)))
        .default_backend_key(key.clone())
        .build()?;

    assert_eq!(
        valence.backend_for_table("counter")?.engine_id(),
        SURREAL_ENGINE_ID
    );
    println!("surreal_embedded: Surreal backend registered at {key}");
    Ok(())
}
