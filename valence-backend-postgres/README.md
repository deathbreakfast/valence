# valence-backend-postgres

Postgres wire [`DatabaseBackend`](../valence-core/src/backend/port.rs) adapter. Enable via the public `valence` feature `postgres` and wire remote Postgres at boot using the builder's `from_env_defaults` pattern.

```rust
pub const ENGINE_ID: &str = "postgres";
```

## Environment

| Variable | Meaning |
|----------|---------|
| `DATABASE_URL` | Postgres connection URL (required unless `.url()` is set) |

## Wiring

```rust
use std::sync::Arc;
use valence::{PostgresBackend, Valence};

// Explicit:
let backend = PostgresBackend::connect("postgres://localhost/valence").await?;
// Or env:
let backend = PostgresBackend::from_env().await?;

let valence = Valence::builder()
    .add_backend("default", Arc::new(backend))
    .build()?;
```

Builder: `PostgresBackend::builder().url(...).from_env_defaults().build().await?`

## Docker

```bash
docker run --rm -d --name valence-postgres -p 5432:5432 -e POSTGRES_PASSWORD=postgres postgres:16
export DATABASE_URL=postgres://postgres:postgres@127.0.0.1:5432/postgres
```

Runnable (skips when unset):

```bash
DATABASE_URL=postgres://postgres:postgres@127.0.0.1:5432/postgres \
  cargo run -p uf-valence --example quickstart_postgres --features postgres
```

Live multi-remote routing alongside Redis: [`examples/remote-multi-backend-host`](../examples/remote-multi-backend-host/).

See `DatabaseBackend` rustdoc (`cargo doc -p uf-valence-core --open`).
