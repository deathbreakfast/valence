//! Boot Valence with the console telemetry sink (`telemetry-console` feature).
//!
//! ```bash
//! cargo run -p valence --example quickstart_telemetry --features mem,telemetry-console
//! ```

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::print_stderr
)]
use std::sync::Arc;

use valence::{ConsoleSink, InMemoryBackend, Valence};

#[tokio::main]
async fn main() -> valence::Result<()> {
    let valence = Valence::builder()
        .add_backend("default", Arc::new(InMemoryBackend::new()))
        .telemetry_sink(Arc::new(ConsoleSink::default()))
        .build()?;

    assert!(valence.active_backend().is_ok());
    println!("quickstart_telemetry: mem backend + ConsoleSink installed");
    Ok(())
}
