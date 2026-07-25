# valence

Public crate re-exporting core, macros, and optional reference adapters.

Overview and quickstart: [../README.md](../README.md).

**Source of truth:** `cargo doc -p uf-valence --open`

## Audience

| Reader | Use this crate for |
|--------|-------------------|
| **App developers** | Primary dependency; enable backend features explicitly |
| **Host integrators** | `Valence::builder()` and prelude re-exports |

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
| `redis` | `valence-backend-redis` (`VALENCE_REDIS_URL`) |
| `telemetry-console` | `valence-telemetry` re-export and stderr sink |

Enable backends explicitly when minimizing dependencies:

```toml
uf-valence = { git = "https://github.com/unified-field-dev/valence", package = "uf-valence", default-features = false, features = ["mem"] }
```

## Runnable examples

From the workspace root:

```bash
cargo run -p uf-valence --example quickstart --features mem
cargo run -p uf-valence --example multi_backend --features mem
cargo run -p uf-valence --example quickstart_sqlite --features sqlite
cargo run -p uf-valence --example quickstart_indradb --features indradb
cargo run -p uf-valence --example surreal_embedded --features surreal
cargo run -p uf-valence --example quickstart_telemetry --features mem,telemetry-console

# Wire (skip when URL unset):
DATABASE_URL=postgres://localhost/valence \
  cargo run -p uf-valence --example quickstart_postgres --features postgres
VALENCE_MONGODB_URI=mongodb://localhost:27017 \
  cargo run -p uf-valence --example quickstart_mongodb --features mongodb
VALENCE_REDIS_URL=redis://127.0.0.1:6379 \
  cargo run -p uf-valence --example quickstart_redis --features redis
```

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
