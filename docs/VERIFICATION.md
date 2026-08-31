# Documentation verification baseline

Re-run after test or CI changes.

## PR CI parity (run before push / opening a PR)

Matches [`.github/workflows/ci.yml`](../.github/workflows/ci.yml) job **`quality-gates`**
on `main`. Re-run after workflow changes.

| CI step | Local command |
|--------|----------------|
| Gate script | `bash scripts/gate.sh` |
| Format | `cargo fmt --all -- --check` |
| Clippy | `cargo clippy --workspace --all-targets -- -D warnings -A missing-docs` |
| Deny rustc warnings (isolated crates) | See ci.yml `for pkg in …` loop with `RUSTFLAGS="-D warnings"` |
| Rustdoc | `RUSTDOCFLAGS="-D warnings" cargo doc -p uf-valence --all-features --no-deps` then `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --exclude uf-valence --no-deps` |
| Doctests | `cargo test --doc -p uf-valence-core -p uf-valence-backend-mem -p uf-valence-telemetry -p uf-valence` |
| Test workspace | `cargo test --workspace -- --test-threads=1` |
| Dependency hygiene | `cargo machete` |

Environment (match CI):

```bash
export CARGO_TARGET_DIR=target-valence
export CARGO_BUILD_JOBS=1
export CARGO_INCREMENTAL=0
```

Skip extended jobs (`package-dry-run`, `examples`, `e2e`, `coverage`, `bench-smoke`,
`codegen-runtime`, `core-skeleton`) unless you are running a maintainer campaign.

## Commands

### Tests

```bash
# Typed storage layout + SQLite sync
cargo test -p uf-valence-core --lib storage_layout
cargo test -p uf-valence-backend-sqlite --test backend_contract --test typed_sync_add_field --test schema_version_sync
# Postgres safe tweaks + concurrent schema growth (skips without DATABASE_URL)
cargo test -p uf-valence-backend-postgres --test safe_tweak_sync --test typed_sync_add_field

# Full workspace tests
cargo test --workspace

# Security hardening (V-1..V-4) unit slice
cargo test -p uf-valence-core --lib privacy::bypass redact error::tests query::clamp_tests
cargo test -p uf-valence-core --lib privacy::evaluator::tests::test_filter_entity_fields

# Deletion DAG privacy + queued delete side-effect dispatch + SetNull/RemoveEdge apply
cargo test -p uf-valence-core --test dag_privacy --test delete_side_effects --test deletion_set_null_apply

# Host: requester actor + OnDelete (SetNull / RemoveEdge / cascade SE)
# cargo test -p valence-platform --test requester_actor --test public_contracts --test deletion_on_delete

# Matrix E2E (includes typed-field / on-delete-* catalog)
# Soft-skip wire adapters unless VALENCE_MATRIX_STRICT=1
cargo test -p valence-e2e --test matrix_catalog -- --test-threads=1
cargo test -p valence-e2e --test cross_backend_hops --features cross-backend-hops -- --test-threads=1

# Strict wire matrix (Postgres / Redis / Mongo services required)
# export VALENCE_MATRIX_STRICT=1
# export DATABASE_URL=postgres://valence:valence@127.0.0.1:5432/valence
# export VALENCE_REDIS_URL=redis://127.0.0.1:6379
# export VALENCE_MONGODB_URI=mongodb://127.0.0.1:27017
# export VALENCE_BENCH_ROCKSDB=1
# cargo test -p valence-e2e --features postgres,hybrid,surreal-rocksdb,cross-backend-hops -- --test-threads=1

# valence-bench local smoke (not AWS capacity numbers)
cargo test -p valence-bench -- --test-threads=1
# cargo run -p valence-bench -- run --experiment bm-v29 --storage sqlite \
#   --duration-secs 2 --concurrency 2 --prefill 128
# VALENCE_BENCH_CLIENT_INDEX=0 cargo run -p valence-bench -- run --experiment bm-v30 --storage sqlite \
#   --duration-secs 2 --concurrency 2 --prefill 128

# Hybrid port contract (mem primary)
cargo test -p uf-valence-backend-hybrid --test backend_contract -- --test-threads=1

# Full AWS campaign (operator shell with cloud env loaded — see uf-live-cloud-lab)
# cd ../../uf-live-cloud-lab/valence
# ./scripts/aws-e2e-bench.sh --dry-run
# ./infra/aws/campaign/provision.sh && … deploy-and-run e2e/bench …
# After typed-storage upgrade: wipe still drops Postgres schema / Mongo DB / Redis.

# Codegen / model runtime subset
cargo test -p valence-e2e --test admin_runtime_catalog
cargo test -p valence-e2e --test model_runtime_catalog
cargo test -p codegen-host -- --test-threads=1
cargo test -p product-model-host -- --test-threads=1

# Extended (RocksDB + STRICT — see .github/workflows/ci-extended.yml)
export VALENCE_BENCH_ROCKSDB=1
cargo test -p valence-e2e --features surreal-rocksdb -- --test-threads=1
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

## PERFORMANCE / EXPERIMENTS — defer_to_edge parent fetch

`defer_to_edge` loads each parent row under a System actor so privacy can evaluate
the edge hop. That is one parent fetch per history (or other deferred) row in a
list today. Batching or caching those parent loads is deferred: correctness and
fail-closed ACL come first. Revisit if list latency or DB load becomes a product
issue; until then treat the per-row parent fetch as an accepted cost with no
extra VERIFICATION gate.

Extended tag CI: [`.github/workflows/ci-extended.yml`](../.github/workflows/ci-extended.yml).
