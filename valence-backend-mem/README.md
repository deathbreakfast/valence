# valence-backend-mem

In-memory [`DatabaseBackend`](../valence-core/src/backend/port.rs) reference adapter. Enable via the public `valence` feature `mem` (default) for embedded storage; use `install_default_mem_router()` in tests. Serves as a template for third-party `ENGINE_ID` + port implementations.

```rust
pub const ENGINE_ID: &str = "inmemory_mem";
```

## Wiring

```rust
use std::sync::Arc;
use valence::{InMemoryBackend, Valence};

let valence = Valence::builder()
    .add_backend("default", Arc::new(InMemoryBackend::new()))
    .build()?;
```

Runnable: `cargo run -p uf-valence --example quickstart --features mem`
