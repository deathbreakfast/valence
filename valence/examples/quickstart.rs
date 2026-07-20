//! Quick start: declare a schema, boot an in-memory [`Valence`] runtime, prove registry discovery.
//!
//! ```bash
//! cargo run -p valence --example quickstart --features mem
//! ```

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::print_stderr
)]
use std::sync::Arc;

use valence::{
    valence_schema, Database, DatabaseFromEngine, InMemoryBackend, SchemaRegistry, Valence,
    MEM_ENGINE_ID,
};

const COUNTER_DB: DatabaseFromEngine = Database::from_engine("default", MEM_ENGINE_ID);

valence_schema! {
    Counter {
        table: "counter",
        version: "0.1.0",
        description: "Simple counter schema",
        database: COUNTER_DB,
        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
            value: { r#type: FieldType::Integer, required: true },
        ],
    }
}

#[tokio::main]
async fn main() -> valence::Result<()> {
    let valence = Valence::builder()
        .add_backend("default", Arc::new(InMemoryBackend::new()))
        .build()?;

    assert_eq!(
        valence.backend_for_table("counter")?.engine_id(),
        MEM_ENGINE_ID
    );

    let meta = SchemaRegistry::global()
        .get_schema("counter")
        .expect("counter schema registered via inventory");
    assert_eq!(meta.table_name, "counter");

    println!(
        "quickstart: schema {:?} v{} registered; Valence runtime ready",
        meta.table_name, meta.version
    );
    Ok(())
}
