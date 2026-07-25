# Contributing to Valence

Thank you for improving Valence. Before opening a PR, run the verification block below on a constrained host (`CARGO_BUILD_JOBS=1`).

## Development setup

1. Clone [unified-field-dev/valence](https://github.com/unified-field-dev/valence)
2. Install Rust stable
3. From the repository root:

```bash
export CARGO_BUILD_JOBS=1
export CARGO_TARGET_DIR=target-valence

./scripts/gate.sh
cargo fmt --all -- --check
cargo test -p uf-valence-core -p uf-valence-telemetry -p uf-valence-macros -p uf-valence-backend-mem
cargo check -p uf-valence --no-default-features
cargo check -p uf-valence --features mem,telemetry-console
RUSTDOCFLAGS="-D warnings" cargo doc -p uf-valence --all-features --no-deps
cargo package -p uf-valence --allow-dirty --no-verify
```

Run Cargo commands sequentially.

Published crates use the `uf-*` package names on crates.io (for example `uf-valence`). Rust import paths remain `valence`, `valence_core`, and so on. Internal crates `valence-testkit`, `valence-e2e`, and `valence-bench` are not published.

## Code of conduct

Participation is governed by [`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md). Security reports: [`SECURITY.md`](SECURITY.md).

## Verify (expanded)

```bash
export CARGO_BUILD_JOBS=1 CARGO_TARGET_DIR=target-valence

./scripts/gate.sh
cargo fmt --all -- --check

cargo test -p uf-valence-core -p uf-valence-macros -p uf-valence-codegen \
           -p uf-valence-backend-mem -p uf-valence-telemetry -p uf-valence

# Public crate docs need --all-features so backend re-exports resolve in Getting started.
RUSTDOCFLAGS="-D warnings" cargo doc -p uf-valence --all-features --no-deps

# Doctests (stable; no nightly)
cargo test --doc -p uf-valence-core
cargo test --doc -p uf-valence-backend-mem
cargo test --doc -p uf-valence-telemetry
cargo test --doc -p uf-valence

# Examples (offline-friendly)
cargo run -p uf-valence --example quickstart --features mem
cargo run -p uf-valence --example multi_backend --features mem
cargo run -p uf-valence --example quickstart_sqlite --features sqlite
cargo run -p uf-valence --example quickstart_indradb --features indradb
cargo run -p uf-valence --example surreal_embedded --features surreal
cargo run -p uf-valence --example quickstart_telemetry --features mem,telemetry-console

# Wire backends when a live service is available:
# DATABASE_URL=… cargo run -p uf-valence --example quickstart_postgres --features postgres
# VALENCE_MONGODB_URI=… cargo run -p uf-valence --example quickstart_mongodb --features mongodb
# VALENCE_REDIS_URL=… cargo run -p uf-valence --example quickstart_redis --features redis
```

See [`docs/VERIFICATION.md`](docs/VERIFICATION.md) for E2E / extended Surreal coverage.

## Pull requests

- Prefer small, focused PRs.
- Scope narrowly: `-p uf-valence-core`, `-p uf-valence`, etc. — avoid `--workspace` unless requested.
- Storage, secrets, telemetry, identity, and endpoint adapters implement ports from `valence-core` / `valence-telemetry` and are wired via `ValenceBuilder` at host boot. See `cargo doc -p uf-valence-core` module `ports`.
