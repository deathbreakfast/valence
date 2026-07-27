# Valence examples walkthrough

This ladder is the ordered path from “schema compiles” to “custom engine in the matrix.” Each crate is a teaching card: run it, open the files it names, then follow **Next step** to the next rung.

Quickstarts live under [`valence/examples/`](../valence/examples/) (the public crate, not this directory). Workspace hosts here prove codegen, product shapes, cross-backend hops, admin surfaces, Surreal bootstrap, and third-party adapters.

---

## Quickstarts

These are single-file `[[example]]` binaries — the fastest way to see Valence boot and register a schema without a host `build.rs`.

### `quickstart` — schema + mem boot

**Teaches:** Declare `valence_schema!`, wire `InMemoryBackend`, prove `SchemaRegistry` discovery.

```bash
cargo run -p uf-valence --example quickstart --features mem
```

**Open first:** [`valence/examples/quickstart.rs`](../valence/examples/quickstart.rs)

**Success:** stdout prints `quickstart: schema "counter" … registered; Valence runtime ready`.

**Next step:** [`quickstart_sqlite`](#quickstart_sqlite--durable-embedded) or workspace [`minimal-schema`](#minimal-schema--compile-only-dsl).

---

### `quickstart_sqlite` — durable embedded

**Teaches:** Same schema contract on a durable SQLite backend (`SQLITE_ENGINE_ID`, `SqliteBackend::connect_memory`).

```bash
cargo run -p uf-valence --example quickstart_sqlite --features sqlite
```

**Open first:** [`valence/examples/quickstart_sqlite.rs`](../valence/examples/quickstart_sqlite.rs)

**Success:** schema registers and runtime boots without panic.

**Next step:** [`multi_backend`](#multi_backend--two-logical-keys).

---

### `multi_backend` — two logical keys

**Teaches:** One process, two router keys (`primary` / `archive`), `default_backend_key`, per-table routing via `database:`.

```bash
cargo run -p uf-valence --example multi_backend --features mem
```

**Open first:** [`valence/examples/multi_backend.rs`](../valence/examples/multi_backend.rs)

**Success:** stdout confirms both logical backends resolve.

**Next step:** workspace [`codegen-host`](#codegen-host--build-time-models) for typed `Model` CRUD.

---

## Workspace hosts (this directory)

### `minimal-schema` — compile-only DSL

**Teaches:** `valence_schema!` expands, registers inventory metadata, and builds a `Valence` runtime — no codegen, no generated models. Reach for this when validating DSL syntax or policy blocks before adding `build.rs`.

```bash
cargo check -p minimal-schema
cargo test -p minimal-schema
```

**Open first:** [`minimal-schema/src/lib.rs`](minimal-schema/src/lib.rs)

**Success:** `cargo check` passes; tests confirm schema inventory and mem boot.

**Next step:** [`codegen-host`](#codegen-host--build-time-models).

---

### `codegen-host` — build-time models

**Teaches:** Host-owned `schemas/` → `valence_codegen::build()` in `build.rs` → `include_generated_models!()` → generated `impl Model` (create / get / merge).

```bash
cargo test -p codegen-host
```

**Open first:** [`codegen-host/build.rs`](codegen-host/build.rs) → [`codegen-host/schemas/widget_valence_schema.rs`](codegen-host/schemas/widget_valence_schema.rs) → [`codegen-host/src/lib.rs`](codegen-host/src/lib.rs) (test module)

**Success:** `generated_widget_impl_model_compiles_and_runs` passes.

**Next step:** [`product-model-host`](#product-model-host--connections--deletion) for connections and deletion queue.

---

### `product-model-host` — connections + deletion

**Teaches:** Product-shaped Project/Task schemas with `HasMany` / `BelongsTo`, generated CRUD, and the deletion dispatcher hook on `Project::delete`.

```bash
cargo test -p product-model-host -- --test-threads=1
```

**Open first:** [`product-model-host/schemas/project_valence_schema.rs`](product-model-host/schemas/project_valence_schema.rs) → [`product-model-host/src/lib.rs`](product-model-host/src/lib.rs) (test module)

**Success:** `product_model_crud_and_delete_queue` passes; deletion request captured with `root_table == "project"`.

**Next step:** [`cross-backend-model-host`](#cross-backend-model-host--mem--sqlite-hop) when tables must live on different engines.

---

### `cross-backend-model-host` — mem ↔ sqlite hop

**Teaches:** Heterogeneous routing (Project on mem / `default`, Task on sqlite / `archive`), BelongsTo/HasMany navigation across backends, and same-backend `Model::query`. Nested HasMany EXISTS across mem↔sqlite may return empty in 0.1.x — navigation is the hard guarantee.

```bash
cargo run -p cross-backend-model-host
```

**Open first:** [`cross-backend-model-host/schemas/project_valence_schema.rs`](cross-backend-model-host/schemas/project_valence_schema.rs) → [`cross-backend-model-host/src/main.rs`](cross-backend-model-host/src/main.rs)

**Success:** stdout prints `cross-backend-model-host: Project(mem) ↔ Task(sqlite) hop + query OK (…)`.

**Next step:** [`admin-runtime-host`](#admin-runtime-host--registry--querycore) for admin/query surfaces, or [`embedded-bootstrap`](#embedded-bootstrap--surreal-inventory) for Surreal inventory.

---

### `admin-runtime-host` — registry + QueryCore

**Teaches:** Boot mem backend, list `SchemaRegistry` / `TraitRegistry`, read rows via `QueryCore::get_record_json` and `latest_ids` — the wiring pattern for admin tooling without a UI crate.

```bash
cargo run -p admin-runtime-host
```

**Open first:** [`admin-runtime-host/src/main.rs`](admin-runtime-host/src/main.rs)

**Success:** stdout prints schema/trait lists, seeded row JSON, `latest_ids`, and `admin-runtime-host: OK`.

**Next step:** [`embedded-bootstrap`](#embedded-bootstrap--surreal-inventory) if you need Surreal embedded inventory, or [`acme-valence-backend-stub`](#acme-valence-backend-stub--custom-engine) for a custom adapter.

---

### `embedded-bootstrap` — Surreal inventory

**Teaches:** Connect embedded Surreal, discover logical DB names from linked `valence_schema!` inventory, bootstrap a router, and build both `Valence` and `RouterValenceFactory` from the same router.

```bash
cargo run -p embedded-bootstrap
```

**Open first:** [`embedded-bootstrap/src/main.rs`](embedded-bootstrap/src/main.rs)

**Success:** stdout prints `embedded-bootstrap: inventory router + ValenceFactory OK`.

**Next step:** [`valence-backend-surreal/README.md`](../valence-backend-surreal/README.md) for feature flags and env vars.

---

### `acme-valence-backend-stub` — custom engine

**Teaches:** Implement `DatabaseBackend` in a separate crate with open `ENGINE_ID`, export `PRIMARY` for schema `database:`, wire with `.add_backend` — no public crate feature required. Matrix row `acme-stub` in testkit/e2e/bench.

```bash
cargo test -p acme-valence-backend-stub
```

**Open first:** [`acme-valence-backend-stub/src/lib.rs`](acme-valence-backend-stub/src/lib.rs) (`ENGINE_ID`, `PRIMARY`, `impl DatabaseBackend`)

**Success:** crate tests pass; `engine_id()` returns `acme_stub`.

**Next step:** `cargo doc -p uf-valence-core --open` → `DatabaseBackend` port contract.

---

## Testkit / matrix fixtures

These crates are **not** day-to-day application hosts. They supply generated models with abstract engine ids (`hop_a`…`hop_d`) so `valence-testkit`, `valence-e2e`, and `valence-bench` can register any physical adapter pair or chain under stable keys. For a runnable hop demo, use [`cross-backend-model-host`](#cross-backend-model-host--mem--sqlite-hop).

### `hop-pair-model-host`

**Teaches:** Two-table hop fixture — Project on `hop_a`, Task on `hop_b` — for adapter-pair matrix rows.

```bash
cargo check -p hop-pair-model-host
```

**Open first:** [`hop-pair-model-host/src/lib.rs`](hop-pair-model-host/src/lib.rs) (`HOP_A`, `HOP_B`, `PROJECT_DB`, `TASK_DB`)

**Next step:** [`cross-backend-model-host`](#cross-backend-model-host--mem--sqlite-hop) for end-to-end navigation; testkit docs in [`valence-testkit/README.md`](../valence-testkit/README.md).

---

### `hop-chain-model-host`

**Teaches:** Four-table chain fixture — Org → Project → Task → Note on `hop_a`…`hop_d` — for nested inner-query hop E2E (bm-v25).

```bash
cargo check -p hop-chain-model-host
```

**Open first:** [`hop-chain-model-host/src/lib.rs`](hop-chain-model-host/src/lib.rs) (`HOP_A`…`HOP_D`, per-table `*_DB` evaluators)

**Next step:** [`cross-backend-model-host`](#cross-backend-model-host--mem--sqlite-hop) for runnable multi-backend hop; [`valence-e2e/README.md`](../valence-e2e/README.md) for matrix layout.

---

## Quick reference

| Rung | Command | Proves |
|------|---------|--------|
| `quickstart` | `cargo run -p uf-valence --example quickstart --features mem` | Schema + registry |
| `quickstart_sqlite` | `cargo run -p uf-valence --example quickstart_sqlite --features sqlite` | Durable embedded |
| `multi_backend` | `cargo run -p uf-valence --example multi_backend --features mem` | Two logical keys |
| Minimal DSL | `cargo test -p minimal-schema` | Macro expansion |
| Codegen | `cargo test -p codegen-host` | Generated `Model` |
| Product | `cargo test -p product-model-host -- --test-threads=1` | Connections + delete queue |
| Cross-backend | `cargo run -p cross-backend-model-host` | Mem↔sqlite hop |
| Admin | `cargo run -p admin-runtime-host` | QueryCore smoke |
| Surreal bootstrap | `cargo run -p embedded-bootstrap` | Inventory router |
| Custom engine | `cargo test -p acme-valence-backend-stub` | Third-party adapter |

Further reading: [`valence/README.md`](../valence/README.md#how-to-run-examples), `cargo doc -p uf-valence --open` (Getting started), [`CONTRIBUTING.md`](../CONTRIBUTING.md).
