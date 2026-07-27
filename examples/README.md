# Valence examples walkthrough

This ladder is the ordered path from “schema compiles” to “custom engine in the matrix.” Each crate is a teaching card: run it, open the files it names, then follow **Next step** to the next rung.

Quickstarts live under [`valence/examples/`](../valence/examples/) (the public crate, not this directory). Workspace hosts here prove codegen, product shapes, cross-backend hops, admin surfaces, host ports + privacy, live multi-remote routing, Surreal bootstrap, and third-party adapters.

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

**Next step:** workspace [`codegen-host`](#codegen-host--build-time-models) for typed `Model` CRUD, or [`surreal_rocksdb`](#surreal_rocksdb--on-disk-embedded) / [`surreal_remote`](#surreal_remote--wire-client) for the Surreal ladder.

---

### `surreal_rocksdb` — on-disk embedded

**Teaches:** Durable single-node Surreal via `EmbeddedEngine::RocksDb` and `connect_embedded_at_path` — same schema/backend contract as `surreal_embedded`, backed by disk instead of memory.

```bash
cargo run -p uf-valence --example surreal_rocksdb --features surreal-rocksdb
```

**Open first:** [`valence/examples/surreal_rocksdb.rs`](../valence/examples/surreal_rocksdb.rs)

**Success:** stdout prints `surreal_rocksdb: Surreal RocksDB backend registered at … (path: …)`.

**Next step:** [`surreal_remote`](#surreal_remote--wire-client) for a networked Surreal server, or [`embedded-bootstrap`](#embedded-bootstrap--surreal-inventory) for inventory-driven bootstrap.

---

### `surreal_remote` — wire client

**Teaches:** Connect to a remote SurrealDB server over WebSocket/HTTP via `Surreal<Any>` and `SurrealRemoteBackend` — the wire-client counterpart to embedded/RocksDB Surreal. Skips cleanly when `VALENCE_SURREAL_URL` is unset.

```bash
docker run --rm -d --name valence-surreal -p 8000:8000 \
  surrealdb/surrealdb:latest start --user root --pass root memory
VALENCE_SURREAL_URL=ws://127.0.0.1:8000/rpc \
  cargo run -p uf-valence --example surreal_remote --features surreal-remote
```

**Open first:** [`valence/examples/surreal_remote.rs`](../valence/examples/surreal_remote.rs)

**Success:** stdout prints `surreal_remote: Surreal remote backend registered at …`.

**Next step:** [`remote-multi-backend-host`](#remote-multi-backend-host--live-postgres--redis) for a live multi-engine host.

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

**Next step:** [`privacy-actor-ports`](#privacy-actor-ports--secrets--identity--endpoints--privacy) for the host-injectable ports, [`embedded-bootstrap`](#embedded-bootstrap--surreal-inventory) if you need Surreal embedded inventory, or [`acme-valence-backend-stub`](#acme-valence-backend-stub--custom-engine) for a custom adapter.

---

### `privacy-actor-ports` — secrets / identity / endpoints / privacy

**Teaches:** The three host-injectable ports (`SecretProvider`, `ActorFactory`, `DatabaseEndpointResolver`) wired on the builder, side by side with the schema `policies:` privacy contract — `Actor::User` / `Actor::System` / `Actor::Anonymous` deny/allow across read, create, and delete, including the "privacy rule beats ownership" case (owner denied, `System` allowed on a `SYSTEM_ONLY` delete policy).

```bash
cargo run -p privacy-actor-ports
```

**Open first:** [`privacy-actor-ports/src/main.rs`](privacy-actor-ports/src/main.rs)

**Success:** stdout walks through each port, then prints allow/deny pairs ending in `privacy-actor-ports: OK (ports wired, allow/deny both proven)`.

**Next step:** [`remote-multi-backend-host`](#remote-multi-backend-host--live-postgres--redis) to combine ports with live multi-engine routing, or `cargo doc -p uf-valence-core --open` → module `ports` for the full port contract table.

---

### `remote-multi-backend-host` — live Postgres + Redis

**Teaches:** Live multi-remote routing — `Project` on Postgres (`primary`), `Task` on Redis (`cache`) — two real network-backed engines behind one router, proven with an actual create + read round trip on each. Skips cleanly when `DATABASE_URL` / `VALENCE_REDIS_URL` are unset.

```bash
docker run --rm -d --name valence-postgres -p 5432:5432 -e POSTGRES_PASSWORD=postgres postgres:16
docker run --rm -d --name valence-redis -p 6379:6379 redis:7

DATABASE_URL=postgres://postgres:postgres@127.0.0.1:5432/postgres \
VALENCE_REDIS_URL=redis://127.0.0.1:6379 \
  cargo run -p remote-multi-backend-host
```

**Open first:** [`remote-multi-backend-host/src/main.rs`](remote-multi-backend-host/src/main.rs)

**Success:** stdout prints `remote-multi-backend-host: project=alpha (postgres) task=first task (redis)` then `… OK`.

**Next step:** [`cross-backend-model-host`](#cross-backend-model-host--mem--sqlite-hop) for the codegen/`Model` version of a cross-backend hop; [Docker one-liners](#docker-one-liners) below for Mongo too.

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

## Docker one-liners

Remote-engine examples skip cleanly when their URL env var is unset, so these are optional. Start whichever backend you need, export the URL, then run the matching example:

```bash
# Postgres — Project storage for remote-multi-backend-host, quickstart_postgres
docker run --rm -d --name valence-postgres -p 5432:5432 -e POSTGRES_PASSWORD=postgres postgres:16
export DATABASE_URL=postgres://postgres:postgres@127.0.0.1:5432/postgres

# Redis — Task/cache storage for remote-multi-backend-host, quickstart_redis
docker run --rm -d --name valence-redis -p 6379:6379 redis:7
export VALENCE_REDIS_URL=redis://127.0.0.1:6379

# MongoDB — quickstart_mongodb
docker run --rm -d --name valence-mongodb -p 27017:27017 mongo:7
export VALENCE_MONGODB_URI=mongodb://127.0.0.1:27017

# SurrealDB — surreal_remote
docker run --rm -d --name valence-surreal -p 8000:8000 \
  surrealdb/surrealdb:latest start --user root --pass root memory
export VALENCE_SURREAL_URL=ws://127.0.0.1:8000/rpc
```

Tear down with `docker rm -f valence-postgres valence-redis valence-mongodb valence-surreal`.

## Quick reference

| Rung | Command | Proves |
|------|---------|--------|
| `quickstart` | `cargo run -p uf-valence --example quickstart --features mem` | Schema + registry |
| `quickstart_sqlite` | `cargo run -p uf-valence --example quickstart_sqlite --features sqlite` | Durable embedded |
| `multi_backend` | `cargo run -p uf-valence --example multi_backend --features mem` | Two logical keys |
| `surreal_rocksdb` | `cargo run -p uf-valence --example surreal_rocksdb --features surreal-rocksdb` | On-disk embedded Surreal |
| `surreal_remote` | `cargo run -p uf-valence --example surreal_remote --features surreal-remote` | Wire-client Surreal |
| Minimal DSL | `cargo test -p minimal-schema` | Macro expansion |
| Codegen | `cargo test -p codegen-host` | Generated `Model` |
| Product | `cargo test -p product-model-host -- --test-threads=1` | Connections + delete queue |
| Cross-backend | `cargo run -p cross-backend-model-host` | Mem↔sqlite hop |
| Admin | `cargo run -p admin-runtime-host` | QueryCore smoke |
| Ports + privacy | `cargo run -p privacy-actor-ports` | Secrets/identity/endpoints + allow/deny |
| Remote multi-backend | `cargo run -p remote-multi-backend-host` | Live Postgres + Redis routing |
| Surreal bootstrap | `cargo run -p embedded-bootstrap` | Inventory router |
| Custom engine | `cargo test -p acme-valence-backend-stub` | Third-party adapter |

Further reading: [`valence/README.md`](../valence/README.md#how-to-run-examples), `cargo doc -p uf-valence --open` (Getting started), [`CONTRIBUTING.md`](../CONTRIBUTING.md).
