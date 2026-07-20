//! Boot Valence with the embedded SQLite backend (in-memory database).
//!
//! ```bash
//! cargo run -p valence --example quickstart_sqlite --features sqlite
//! ```

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::print_stderr
)]
use std::sync::Arc;

use valence::{
    router_key, valence_schema, Database, DatabaseFromEngine, SqliteBackend, Valence,
    SQLITE_ENGINE_ID,
};

const COUNTER_DB: DatabaseFromEngine = Database::from_engine("default", SQLITE_ENGINE_ID);

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
    let backend = SqliteBackend::connect_memory().await?;
    let key = router_key("default", SQLITE_ENGINE_ID);
    let valence = Valence::builder()
        .add_backend("default", Arc::new(backend))
        .default_backend_key(key.clone())
        .build()?;

    assert_eq!(
        valence.backend_for_table("counter")?.engine_id(),
        SQLITE_ENGINE_ID
    );
    println!("quickstart_sqlite: SQLite backend registered at {key}");
    Ok(())
}
