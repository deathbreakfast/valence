#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::print_stderr
)]
use std::sync::Arc;

use valence_backend_redis::RedisBackend;
use valence_testkit::run_backend_contract;

#[tokio::test]
async fn redis_backend_passes_port_contract() {
    let backend = match RedisBackend::builder().from_env_defaults().build().await {
        Ok(b) => b,
        Err(e) => {
            eprintln!("redis builder resolve/connect failed: {e} — skipping");
            return;
        }
    };
    let backend = Arc::new(backend) as Arc<dyn valence_core::DatabaseBackend>;
    run_backend_contract(backend)
        .await
        .expect("backend contract");
}
