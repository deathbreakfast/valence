# codegen-host

End-to-end proof that a host-owned `schemas/` tree, a one-line `build.rs`, and `valence::include_generated_models!()` produce a working generated `impl Model` against the public crate. This is the minimal codegen host — one table (`Widget`), create / get / merge in a co-located test.

## Prerequisites

- [`valence-codegen/README.md`](../../valence-codegen/README.md) — scan roots and `CodegenConfig`.
- Quickstarts or [`minimal-schema`](../minimal-schema/) — schema DSL already familiar.

## What to look at (file order)

1. [`build.rs`](build.rs) — `valence_codegen::build()`.
2. [`schemas/widget_valence_schema.rs`](schemas/widget_valence_schema.rs) — scan input (not `mod`-linked).
3. [`src/lib.rs`](src/lib.rs) — `include_generated_models!()` and the `#[cfg(test)]` module (Photon-style step comments).

## Run / verify

```bash
cargo test -p codegen-host
```

**Success signal:** `generated_widget_impl_model_compiles_and_runs` passes (create → get → merge on `Widget`).

## Next step

Product shapes with connections and deletion: [`product-model-host`](../product-model-host/) (`cargo test -p product-model-host -- --test-threads=1`).
