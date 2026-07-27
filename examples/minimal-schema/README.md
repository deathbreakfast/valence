# minimal-schema

Compile-only proof that `valence_schema!` expands against the public crate, registers metadata via inventory, and boots a mem `Valence` — without codegen or generated models. Reach for this when you want to validate DSL syntax, explicit `policies:` blocks, or registry wiring before adding a `build.rs`.

## Prerequisites

- Workspace member `minimal-schema` (no backend features beyond `mem` on the public crate dependency).
- Familiarity with [`quickstart`](../../valence/examples/quickstart.rs) helps; this crate drops the runnable binary and adds explicit policies.

## What to look at (file order)

1. [`src/lib.rs`](src/lib.rs) — `valence_schema!` for `Smoke` with `PUBLIC_READ` policies.
2. [`src/lib.rs`](src/lib.rs) (`#[cfg(test)]`) — inventory registration and mem boot smoke.

## Run / verify

```bash
cargo check -p minimal-schema
cargo test -p minimal-schema
```

**Success signal:** `cargo check` completes; `schema_metadata_registers` and `valence_builds_with_mem_backend` pass.

## Next step

Add host codegen: [`codegen-host`](../codegen-host/) (`cargo test -p codegen-host`).
