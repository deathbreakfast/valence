# hop-pair-model-host

**Testkit / matrix fixture** — not a day-to-day application host. Supplies generated Project/Task models with abstract engine ids `hop_a` / `hop_b` so `valence-testkit` can register any physical `StorageAdapter` pair under stable keys. Adapter and matrix developers use this crate; integrators building apps should start with [`cross-backend-model-host`](../cross-backend-model-host/) instead.

## Prerequisites

- [`cross-backend-model-host`](../cross-backend-model-host/) — runnable mem↔sqlite hop demo with real engine ids.
- [`valence-testkit/README.md`](../../valence-testkit/README.md) — adapter registration and matrix layout.

## What to look at (file order)

1. [`src/lib.rs`](src/lib.rs) — `HOP_A`, `HOP_B`, `PROJECT_DB`, `TASK_DB`, re-exported generated types.
2. [`schemas/`](schemas/) — schema scan inputs (abstract `database:` evaluators).

## Run / verify

```bash
cargo check -p hop-pair-model-host
```

**Success signal:** crate compiles; generated models link with abstract engine constants.

## Next step

Runnable hop navigation: [`cross-backend-model-host`](../cross-backend-model-host/) (`cargo run -p cross-backend-model-host`).
