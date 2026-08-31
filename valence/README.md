# valence

Public crate re-exporting core, macros, and optional reference adapters — the primary dependency for Valence applications. Enable backend features explicitly (see below) and wire runtime storage with `Valence::builder()` at boot.

Overview and quickstart: [../README.md](../README.md).

**Source of truth:** `cargo doc -p uf-valence --open`

## Cargo features

| Feature | Enables |
|---------|---------|
| `mem` (default) | `valence-backend-mem` — [`InMemoryBackend`](../valence-backend-mem/src/lib.rs) |
| `sqlite` | `valence-backend-sqlite` embedded |
| `indradb` | `valence-backend-indradb` embedded graph |
| `surreal` | `valence-backend-surreal` embedded memory engine |
| `surreal-rocksdb` | On-disk embedded Surreal (RocksDB) |
| `surreal-remote` | Remote Surreal via WebSocket/HTTP |
| `surreal-inventory` | Discover logical DB names from linked `valence_schema!` |
| `surreal-connect-env` | `connect_embedded_from_env()` via `VALENCE_EMBEDDED_*` |
| `postgres` | `valence-backend-postgres` (`DATABASE_URL`) |
| `mongodb` | `valence-backend-mongodb` (`VALENCE_MONGODB_URI`) |
| `hybrid` | `valence-backend-hybrid` (IndraDB cache over a primary) |
| `redis` | `valence-backend-redis` (`VALENCE_REDIS_URL`) |
| `telemetry-console` | `valence-telemetry` re-export and stderr sink |

Enable backends explicitly when minimizing dependencies:

```toml
uf-valence = { git = "https://github.com/unified-field-dev/valence", package = "uf-valence", default-features = false, features = ["mem"] }
```

## How to run examples

**Walkthrough ladder:** [`examples/README.md`](../examples/README.md) — ordered path from quickstarts through workspace hosts to testkit fixtures.

