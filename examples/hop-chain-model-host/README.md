# hop-chain-model-host

**Testkit / matrix fixture** — not a day-to-day application host. Four-table chain (Org → Project → Task → Note) on abstract engines `hop_a`…`hop_d` for nested inner-query hop E2E and bm-v25 matrix rows. Adapter developers and bench/e2e harnesses depend on this crate; application integrators should use [`cross-backend-model-host`](../cross-backend-model-host/) for a runnable multi-backend demo.

## Prerequisites

- [`hop-pair-model-host`](../hop-pair-model-host/) — two-table abstract hop pattern.
- [`cross-backend-model-host`](../cross-backend-model-host/) — real-engine navigation reference.
- [`valence-e2e/README.md`](../../valence-e2e/README.md) — chain fixture usage in matrix.

## What to look at (file order)

1. [`src/lib.rs`](src/lib.rs) — `HOP_A`…`HOP_D`, per-table `*_DB` evaluators, re-exported generated types.
2. [`schemas/`](schemas/) — four linked schema scan inputs.

## Run / verify

```bash
cargo check -p hop-chain-model-host
```

**Success signal:** crate compiles; four generated models link with distinct abstract engine ids.

## Next step

Runnable hop: [`cross-backend-model-host`](../cross-backend-model-host/) — matrix docs: [`valence-e2e/README.md`](../../valence-e2e/README.md).
