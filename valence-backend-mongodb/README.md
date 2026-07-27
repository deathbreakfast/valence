# valence-backend-mongodb

MongoDB wire [`DatabaseBackend`](../valence-core/src/backend/port.rs) adapter. Enable via the public `valence` feature `mongodb` and wire remote Mongo at boot using the builder's `from_env_defaults` pattern.

```rust
pub const ENGINE_ID: &str = "mongodb";
```

## Environment

| Variable | Meaning |
|----------|---------|
| `VALENCE_MONGODB_URI` | Mongo connection URI |
| `VALENCE_TEST_MONGODB_URI` | Fallback test URI |
| `VALENCE_MONGODB_DB` | Database name (default `valence`) |

## Wiring

```rust
use std::sync::Arc;
use valence::{MongoBackend, Valence};

let backend = MongoBackend::from_env().await?;
// Or: MongoBackend::connect("mongodb://localhost:27017", "valence").await?;

let valence = Valence::builder()
    .add_backend("default", Arc::new(backend))
    .build()?;
```

## Docker

```bash
docker run --rm -d --name valence-mongodb -p 27017:27017 mongo:7
export VALENCE_MONGODB_URI=mongodb://127.0.0.1:27017
```

Runnable (skips when unset):

```bash
VALENCE_MONGODB_URI=mongodb://127.0.0.1:27017 \
  cargo run -p uf-valence --example quickstart_mongodb --features mongodb
```

See `DatabaseBackend` rustdoc (`cargo doc -p uf-valence-core --open`).
