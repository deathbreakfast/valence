# acme-valence-backend-stub

Minimal third-party [`DatabaseBackend`](../../valence-core/src/backend/port.rs) with open `ENGINE_ID` (`acme_stub`). Proves custom engines need no public crate feature — depend on `valence-core`, implement the port, wire with `.add_backend(...)`. Exercises the `acme-stub` matrix row in `valence-testkit` / `valence-e2e` / `valence-bench`.

## Prerequisites

- `cargo doc -p uf-valence-core --open` → `DatabaseBackend` trait and `BackendCapabilities`.
- Optional: [`cross-backend-model-host`](../cross-backend-model-host/) — routing with published adapters first.

## What to look at (file order)

1. [`src/lib.rs`](src/lib.rs) — `ENGINE_ID`, `PRIMARY` (`DatabaseFromEngine`), `impl DatabaseBackend` (rustdoc checklist is the deep reference).
2. [`src/lib.rs`](src/lib.rs) (tests) — host wiring with `.add_backend("primary", …)`.

Key symbols for schema authors:

| Symbol | Role |
|--------|------|
| `ENGINE_ID` | Router engine slug (`"acme_stub"`) |
| `PRIMARY` | Schema `database:` evaluator (`"primary"` + `ENGINE_ID`) |
| `DatabaseBackend` | Port impl — create/read/merge/query paths |

## Run / verify

```bash
cargo test -p acme-valence-backend-stub
```

**Success signal:** crate tests pass; active backend reports `engine_id() == "acme_stub"`.

## Host wiring

```rust
use acme_valence_backend_stub::{AcmeStubBackend, PRIMARY};

Valence::builder()
    .add_backend("primary", Arc::new(AcmeStubBackend::new()))
    .build()?;
// schema: database: PRIMARY
```

## Next step

Matrix coverage: [`valence-testkit/README.md`](../../valence-testkit/README.md) and [`valence-e2e/README.md`](../../valence-e2e/README.md).
