# acme-valence-backend-stub

Example third-party [`DatabaseBackend`](../../valence-core/src/backend/port.rs) with open `ENGINE_ID` (`acme_stub`). Exercises the `acme-stub` matrix row in `valence-testkit` / `valence-e2e` / `valence-bench` without a public crate feature flag. Minimal reference for custom engine crates and port contract + matrix catalog coverage for third-party adapters.

## Verify

```bash
cargo test -p acme-valence-backend-stub
```

## Host wiring

```rust
use acme_valence_backend_stub::{AcmeStubBackend, PRIMARY};

Valence::builder()
    .add_backend("primary", Arc::new(AcmeStubBackend::new()))
    .build()?;
// schema: database: PRIMARY
```

Published-adapter checklist: crate rustdoc (`cargo doc -p acme-valence-backend-stub --open`) and [`DatabaseBackend`](../../valence-core/src/backend/port.rs).
