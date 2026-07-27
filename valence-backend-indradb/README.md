# valence-backend-indradb

IndraDB embedded graph [`DatabaseBackend`](../valence-core/src/backend/port.rs) adapter. Enable via the public `valence` feature `indradb` for an in-process graph store; demonstrates graph-edge capability patterns for adapters.

```rust
pub const ENGINE_ID: &str = "indradb";
```

## Wiring

```rust
use std::sync::Arc;
use valence::{IndradbBackend, Valence};

let valence = Valence::builder()
    .add_backend("default", Arc::new(IndradbBackend::new()))
    .build()?;
```

Runnable: `cargo run -p uf-valence --example quickstart_indradb --features indradb`

See `DatabaseBackend` rustdoc (`cargo doc -p uf-valence-core --open`).
