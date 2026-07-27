# valence-backend-sqlite

SQLite [`DatabaseBackend`](../valence-core/src/backend/port.rs) adapter (embedded). Enable via the public `valence` feature `sqlite` for durable single-host storage. Shares the SQL document-row pattern with postgres for adapter work.

```rust
pub const ENGINE_ID: &str = "sqlite";
```

## Wiring

```rust
use std::sync::Arc;
use valence::{SqliteBackend, Valence};

let backend = SqliteBackend::connect_memory().await?;
// Or: SqliteBackend::connect("/tmp/valence.db").await?;
let valence = Valence::builder()
    .add_backend("default", Arc::new(backend))
    .build()?;
```

Runnable: `cargo run -p uf-valence --example quickstart_sqlite --features sqlite`

Shared SQL helpers: [`valence-backend-sql`](../valence-backend-sql/). Contract: `DatabaseBackend` rustdoc (`cargo doc -p uf-valence-core --open`).