Topology docs:
[Embedded](https://docs.rs/uf-valence/latest/valence/index.html#embedded-one-process) /
[Remote (wire)](https://docs.rs/uf-valence/latest/valence/index.html#remote-wire).

Valence is an in-process ORM: one host process owns the router. There is no coordinator/worker split. “Remote” means a wire client to an external database.

### 1. Embedded mem — `quickstart` (standalone)

```bash
cargo run -p uf-valence --example quickstart --features mem
```

Success: stdout prints `quickstart: schema … registered; Valence runtime ready`.

### 2. Durable embedded — `quickstart_sqlite` (standalone)

```bash
cargo run -p uf-valence --example quickstart_sqlite --features sqlite
```

### 3. Multi-backend routing — `multi_backend` (standalone)

One process, two logical mem backends + default key.

```bash
cargo run -p uf-valence --example multi_backend --features mem
```

### 4. Codegen → Model — workspace `codegen-host`

Typed `Model` impls need host `build.rs` + `valence-codegen` (not a single-file `[[example]]`).
Proof is the co-located test in `examples/codegen-host/src/lib.rs`.

```bash
cargo test -p codegen-host
```

### 5. Cross-backend hop + query — workspace `cross-backend-model-host`

One process, two backends (Project on mem, Task on sqlite): create rows, BelongsTo/HasMany hop, `Model::query`.

```bash
cargo run -p cross-backend-model-host
```

Success: stdout prints `cross-backend-model-host: Project(mem) ↔ Task(sqlite) hop + query OK (…)`.

### 6. Remote wire — `quickstart_postgres` (optional)

Start Postgres with Docker, set the shared URL, then run one example process (skips cleanly when unset):

```bash
docker run --rm -d --name valence-postgres -p 5432:5432 -e POSTGRES_PASSWORD=postgres postgres:16
export DATABASE_URL=postgres://postgres:postgres@127.0.0.1:5432/postgres
cargo run -p uf-valence --example quickstart_postgres --features postgres
```

MongoDB / Redis follow the same pattern with `VALENCE_MONGODB_URI` / `VALENCE_REDIS_URL` (see Other examples and the [Docker one-liners](../examples/README.md#docker-one-liners) in the examples walkthrough). For all four remote backends live at once, see workspace [`remote-multi-backend-host`](../examples/remote-multi-backend-host/) below.

### Other examples

| Example | Topology | Features | Notes |
|---------|----------|----------|-------|
| `hybrid_multi_logical` | Embedded | `hybrid,mem` | Hybrid primary under several logical names |
| `surreal_embedded` | Embedded | `surreal` | Surreal mem engine boot |
| `surreal_rocksdb` | Embedded (disk) | `surreal-rocksdb` | On-disk embedded Surreal via RocksDB |
| `surreal_remote` | Remote (wire) | `surreal-remote` | Requires `VALENCE_SURREAL_URL` |
| `quickstart_indradb` | Embedded | `indradb` | Graph backend boot |
| `quickstart_mongodb` | Remote (wire) | `mongodb` | Requires `VALENCE_MONGODB_URI` |
| `quickstart_redis` | Remote (wire) | `redis` | Requires `VALENCE_REDIS_URL` |
| `quickstart_telemetry` | Embedded | `mem,telemetry-console` | `ConsoleSink` port |

### Workspace host proofs (not crate `[[example]]`s)

See [`examples/README.md`](../examples/README.md) for the full ordered ladder.

| Crate | Role |
|-------|------|
| [`examples/minimal-schema`](../examples/minimal-schema/) | Compile-only `valence_schema!` |
| [`examples/codegen-host`](../examples/codegen-host/) | Codegen → generated `Model` |
| [`examples/product-model-host`](../examples/product-model-host/) | Product schemas / connections / deletion |
| [`examples/cross-backend-model-host`](../examples/cross-backend-model-host/) | Hop + query demo (path step 5) |
| [`examples/admin-runtime-host`](../examples/admin-runtime-host/) | Admin / `QueryCore` smoke |
| [`examples/privacy-actor-ports`](../examples/privacy-actor-ports/) | `SecretProvider` / `ActorFactory` / `DatabaseEndpointResolver` ports + privacy deny/allow |
| [`examples/remote-multi-backend-host`](../examples/remote-multi-backend-host/) | Live Postgres + Redis multi-remote routing |
| [`examples/embedded-bootstrap`](../examples/embedded-bootstrap/) | Surreal inventory bootstrap |
| [`examples/acme-valence-backend-stub`](../examples/acme-valence-backend-stub/) | Third-party `DatabaseBackend` checklist |
| [`examples/hop-pair-model-host`](../examples/hop-pair-model-host/) | Testkit fixture — adapter-pair hop models |
| [`examples/hop-chain-model-host`](../examples/hop-chain-model-host/) | Testkit fixture — four-table hop chain |

## Configuration

There is no config file or global settings loader. Integrators wire backends in code; optional env vars tune runtime behavior.

### Precedence (library)

1. **Cargo features** — choose which adapters and telemetry are linked (`default = ["mem"]`).
2. **Constructor / builder arguments** — [`ValenceBuilder`](../valence-core/src/runtime/builder.rs) methods at host boot.
3. **Struct `Default`** — omitted builder ports fall back to no-op providers.
4. **Environment variables** — read once at first use (see table below).

### Library environment variables

| Variable | Default | Effect |
|----------|---------|--------|
| `VALENCE_READ_CACHE` | on | Set `0` / `false` to disable the read-through LRU |
| `VALENCE_READ_CACHE_MAX` | `10000` | LRU capacity for point reads |
| `VALENCE_OWNERSHIP_COLOCATE` | on | Set `0` / `false` to disable ownership colocation |
| `VALENCE_OWNERSHIP_UNIFIED_FETCH` | on | Set `0` / `false` for legacy two-trip ownership reads |
| `VALENCE_OWNERSHIP_GET_JOIN` | off | Set `1` / `true` to join ownership on GET |
| `VALENCE_ENDPOINTS_JSON` | — | JSON map of logical name → physical URL |
| `VALENCE_ENDPOINT_<LOGICAL>` | — | Per-logical endpoint URL (name lowercased) |
| `VALENCE_EMBEDDED_ENGINE` | `rocksdb` | `mem` or `rocksdb` (feature `surreal-connect-env`) |
| `VALENCE_EMBEDDED_PATH` | `surreal/data` | RocksDB directory path |
| `VALENCE_NS` / `VALENCE_DB` | `prod` / `prod` | Surreal namespace and database |
| `VALENCE_DB_WALL_MS` | off | Set `1` / `true` to emit DB wall-time metrics |
| `VALENCE_DB_WALL_MS_SAMPLE` | `0` | Sample rate in `[0, 1]` when wall-ms mode is off |
| `VALENCE_SLOW_OP_MS` | — | Threshold for slow-op telemetry (milliseconds) |
| `DATABASE_URL` | — | Postgres adapter / `quickstart_postgres` |
| `VALENCE_MONGODB_URI` | — | MongoDB adapter / `quickstart_mongodb` |
| `VALENCE_REDIS_URL` | — | Redis adapter / `quickstart_redis` |

Surreal-specific wiring and bootstrap helpers: [`valence-backend-surreal/README.md`](../valence-backend-surreal/README.md).

Host ports (secrets, actor, endpoints, telemetry): `cargo doc -p uf-valence-core` → module `ports`.

Storage adapter contract: `cargo doc -p uf-valence-core` → `DatabaseBackend`; example `examples/acme-valence-backend-stub`.
