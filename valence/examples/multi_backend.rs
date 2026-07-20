//! Register multiple logical backends on one router and select a default key.
//!
//! ```bash
//! cargo run -p valence --example multi_backend --features mem
//! ```

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::print_stderr
)]
use std::sync::Arc;

use valence::{router_key, InMemoryBackend, Valence, MEM_ENGINE_ID};

#[tokio::main]
async fn main() -> valence::Result<()> {
    let primary = router_key("primary", MEM_ENGINE_ID);
    let archive = router_key("archive", MEM_ENGINE_ID);

    let valence = Valence::builder()
        .add_backend("primary", Arc::new(InMemoryBackend::new()))
        .add_backend("archive", Arc::new(InMemoryBackend::new()))
        .default_backend_key(primary.clone())
        .build()?;

    assert_eq!(valence.active_backend()?.engine_id(), MEM_ENGINE_ID);
    assert_ne!(primary, archive);
    println!("multi_backend: registered {primary} (active) and {archive}");
    Ok(())
}
