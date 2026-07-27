# embedded-bootstrap

Surreal embedded end-to-end: connect in-process, discover logical DB names from linked `valence_schema!` inventory (`surreal-inventory`), bootstrap a shared router, and build both `Valence` and `RouterValenceFactory` from that router. Reach for this when Surreal is your embedded engine and you want schema-driven logical name registration instead of hand-wiring every key.

## Prerequisites

- `surreal` + `surreal-inventory` features on the public crate (enabled by this crate).
- [`valence-backend-surreal/README.md`](../../valence-backend-surreal/README.md) — engine helpers and env vars.

## What to look at (file order)

1. [`src/main.rs`](src/main.rs) — inline `DemoItem` schema, `connect_embedded_at_path`, `bootstrap_embedded_router_from_inventory`, factory build (Photon-style steps).

## Run / verify

```bash
cargo run -p embedded-bootstrap
```

**Success signal:** stdout prints `embedded-bootstrap: inventory router + ValenceFactory OK`.

## Next step

Feature flags and `VALENCE_EMBEDDED_*` env wiring: [`valence-backend-surreal/README.md`](../../valence-backend-surreal/README.md).
