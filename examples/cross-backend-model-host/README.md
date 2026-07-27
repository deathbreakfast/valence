# cross-backend-model-host

Runnable proof of heterogeneous routing: generated Project on mem (`default`) and Task on sqlite (`archive`), with BelongsTo/HasMany navigation and same-backend `Model::query`. One process, two physical backends — the pattern you reach for when durability or engine choice differs per table.

**Limitation (0.1.x):** nested HasMany EXISTS queries across mem↔sqlite may return empty; BelongsTo/HasMany navigation and Project-side filters are the hard guarantees. See [`src/main.rs`](src/main.rs) step comments.

## Prerequisites

- [`product-model-host`](../product-model-host/) — connection semantics on a single backend.
- `sqlite` feature on the public crate (pulled by this crate's dependencies).

## What to look at (file order)

1. [`schemas/project_valence_schema.rs`](schemas/project_valence_schema.rs) — `database:` → mem / `default`.
2. [`schemas/task_valence_schema.rs`](schemas/task_valence_schema.rs) — `database:` → sqlite / `archive`.
3. [`src/main.rs`](src/main.rs) — dual backend boot, hop navigation, query shapes (Photon-style steps).
4. [`tests/routing_metadata.rs`](tests/routing_metadata.rs) — routing metadata assertions (optional).

## Run / verify

```bash
cargo run -p cross-backend-model-host
```

**Success signal:** stdout prints `cross-backend-model-host: Project(mem) ↔ Task(sqlite) hop + query OK (…)`.

Also:

```bash
cargo check -p cross-backend-model-host
cargo test -p cross-backend-model-host --test routing_metadata
```

## Next step

Admin/query surfaces: [`admin-runtime-host`](../admin-runtime-host/) — or Surreal inventory: [`embedded-bootstrap`](../embedded-bootstrap/).
