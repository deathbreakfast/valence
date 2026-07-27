# valence-testkit

Matrix bootstrap, [`DatabaseBackend`](../valence-core/src/backend/port.rs) port contract, and declarative scenario catalog for `valence-e2e` and `valence-bench`. Extend matrix dimensions, catalog scenarios, and backend contract checks here; wire `run_backend_contract` in adapter integration tests before adding matrix rows. Optional dev-dependency for integration tests using the same patterns as `valence-e2e`.

## Matrix dimensions

| Dimension | CI default variants |
|-----------|---------------------|
| **storage** | `Mem`, `SurrealMem`, `SurrealRocksdb`, `AcmeStub` |
| **telemetry** | `Off`, `Console`, `Recording` |
| **topology** | `Embedded` (`RemoteStub` = skip/ignore) |

## Key modules

| Module | Role |
|--------|------|
| `matrix.rs` | `MatrixSpec`, storage/telemetry/topology enums |
| `bootstrap/session.rs` | `BootstrapSession::spawn` → router + factory + optional `RecordingSink` |
| `backend_contract.rs` | `run_backend_contract` port suite |
| `catalog.rs` | Shared correctness catalog (7 scenarios) |
| `scenario.rs` / `runner.rs` | Declarative steps; `RunMode::Correctness` vs `Benchmark` |

## Features

| Feature | Enables |
|---------|---------|
| `surreal-mem` (default) | Embedded Surreal mem bootstrap |
| `surreal-rocksdb` | RocksDB matrix row (`VALENCE_BENCH_ROCKSDB=1`) |
| `surreal-inventory` | Inventory bootstrap scenario |
| `acme-stub` (default) | Third-party adapter matrix row |

## Hop capability matrix (0.1.x)

Cross-backend hop contracts in `hops/` assert **seed + BelongsTo/HasMany navigation** when backends are available. Nested `EXISTS` / connection predicates are **not asserted** on multi-engine layouts until the capability matrix expands — those paths log `SKIP nested_where_unsupported` with an explicit reason instead of soft-passing empty or false-positive results.

| Skip label | Meaning |
|------------|---------|
| `backend_unavailable` | Required adapter missing in this environment (or acme-stub excluded) |
| `nested_where_unsupported` | Nested EXISTS deliberately out of scope for this layout in 0.1.x |

This crate is **not published** (`publish = false`); it is workspace-internal support for e2e/bench.

## Verify

```bash
cargo test -p valence-testkit
```

See [docs/E2E_BENCH_COVERAGE.md](../docs/E2E_BENCH_COVERAGE.md) for the full matrix.
