# valence-e2e

Matrix-driven integration tests exercising the shared `valence-testkit` correctness catalog.
Add catalog scenarios in `valence-testkit` and expand matrix tests here.

## Coverage contract

- **Y** means the scenario **executed** with validating assertions on that adapter.
- Soft-skip when wire env is unset is local convenience only — **not** coverage.
- Set `VALENCE_MATRIX_STRICT=1` (extended CI / AWS campaign) so missing wire env **fails**.

See [docs/E2E_BENCH_COVERAGE.md](../docs/E2E_BENCH_COVERAGE.md).

## Tests

| Test | Storage slice |
|------|---------------|
| `matrix_mem_embedded_catalog` | `mem` |
| `matrix_sqlite_catalog` | `sqlite` |
| `matrix_indradb_catalog` | `indradb` |
| `matrix_mongodb_catalog` | `mongodb` (URI; soft-skip unless STRICT) |
| `matrix_redis_catalog` | `redis` (URL; soft-skip unless STRICT) |
| `matrix_postgres_catalog` | `postgres` feature + `DATABASE_URL` |
| `matrix_hybrid_catalog` | `hybrid` feature + `DATABASE_URL` |
| `matrix_surreal_mem_catalog` | `surreal-mem` |
| `matrix_surreal_rocksdb_catalog` | `VALENCE_BENCH_ROCKSDB=1` + feature |
| `matrix_acme_stub_catalog` | `acme-stub` (subset) |
| `admin_runtime_*` / `model_runtime_*` | per-adapter contracts |
| `cross_backend_hops_*` | hop Cartesian (nested EXISTS = **X** in 0.1.x) |

The embedded catalog is the full `valence-testkit` list (50+ entries including typed-field / datetime query scenarios). Run with `--test-threads=1` because telemetry sink installation is process-global.

## Verify

```bash
export CARGO_TARGET_DIR=target-valence-e2e
# PR / local in-process
cargo test -p valence-e2e --test matrix_catalog -- --test-threads=1
cargo test -p valence-e2e --test cross_backend_hops --features cross-backend-hops -- --test-threads=1

# Extended / campaign (services + fail-not-skip)
export VALENCE_MATRIX_STRICT=1
export DATABASE_URL=postgres://…
export VALENCE_REDIS_URL=redis://…
export VALENCE_MONGODB_URI=mongodb://…
export VALENCE_BENCH_ROCKSDB=1
cargo test -p valence-e2e --features postgres,hybrid,surreal-rocksdb,cross-backend-hops -- --test-threads=1
```

See [docs/VERIFICATION.md](../docs/VERIFICATION.md).
