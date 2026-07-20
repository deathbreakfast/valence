//! Boot Valence with the embedded IndraDB graph backend.
//!
//! ```bash
//! cargo run -p valence --example quickstart_indradb --features indradb
//! ```

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::print_stderr
)]
use std::sync::Arc;

use valence::{
    router_key, valence_schema, Database, DatabaseFromEngine, IndradbBackend, Valence,
    INDRADB_ENGINE_ID,
};

const COUNTER_DB: DatabaseFromEngine = Database::from_engine("default", INDRADB_ENGINE_ID);

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
    let key = router_key("default", INDRADB_ENGINE_ID);
    let valence = Valence::builder()
        .add_backend("default", Arc::new(IndradbBackend::new()))
        .default_backend_key(key.clone())
        .build()?;

    assert_eq!(
        valence.backend_for_table("counter")?.engine_id(),
        INDRADB_ENGINE_ID
    );
    println!("quickstart_indradb: IndraDB backend registered at {key}");
    Ok(())
}
