# Documentation verification baseline

Re-run after test or CI changes.

## Commands

### Tests

```bash
# Full workspace tests
cargo test --workspace

# Security hardening (V-1..V-4) unit slice
cargo test -p uf-valence-core --lib privacy::bypass redact error::tests query::clamp_tests
cargo test -p uf-valence-core --lib privacy::evaluator::tests::test_filter_entity_fields

# Deletion DAG privacy + queued delete side-effect dispatch + SetNull/RemoveEdge apply
cargo test -p uf-valence-core --test dag_privacy --test delete_side_effects --test deletion_set_null_apply

# Host: requester actor + OnDelete (SetNull / RemoveEdge / cascade SE)
# cargo test -p valence-platform --test requester_actor --test public_contracts --test deletion_on_delete

# Matrix E2E (includes on-delete-* catalog; wire soft-skip)
cargo test -p valence-e2e
cargo test -p valence-e2e --test cross_backend_hops on_delete_hop

# Codegen / model runtime subset
cargo test -p valence-e2e --test admin_runtime_catalog
cargo test -p valence-e2e --test model_runtime_catalog
cargo test -p codegen-host -- --test-threads=1
cargo test -p product-model-host -- --test-threads=1

# Extended (RocksDB matrix)
export VALENCE_BENCH_ROCKSDB=1
cargo test -p valence-e2e --features surreal-rocksdb -- --ignored
```

### Examples and docs

Canonical path and full catalog: [`valence/README.md`](../valence/README.md#how-to-run-examples).

```bash
RUSTDOCFLAGS="-D warnings" cargo doc -p uf-valence --all-features --no-deps
cargo test --doc -p uf-valence-core -p uf-valence
cargo run -p uf-valence --example quickstart --features mem
cargo run -p uf-valence --example quickstart_sqlite --features sqlite
cargo run -p uf-valence --example multi_backend --features mem
cargo run -p cross-backend-model-host
```

## Line coverage (CI artifact)

PR CI runs a non-blocking [`coverage`](../.github/workflows/ci.yml) job with `cargo-llvm-cov`:

```bash
cargo install cargo-llvm-cov

cargo llvm-cov --workspace \
  --exclude valence-e2e --exclude valence-bench \
  --features mem,surreal-mem \
  --summary-only

cargo llvm-cov --workspace \
  --exclude valence-e2e --exclude valence-bench \
  --features mem,surreal-mem \
  --lcov --output-path lcov.info
```

Download `coverage-lcov` from GitHub Actions run artifacts for the CI report.

**Baseline (2026-07-08):** ~55% line coverage on the scoped workspace slice above (`mem,surreal-mem` features, excluding `valence-e2e` / `valence-bench`).

Extended tag CI: [`.github/workflows/ci-extended.yml`](../.github/workflows/ci-extended.yml).
