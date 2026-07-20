//! Boot Valence with the Redis wire backend.
//!
//! Skips cleanly when neither `VALENCE_REDIS_URL` nor `VALENCE_TEST_REDIS_URL` is set.
//!
//! ```bash
//! VALENCE_REDIS_URL=redis://127.0.0.1:6379 \
//!   cargo run -p valence --example quickstart_redis --features redis
//! ```

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::print_stderr
)]
use std::sync::Arc;

use valence::{
    router_key, valence_schema, Database, DatabaseFromEngine, RedisBackend, Valence,
    REDIS_ENGINE_ID,
};

const COUNTER_DB: DatabaseFromEngine = Database::from_engine("default", REDIS_ENGINE_ID);

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
    let url_set = std::env::var("VALENCE_REDIS_URL").is_ok()
        || std::env::var("VALENCE_TEST_REDIS_URL").is_ok();
    if !url_set {
        eprintln!("skip: set VALENCE_REDIS_URL (or VALENCE_TEST_REDIS_URL) to run this example");
        return Ok(());
    }

    let backend = RedisBackend::from_env().await?;
    let key = router_key("default", REDIS_ENGINE_ID);
    let valence = Valence::builder()
        .add_backend("default", Arc::new(backend))
        .default_backend_key(key.clone())
        .build()?;

    assert_eq!(
        valence.backend_for_table("counter")?.engine_id(),
        REDIS_ENGINE_ID
    );
    println!("quickstart_redis: Redis backend registered at {key}");
    Ok(())
}
