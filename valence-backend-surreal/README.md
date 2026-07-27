# valence-backend-surreal

SurrealDB-backed [`DatabaseBackend`](https://github.com/unified-field-dev/valence) reference adapter. All `surrealdb` dependencies stay in this crate only — never in `valence-core`. Enable via the public `valence` `surreal` feature after wiring at boot; study alongside `valence-backend-mem` for the port contract. `./scripts/gate.sh` blocks `surrealdb` in `valence-core`.

## Features

| Feature | Meaning |
|---------|---------|
| `embedded-mem` (default) | In-process Surreal `Mem` engine |
| `embedded-rocksdb` | On-disk embedded RocksDB |
| `remote` | WebSocket/HTTP via `Surreal<Any>` |
| `inventory` | Discover logical DB names from linked `valence_schema!` |
| `connect-env` | `connect_embedded_from_env()` via `VALENCE_EMBEDDED_*` |
| `instrumentation` | Wrap backends with instrumentation telemetry hooks |

## Wiring

```rust
use std::sync::Arc;
use valence_backend_surreal::{SurrealEmbeddedBackend, ENGINE_ID, SDb};
use valence_core::{router_key, ValenceBuilder};

let db = SDb::init();
db.connect::<surrealdb::engine::local::Mem>(()).await?;
db.use_ns("prod").use_db("prod").await?;
let backend = Arc::new(SurrealEmbeddedBackend::new(db));
let valence = ValenceBuilder::new()
    .add_backend("default", backend)
    .default_backend_key(router_key("default", ENGINE_ID))
    .build()?;
```

### Inventory bootstrap

```rust
use valence_backend_surreal::{
    bootstrap_embedded_router_from_inventory, connect_embedded_at_path, EmbeddedEngine,
    RegisterEmbeddedLogicalNamesOptions,
};

let db = connect_embedded_at_path(EmbeddedEngine::Mem, "", "prod", "prod").await?;
let router = bootstrap_embedded_router_from_inventory(
    db,
    RegisterEmbeddedLogicalNamesOptions::default(),
)?;
```

Runnable: `cargo run -p uf-valence --example surreal_embedded --features surreal`

Inventory bootstrap host: [`examples/embedded-bootstrap/`](../examples/embedded-bootstrap/).

Topology catalog: [Embedded](https://docs.rs/uf-valence/latest/valence/index.html#embedded-one-process) /
[Remote (wire)](https://docs.rs/uf-valence/latest/valence/index.html#remote-wire).

Telemetry label: `database_type=surrealdb` on instrumentation hooks.

## Schema evaluator (host)

```rust
pub const SURREAL_DEFAULT: DatabaseFromEngine =
    Database::from_engine("default", valence_backend_surreal::ENGINE_ID);
```
