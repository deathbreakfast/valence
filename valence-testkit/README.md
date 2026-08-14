# valence-testkit

Matrix bootstrap, [`DatabaseBackend`](../valence-core/src/backend/port.rs) port contract, and declarative scenario catalog for `valence-e2e` and `valence-bench`.

## Coverage contract

Every capability-applicable catalog scenario must run on every `StorageAdapter`. Soft-skip without `VALENCE_MATRIX_STRICT` is for local convenience only. Under `VALENCE_MATRIX_STRICT=1`, unavailable wire adapters **panic**.

Capability **X** cells (AcmeStub model runtime, Indra TTL Unsupported, Redis/Mongo iter scan, cross-backend nested EXISTS) stay documented skips — not silent passes.

## Matrix dimensions

| Dimension | Always-on (PR) | Wire (extended / STRICT) |
|-----------|----------------|--------------------------|
| **storage** | Mem, Sqlite, IndraDb, SurrealMem, AcmeStub | Postgres, Redis, MongoDb, HybridIndraPg, SurrealRocksdb |
| **telemetry** | Off, Console, Recording | same |
| **topology** | Embedded | Embedded |

## Key modules

| Module | Role |
|--------|------|
| `matrix.rs` | `MatrixSpec`, `matrix_strict()`, storage enums |
| `bootstrap/session.rs` | `BootstrapSession::spawn` |
| `backend_contract.rs` | `run_backend_contract` (asserts SELECT datetime numbers when seeded) |
| `catalog.rs` | Full correctness catalog (includes `typed-field-roundtrip`, `query-filter-datetime*`) |
| `scenario.rs` / `runner.rs` | Declarative steps |

## Features

| Feature | Enables |
|---------|---------|
| `sqlite` / `mongodb` / `indradb` / `redis` (default) | Always-on or soft-skip wire rows |
| `postgres` / `hybrid` | Opt-in wire adapters |
| `surreal-mem` (default) | Embedded Surreal mem |
| `surreal-rocksdb` | RocksDB (`VALENCE_BENCH_ROCKSDB=1`) |
| `acme-stub` (default) | Stub port subset |

## Hop capability matrix (0.1.x)

Cross-backend hop contracts assert **seed + BelongsTo/HasMany navigation**. Nested `EXISTS` is **X** (skipped with `nested_where_unsupported`) — do not claim Y in coverage docs.

| Skip label | Meaning |
|------------|---------|
| `backend_unavailable` | Required adapter missing (or acme-stub excluded) |
| `nested_where_unsupported` | Nested EXISTS deliberately X for multi-engine layouts |

## Verify

```bash
cargo test -p valence-testkit -- --test-threads=1
cargo test -p uf-valence-backend-hybrid --test backend_contract -- --test-threads=1
```
