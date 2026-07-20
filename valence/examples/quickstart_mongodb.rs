//! Boot Valence with the MongoDB wire backend.
//!
//! Skips cleanly when neither `VALENCE_MONGODB_URI` nor `VALENCE_TEST_MONGODB_URI` is set.
//!
//! ```bash
//! VALENCE_MONGODB_URI=mongodb://localhost:27017 \
//!   cargo run -p valence --example quickstart_mongodb --features mongodb
//! ```

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::print_stderr
)]
use std::sync::Arc;

use valence::{
    router_key, valence_schema, Database, DatabaseFromEngine, MongoBackend, Valence,
    MONGODB_ENGINE_ID,
};

const COUNTER_DB: DatabaseFromEngine = Database::from_engine("default", MONGODB_ENGINE_ID);

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
    let uri_set = std::env::var("VALENCE_MONGODB_URI").is_ok()
        || std::env::var("VALENCE_TEST_MONGODB_URI").is_ok();
    if !uri_set {
        eprintln!(
            "skip: set VALENCE_MONGODB_URI (or VALENCE_TEST_MONGODB_URI) to run this example"
        );
        return Ok(());
    }

    let backend = MongoBackend::from_env().await?;
    let key = router_key("default", MONGODB_ENGINE_ID);
    let valence = Valence::builder()
        .add_backend("default", Arc::new(backend))
        .default_backend_key(key.clone())
        .build()?;

    assert_eq!(
        valence.backend_for_table("counter")?.engine_id(),
        MONGODB_ENGINE_ID
    );
    println!("quickstart_mongodb: MongoDB backend registered at {key}");
    Ok(())
}
