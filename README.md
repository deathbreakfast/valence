# Valence

[![CI](https://github.com/unified-field-dev/valence/actions/workflows/ci.yml/badge.svg)](https://github.com/unified-field-dev/valence/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/uf-valence.svg)](https://crates.io/crates/uf-valence)
[![docs.rs](https://docs.rs/uf-valence/badge.svg)](https://docs.rs/uf-valence)
[![Downloads](https://img.shields.io/crates/d/uf-valence.svg)](https://crates.io/crates/uf-valence)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

[GitHub](https://github.com/unified-field-dev/valence) · [crates.io](https://crates.io/crates/uf-valence) · [docs.rs](https://docs.rs/uf-valence) · `cargo doc -p uf-valence --all-features --open`

**Valence** is a schema-driven ORM for Rust: privacy-aware models, composable storage adapters, and host-injectable ports. Wire backends at boot via **`Valence::builder()`** — trait ports in `valence-core`, engines in feature-gated `valence-backend-*` crates.

*Typed schemas and models without locking you into one database.*

**Status:** v0.1.1 · MIT · crates.io package **`uf-valence`** (the name `valence` is taken); Rust imports stay `use valence::…`.

Import `uf-valence` with the `mem` feature for a minimal embedded stack and follow rustdoc Getting started for a full walkthrough. Contributions: [CONTRIBUTING.md](CONTRIBUTING.md).

## Schema DSL

```rust
use std::sync::Arc;
use valence::{
    Database, DatabaseFromEngine, FieldType, InMemoryBackend, Valence, MEM_ENGINE_ID,
    valence_schema,
};

const COUNTER_DB: DatabaseFromEngine = Database::from_engine("default", MEM_ENGINE_ID);

valence_schema! {
    Counter {
        table: "counter",
        version: "0.1.0",
        description: "Simple counter",
        database: COUNTER_DB,
        fields: [
            id: { r#type: FieldType::String, primary_key: true, required: true },
            value: { r#type: FieldType::Integer, required: true },
        ],
    }
}

let valence = Valence::builder()
    .add_backend("default", Arc::new(InMemoryBackend::new()))
    .build()?;
```

`database:` selects a logical name plus engine. The logical name (`"default"`) must match
`.add_backend("default", …)`. Replace `MEM_ENGINE_ID` with the selected adapter’s engine constant
for SQLite, IndraDB, SurrealDB, Postgres, MongoDB, or Redis.

## Architecture

```mermaid
flowchart TD
  App["Your application"] --> Builder["ValenceBuilder"]
  Builder --> Core["valence-core ports"]
  Core --> MemBackend["mem backend"]
  Core --> SqliteBackend["sqlite / indradb"]
  Core --> SurrealBackend["surreal backend"]
  Core --> WireBackend["postgres / mongodb / redis"]
  Builder --> Ports["secrets / actor / endpoints / telemetry"]
  App -->|"build.rs"| Codegen["valence-codegen → Model"]
  Codegen --> App
```

Your application owns schemas, codegen roots, and business logic. Valence owns the ORM surface: schema registry, privacy/ownership hooks, query routing, and the `DatabaseBackend` port. Storage engines live in separate adapter crates.

**Dependency rules**

1. `valence-core` defines ports and model/runtime semantics; it contains no engine SDK.
2. `valence-backend-*` crates implement storage and advertise an open `ENGINE_ID`.
3. The `valence` public crate re-exports adapters behind Cargo features.
4. Applications own schema roots and invoke `valence-codegen` from `build.rs`.
5. One runtime may register heterogeneous backends; one operation stays on one backend.
6. Host-specific adapters remain outside core and are injected at boot.

## Quick start

Add the public crate with the in-memory backend for local evaluation. The crates.io
package is **`uf-valence`** (the name `valence` is taken); imports stay `use valence::…`:

```toml
[dependencies]
valence = { package = "uf-valence", version = "0.1.1", features = ["mem"] }
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
```

From git (same package name):

```toml
[dependencies]
valence = { package = "uf-valence", git = "https://github.com/deathbreakfast/valence", features = ["mem"] }
```

Published crates use the `uf-*` package names on crates.io (`uf-valence`, `uf-valence-core`, …). Rust imports remain `use valence::…`.

```bash
cargo run -p uf-valence --example quickstart --features mem
cargo doc -p uf-valence --open
```

Follow the numbered **Getting started** guide in `cargo doc -p uf-valence` (wire → schema → codegen → Model CRUD → routing).

## Cargo features

| Feature | Backend | Notes |
|---------|---------|-------|
| `mem` | `valence-backend-mem` | **Default** — quick start, non-durable |
| `sqlite` | `valence-backend-sqlite` | Durable embedded |
| `indradb` | `valence-backend-indradb` | Embedded graph |
| `hybrid` | `valence-backend-hybrid` | IndraDB cache over a primary |
| `surreal` | `valence-backend-surreal` | Embedded Surreal (mem engine) |
| `surreal-rocksdb` | Surreal RocksDB | On-disk embedded |
| `surreal-remote` | Surreal remote | WebSocket/HTTP |
| `surreal-inventory` | Surreal helpers | Discover logical DB names from schemas |
| `surreal-connect-env` | Surreal helpers | `VALENCE_EMBEDDED_*` bootstrap |
| `postgres` | `valence-backend-postgres` | Wire — requires `DATABASE_URL` |
| `mongodb` | `valence-backend-mongodb` | Wire — requires `VALENCE_MONGODB_URI` |
| `redis` | `valence-backend-redis` | Wire — requires `VALENCE_REDIS_URL` |
| `telemetry-console` | `valence-telemetry` | Stderr telemetry sink |

### Limitations (read before integrating)

| Capability | Status |
|------------|--------|
| Generated Model CRUD | Requires host `build.rs` + `valence-codegen` (see `examples/codegen-host`) |
| Wire backends (postgres/mongodb/redis) | Need a live server URL; examples skip when unset |
| Cross-backend transactions | **Not supported** — batch ops stay on one backend |
| Privacy / ownership / deletion DAG | Schema-driven; see `examples/product-model-host` and crate rustdoc (`Model`, ownership, deletion) |
| Third-party engines | Implement `DatabaseBackend` in a separate crate — no public crate change |

## Workspace crates

| Crate | Role |
|-------|------|
| `valence-core` | Traits, router, builder, Model, privacy |
| `valence-schema-dsl` | Shared syn DSL parser (macros + codegen) |
| `valence-macros` | `valence_schema!`, `valence_trait_schema!` |
| `valence-codegen` | Build-time model generation |
| `valence-backend-*` | Feature-gated storage adapters |
| `valence-telemetry` | `TelemetrySink` + reference sinks |
| `valence` | Public crate |
| `valence-testkit` / `valence-e2e` / `valence-bench` | Matrix verification and benchmarks |

## Documentation

| Doc | For |
|-----|-----|
| `cargo doc -p uf-valence --open` | Getting started, API map, examples |
| [`examples/README.md`](examples/README.md) | **Ordered walkthrough ladder** (quickstarts → workspace hosts → testkit fixtures) |
| [`valence/README.md`](valence/README.md) | Feature flags, configuration precedence, env vars |
| [`valence-macros/README.md`](valence-macros/README.md) | Schema DSL field reference |
| [`valence-codegen/README.md`](valence-codegen/README.md) | Build pipeline — model generation |
| [`examples/minimal-schema/`](examples/minimal-schema/) | Compile-only `valence_schema!` |
| [`examples/codegen-host/`](examples/codegen-host/) | End-to-end codegen → `Model` |
| [`examples/product-model-host/`](examples/product-model-host/) | Product schemas, connections, deletion queue |
| [`examples/cross-backend-model-host/`](examples/cross-backend-model-host/) | Mem↔sqlite hop + `Model::query` |
| [`examples/admin-runtime-host/`](examples/admin-runtime-host/) | Admin / `QueryCore` smoke |
| [`examples/embedded-bootstrap/`](examples/embedded-bootstrap/) | Surreal inventory bootstrap |
| [`examples/acme-valence-backend-stub/`](examples/acme-valence-backend-stub/) | Third-party adapter checklist |
| [`examples/hop-pair-model-host/`](examples/hop-pair-model-host/) | Testkit fixture — adapter-pair hop models |
| [`examples/hop-chain-model-host/`](examples/hop-chain-model-host/) | Testkit fixture — four-table hop chain |
| `cargo doc -p uf-valence-core --open` | `DatabaseBackend`, `ports`, `ValenceBuilder` |
| Per-crate READMEs | Backend- and crate-specific entry points |

### How to run examples

Full walkthrough ladder: [`examples/README.md`](examples/README.md).

Runbook details: [`valence/README.md`](valence/README.md#how-to-run-examples)
([Embedded](https://docs.rs/uf-valence/latest/valence/index.html#embedded-one-process) /
[Remote (wire)](https://docs.rs/uf-valence/latest/valence/index.html#remote-wire)).

```bash
# Canonical path
cargo run -p uf-valence --example quickstart --features mem
cargo run -p uf-valence --example quickstart_sqlite --features sqlite
cargo run -p uf-valence --example multi_backend --features mem
cargo test -p codegen-host
cargo run -p cross-backend-model-host

# Optional remote wire (skip when URL unset):
DATABASE_URL=postgres://… cargo run -p uf-valence --example quickstart_postgres --features postgres
```

### Maintainers

- [CONTRIBUTING.md](CONTRIBUTING.md) — verify commands and build guardrails before PRs
- [docs/VERIFICATION.md](docs/VERIFICATION.md) — test and coverage baseline
- [docs/E2E_BENCH_COVERAGE.md](docs/E2E_BENCH_COVERAGE.md) — feature × happy/sad/bench matrix
- [docs/AWS_E2E_BENCH_CAMPAIGN.md](docs/AWS_E2E_BENCH_CAMPAIGN.md) — AWS campaign runbook

## Composable storage (FAQ)

**Q: How do I add Postgres, Vault, or a custom engine?**  
Publish (or depend on) a crate that implements [`DatabaseBackend`](valence-core/src/backend/port.rs). Wire it with `.add_backend("logical", Arc::new(your_backend))` — no change to the `valence` crate features.

**Q: Can one `Valence` use multiple engines?**  
Yes. One [`DatabaseRouter`](valence-core/src/router.rs) holds heterogeneous backends. Schema `database:` evaluators pick the router key per table.

See `cargo doc -p uf-valence-core` (`DatabaseBackend`, `ports`) and `examples/acme-valence-backend-stub`.

## Development

```bash
export CARGO_BUILD_JOBS=1 CARGO_TARGET_DIR=target-valence
./scripts/gate.sh
cargo test -p uf-valence-core -p uf-valence-telemetry -p uf-valence-macros -p uf-valence-backend-mem
RUSTDOCFLAGS="-D warnings" cargo doc -p uf-valence --all-features --no-deps
```

## License

MIT — see [LICENSE](LICENSE).
