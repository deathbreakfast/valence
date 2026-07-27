# product-model-host

Product-shaped Project/Task schemas with `HasMany` / `BelongsTo` connections, generated Model CRUD, and the deletion dispatcher hook on `Project::delete`. Same codegen pipeline as `codegen-host`, but with real-world connection metadata and ownership-adjacent delete semantics.

## Prerequisites

- [`codegen-host`](../codegen-host/) — `build.rs` + `include_generated_models!()` pattern understood.
- Optional: [`valence-macros/README.md`](../../valence-macros/README.md) — connection field reference.

## What to look at (file order)

1. [`schemas/project_valence_schema.rs`](schemas/project_valence_schema.rs) — `connections:` with `HasMany`, `on_delete: Cascade`.
2. [`schemas/task_valence_schema.rs`](schemas/task_valence_schema.rs) — `BelongsTo` reverse field.
3. [`build.rs`](build.rs) — shared codegen entry.
4. [`src/lib.rs`](src/lib.rs) — test module: create project + task, merge, delete queue capture.

## Run / verify

```bash
cargo test -p product-model-host -- --test-threads=1
```

**Success signal:** `product_model_crud_and_delete_queue` passes; one `DeletionRequest` with `root_table == "project"`.

## Next step

Split tables across engines: [`cross-backend-model-host`](../cross-backend-model-host/) (`cargo run -p cross-backend-model-host`).
