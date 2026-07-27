# cross-backend-model-host

Generated Project (mem / `default`) and Task (sqlite / `archive`) models with
HasMany / BelongsTo connections — one process, two backends, hop + `Model::query`.
Demonstrates heterogeneous routing, connection hops, and nested queries.

## Run

```bash
cargo run -p cross-backend-model-host
```

Success: stdout prints `cross-backend-model-host: Project(mem) ↔ Task(sqlite) hop + query OK (…)`.

Also:

```bash
cargo check -p cross-backend-model-host
cargo test -p cross-backend-model-host --test routing_metadata
```

See `DatabaseBackend` / `DatabaseRouter` rustdoc (`cargo doc -p uf-valence-core --open`)
and the public crate **How to run examples** (`valence/README.md`).
