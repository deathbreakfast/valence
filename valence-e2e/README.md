# valence-e2e

Matrix-driven integration tests exercising the shared `valence-testkit` correctness catalog. Add catalog scenarios in `valence-testkit` and expand matrix tests here; useful reference when expanding host-side adapter matrix coverage after a git pin.

## Tests

| Test | Storage slice |
|------|---------------|
| `matrix_mem_embedded_catalog` | `mem` |
| `matrix_surreal_mem_catalog` | `surreal-mem` (feature `surreal-mem`) |
| `matrix_surreal_rocksdb_catalog` | `#[ignore]` unless `VALENCE_BENCH_ROCKSDB=1` |
| `matrix_acme_stub_catalog` | `acme-stub` |
| `matrix_remote_stub_skipped` | `#[ignore]` remote topology stub |
| `admin_runtime_*` | mem + surreal-mem admin contract |
| `model_runtime_*` | mem + surreal-mem + acme-stub model contract |

The embedded catalog now includes **15 scenarios** (7 original happy paths, 3 sad paths, 5 feature smokes). Sad-path entries use negative assertion steps (`AssertRouterResolveFails`, `AssertGetMissing`, `AssertPrivacyReadDenied`). Run with `--test-threads=1` because telemetry sink installation is process-global.

Inventory scenario uses `tests/support/` schema fixture when `surreal-inventory` feature is enabled.

## Verify

```bash
export CARGO_TARGET_DIR=target-valence-e2e
cargo test -p valence-e2e -- --test-threads=1
```

See [docs/E2E_BENCH_COVERAGE.md](../docs/E2E_BENCH_COVERAGE.md) and [docs/VERIFICATION.md](../docs/VERIFICATION.md).
