# valence-backend-hybrid

Hybrid [`DatabaseBackend`](../valence-core/src/backend/port.rs): IndraDB warm-edge
cache over a primary store (SQL in production; mem keeps the offline demo free of
`DATABASE_URL`). Enable via the public `valence` feature `hybrid` and register one hybrid backend under multiple logical names.

```rust
pub const ENGINE_ID: &str = "hybrid";
```

## Wiring

See the `hybrid_multi_logical` example (mem primary, several logical names):

```bash
cargo run -p uf-valence --example hybrid_multi_logical --features hybrid,mem
```

Topology catalog: [Embedded](https://docs.rs/uf-valence/latest/valence/index.html#embedded-one-process).
