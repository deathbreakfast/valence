# valence-backend-hybrid

Hybrid [`DatabaseBackend`](../valence-core/src/backend/port.rs): IndraDB warm-edge
cache over a primary store (SQL in production; mem keeps the offline demo free of
`DATABASE_URL`).

## Audience

| Reader | Use this crate for |
|--------|-------------------|
| **App developers** | Enable via public crate feature `hybrid` |
| **Host integrators** | Multi-logical registration of one hybrid backend |

```rust
pub const ENGINE_ID: &str = "hybrid";
```

## Wiring

See the facade example (mem primary, several logical names):

```bash
cargo run -p uf-valence --example hybrid_multi_logical --features hybrid,mem
```

Topology catalog: [Embedded](https://docs.rs/uf-valence/latest/valence/index.html#embedded-one-process).
