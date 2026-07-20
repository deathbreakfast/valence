//! Boot Valence with the Postgres wire backend.
//!
//! Skips cleanly when `DATABASE_URL` is unset.
//!
//! ```bash
//! DATABASE_URL=postgres://localhost/valence \
//!   cargo run -p valence --example quickstart_postgres --features postgres
//! ```

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::print_stderr
)]
use std::sync::Arc;

use valence::{
    router_key, valence_schema, Database, DatabaseFromEngine, PostgresBackend, Valence,
    POSTGRES_ENGINE_ID,
};

const COUNTER_DB: DatabaseFromEngine = Database::from_engine("default", POSTGRES_ENGINE_ID);

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
    if std::env::var("DATABASE_URL").is_err() {
        eprintln!("skip: set DATABASE_URL to run this example");
        return Ok(());
    }

    let backend = PostgresBackend::from_env().await?;
    let key = router_key("default", POSTGRES_ENGINE_ID);
    let valence = Valence::builder()
        .add_backend("default", Arc::new(backend))
        .default_backend_key(key.clone())
        .build()?;

    assert_eq!(
        valence.backend_for_table("counter")?.engine_id(),
        POSTGRES_ENGINE_ID
    );
    println!("quickstart_postgres: Postgres backend registered at {key}");
    Ok(())
}
