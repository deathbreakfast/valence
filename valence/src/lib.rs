//! **Valence** is a schema-driven ORM for Rust: declare typed tables with
//! [`valence_schema!`], generate [`Model`] CRUD at build time, and wire storage through
//! [`Valence::builder`] without locking into one database.
//!
//! *Typed schemas and models with composable storage adapters.*
//!
//! # Features
//!
//! - **Schema DSL** — fields, connections, policies, ownership, TTL, and trait mixins
//!   ([`valence_schema!`], [`valence_trait_schema!`])
//! - **Build-time codegen** — typed models from host `schemas/` via `valence-codegen`
//! - **Composable backends** — in-memory, SQLite, IndraDB, SurrealDB, Postgres, MongoDB, Redis
//! - **Multi-backend routing** — one [`DatabaseRouter`]; each schema picks a backend with
//!   `database:` / [`DatabaseFromEngine`]
//! - **Host ports** — secrets, actor identity, endpoints, and telemetry injected at boot
//! - **Privacy-aware CRUD** — policy and ownership hooks on generated [`Model`] paths
//!
//! Enable backends with Cargo features (`mem` is the default). The crate `README.md` lists every
//! feature flag and environment variable.
//!
//! # Getting started
//!
//! Follow these steps in order. Each linked API page includes details for that task.
//!
//! ## 1. Choose and wire storage
//!
//! | Backend | Type | Feature | Topology | When to use |
//! |---------|------|---------|----------|-------------|
//! | In-memory | [`InMemoryBackend`] | `mem` (default) | embedded | Local experiments; tests |
//! | SQLite | [`SqliteBackend`] | `sqlite` | embedded | Durable single-host store |
//! | IndraDB | [`IndradbBackend`] | `indradb` | embedded | Graph-oriented workloads |
//! | SurrealDB | [`SurrealEmbeddedBackend`] | `surreal` | embedded | Surreal engine in-process |
//! | Postgres | [`PostgresBackend`] | `postgres` | remote | Wire Postgres (`DATABASE_URL`) |
//! | MongoDB | [`MongoBackend`] | `mongodb` | remote | Wire Mongo (`VALENCE_MONGODB_URI`) |
//! | Redis | [`RedisBackend`] | `redis` | remote | Wire Redis (`VALENCE_REDIS_URL`) |
//!
//! ### Select a backend in the schema
//!
//! A schema does not contain a backend instance. Its `database:` field points to a stable
//! [`DatabaseFromEngine`] evaluator. The evaluator combines:
//!
//! - a **logical name** (for example `"default"`) that must match
//!   [`ValenceBuilder::add_backend`], and
//! - an **engine ID** exported by the selected adapter.
//!
//! Define `COUNTER_DB` for the backend you enable:
//!
//! | Backend | `COUNTER_DB` declaration |
//! |---------|--------------------------|
//! | In-memory | `Database::from_engine("default", MEM_ENGINE_ID)` |
//! | SQLite | `Database::from_engine("default", SQLITE_ENGINE_ID)` |
//! | IndraDB | `Database::from_engine("default", INDRADB_ENGINE_ID)` |
//! | SurrealDB | `Database::from_engine("default", SURREAL_ENGINE_ID)` |
//! | Postgres | `Database::from_engine("default", POSTGRES_ENGINE_ID)` |
//! | MongoDB | `Database::from_engine("default", MONGODB_ENGINE_ID)` |
//! | Redis | `Database::from_engine("default", REDIS_ENGINE_ID)` |
//!
//! Then use that evaluator in the same Counter schema:
//!
//! ```ignore
//! use valence::{Database, DatabaseFromEngine, FieldType, valence_schema};
//!
//! // Choose the engine constant for the enabled backend.
//! pub const COUNTER_DB: DatabaseFromEngine =
//!     Database::from_engine("default", valence::MEM_ENGINE_ID);
//!
//! valence_schema! {
//!     Counter {
//!         table: "counter",
//!         version: "0.1.0",
//!         description: "Simple counter",
//!         database: COUNTER_DB,
//!         fields: [
//!             id: { r#type: FieldType::String, primary_key: true, required: true },
//!             value: { r#type: FieldType::Integer, required: true },
//!         ],
//!     }
//! }
//! ```
//!
//! Omitting `database:` selects [`DEFAULT_IN_MEMORY`] (`"default"` +
//! [`MEM_ENGINE_ID`]). If that router key is absent, the current runtime falls back to its
//! active/default backend. Declare `database:` explicitly for clear behavior and for any runtime
//! with multiple backends.
//!
//! **In-memory first run:**
//!
//! ```rust
//! # #[cfg(feature = "mem")]
//! use std::sync::Arc;
//! # #[cfg(feature = "mem")]
//! use valence::{
//!     Database, DatabaseFromEngine, FieldType, InMemoryBackend, Valence, MEM_ENGINE_ID,
//!     valence_schema,
//! };
//!
//! # #[cfg(feature = "mem")]
//! const COUNTER_DB: DatabaseFromEngine =
//!     Database::from_engine("default", MEM_ENGINE_ID);
//!
//! # #[cfg(feature = "mem")]
//! valence_schema! {
//!     Counter {
//!         table: "counter",
//!         version: "0.1.0",
//!         database: COUNTER_DB,
//!         fields: [
//!             id: { r#type: FieldType::String, primary_key: true, required: true },
//!             value: { r#type: FieldType::Integer, required: true },
//!         ],
//!     }
//! }
//!
//! # #[cfg(feature = "mem")]
//! # #[tokio::main]
//! # async fn main() -> valence::Result<()> {
//! # #[cfg(feature = "mem")]
//! let valence = Valence::builder()
//!     .add_backend("default", Arc::new(InMemoryBackend::new()))
//!     .build()?;
//! # #[cfg(feature = "mem")]
//! assert_eq!(valence.backend_for_table("counter")?.engine_id(), MEM_ENGINE_ID);
//! # #[cfg(feature = "mem")]
//! # Ok(())
//! # }
//! ```
//!
//! Runnable: `cargo run -p valence --example quickstart --features mem`
//!
//! ## 2. Declare schemas
//!
//! Schemas are the typed contracts Valence registers and (via codegen) turns into models.
//!
//! ```ignore
//! use valence::{
//!     Database, DatabaseFromEngine, FieldType, MEM_ENGINE_ID, valence_schema,
//! };
//!
//! const COUNTER_DB: DatabaseFromEngine =
//!     Database::from_engine("default", MEM_ENGINE_ID);
//!
//! valence_schema! {
//!     Counter {
//!         table: "counter",
//!         version: "0.1.0",
//!         description: "Simple counter",
//!         database: COUNTER_DB,
//!         fields: [
//!             id: { r#type: FieldType::String, primary_key: true, required: true },
//!             value: { r#type: FieldType::Integer, required: true },
//!         ],
//!     }
//! }
//! ```
//!
//! Replace `MEM_ENGINE_ID` with the engine constant from the table in step 1 when the Counter
//! belongs on another backend.
//!
//! See [`valence_schema!`] and [`valence_trait_schema!`] for the DSL field reference.
//! Minimal schema example: workspace `examples/minimal-schema`.
//! Macros and `valence-codegen` share one syn DSL parser (`valence-schema-dsl`), so
//! host `schemas/*_valence_schema.rs` files accept the same syntax and semantics
//! (including `database:` evaluators).
//!
//! ## 3. Set up build-time codegen
//!
//! Typed [`Model`] impls are **generated at compile time** from schema files under
//! `schemas/` (for example `widget_valence_schema.rs`). Add a build dependency and a
//! one-line `build.rs`:
//!
//! ```toml
//! [dependencies]
//! uf-valence = { git = "https://github.com/unified-field-dev/valence", package = "uf-valence", features = ["mem"] }
//!
//! [build-dependencies]
//! uf-valence-codegen = { git = "https://github.com/unified-field-dev/valence", package = "uf-valence-codegen" }
//! ```
//!
//! ```ignore
//! // build.rs
//! fn main() {
//!     valence_codegen::build().expect("valence codegen failed");
//! }
//! ```
//!
//! Include generated models (this is what must be linked for typed CRUD and inventory):
//!
//! ```ignore
//! valence::include_generated_models!();
//! ```
//!
//! Schema files under `schemas/` are **scan inputs** for codegen; they are not
//! `mod`-linked. End-to-end proof: workspace `examples/codegen-host` and
//! `examples/product-model-host`. See the
//! [valence-codegen](../valence_codegen/index.html) crate docs for custom roots via
//! `build_with` / `CodegenConfig`.
//!
//! ## 4. Use generated models (CRUD)
//!
//! After codegen, call [`Model`] methods with a [`Valence`] runtime:
//!
//! ```ignore
//! use valence::Model;
//!
//! // Widget is generated from schemas/widget_valence_schema.rs
//! let created = Widget::create(widget, &valence).await?;
//! let loaded = Widget::get(created.id(), &valence).await?;
//! Widget::update(created.id(), updated, &valence).await?;
//! Widget::delete(created.id(), &valence).await?;
//! ```
//!
//! Product-shaped schemas and connections: `examples/product-model-host`.
//!
//! ## 5. Route multiple backends
//!
//! One [`Valence`] holds a heterogeneous [`DatabaseRouter`]. Schema `database:` evaluators
//! pick the router key per table.
//!
//! ```rust,no_run
//! # #[cfg(feature = "mem")]
//! # async fn demo() -> valence::Result<()> {
//! use std::sync::Arc;
//! use valence::{InMemoryBackend, Valence, router_key, MEM_ENGINE_ID};
//!
//! let primary = router_key("primary", MEM_ENGINE_ID);
//! let valence = Valence::builder()
//!     .add_backend("primary", Arc::new(InMemoryBackend::new()))
//!     .add_backend("archive", Arc::new(InMemoryBackend::new()))
//!     .default_backend_key(primary)
//!     .build()?;
//! # let _ = valence;
//! # Ok(())
//! # }
//! ```
//!
//! Runnable: `cargo run -p valence --example multi_backend --features mem`
//!
//! ## 6. Inject host ports
//!
//! Optional builder methods wire secrets, actor identity, endpoints, and telemetry:
//! [`ValenceBuilder::secret_provider`], [`ValenceBuilder::actor_factory`],
//! [`ValenceBuilder::endpoint_resolver`], [`ValenceBuilder::telemetry_sink`].
//!
//! Port table and reference impls: [`valence_core::ports`]. Storage adapter contract and
//! third-party checklist: [`DatabaseBackend`]. Router semantics: [`DatabaseRouter`].
//!
//! ```rust
//! # #[cfg(feature = "mem")]
//! # fn demo() -> valence::Result<()> {
//! use std::sync::Arc;
//! use valence::{
//!     ConsoleSink, EnvSecretProvider, InMemoryBackend, JsonActorFactory, NoopEndpointResolver,
//!     Valence,
//! };
//!
//! let _valence = Valence::builder()
//!     .add_backend("default", Arc::new(InMemoryBackend::new()))
//!     .secret_provider(Arc::new(EnvSecretProvider))
//!     .actor_factory(Arc::new(JsonActorFactory))
//!     .endpoint_resolver(Arc::new(NoopEndpointResolver))
//!     .telemetry_sink(Arc::new(ConsoleSink))
//!     .build()?;
//! # Ok(())
//! # }
//! ```
//!
//! ## How the pieces link together
//!
//! ```text
//! schemas/*.rs ──► build.rs (valence_codegen::build) ──► $OUT_DIR/generated_models.rs
//!   (scan inputs)                                              │
//!                                                              │ include_generated_models!
//!                                                              ▼
//!                                              impl Model + inventory submit
//!                                                              │
//!                              SchemaRegistry ◄────────────────┤
//!                                                              ▼
//!                                              typed CRUD on Valence
//!                                                              │
//!                              Valence runtime ◄── DatabaseRouter / backends
//! ```
//!
//! **Dependency rules:** `valence-core` owns ports and runtime (no engine SDK);
//! `valence-backend-*` advertise open `ENGINE_ID`s; the public crate re-exports behind features;
//! apps own schema roots and call `valence-codegen` from `build.rs`; one operation stays on
//! one backend; host adapters inject at boot.
//!
//! # Next steps
//!
//! | Task | Start here |
//! |------|------------|
//! | Schema DSL fields | [`valence_schema!`], [`valence_trait_schema!`] |
//! | Build-time codegen | [valence-codegen](../valence_codegen/index.html), `examples/codegen-host` |
//! | Wire storage | [`Valence::builder()`], [`InMemoryBackend`] |
//! | Model CRUD | [`Model`], `examples/product-model-host` |
//! | Multi-backend routing | [`DatabaseRouter`], `multi_backend` example |
//! | Custom adapter | [`DatabaseBackend`], `examples/acme-valence-backend-stub` |
//! | SQLite | [`SqliteBackend`], `quickstart_sqlite` example |
//! | IndraDB | [`IndradbBackend`], `quickstart_indradb` example |
//! | Surreal embedded | [`SurrealEmbeddedBackend`], `surreal_embedded` example |
//! | Postgres | [`PostgresBackend`], `quickstart_postgres` (env-gated) |
//! | MongoDB | [`MongoBackend`], `quickstart_mongodb` (env-gated) |
//! | Redis | [`RedisBackend`], `quickstart_redis` (env-gated) |
//! | Admin runtime | [`SchemaRegistry`], [`QueryCore`], `examples/admin-runtime-host` |
//! | Config / env vars | crate [`README.md`](README.md) |
//!
//! # Entry points
//!
//! - [`prelude`] — ergonomic schema authoring imports
//! - [`Valence`] / [`ValenceBuilder`] — runtime assembly
//! - [`valence_schema!`] — schema DSL macro
//! - [`Model`] — generated CRUD surface
//! - [`DatabaseBackend`] / [`DatabaseRouter`] — storage ports
//!
//! # Prerequisites and gotchas
//!
//! - Enable backend features explicitly (`mem` is on by default).
//! - Product schemas and codegen roots belong in **your** application.
//! - Wire adapters (postgres/mongodb/redis) need live URLs; examples skip cleanly when unset.
//! - SurrealDB support lives in `valence-backend-surreal` (feature `surreal`), not in core ports.
//! - Generated models (or macro-expanded schemas) must be linked into the binary or
//!   `inventory` will not see them.
//!
//! # Runnable examples
//!
//! | Example | Features | Notes |
//! |---------|----------|-------|
//! | `quickstart` | `mem` | Schema + mem boot + registry proof |
//! | `multi_backend` | `mem` | Multiple logical backends |
//! | `quickstart_sqlite` | `sqlite` | Embedded SQLite |
//! | `quickstart_indradb` | `indradb` | Embedded IndraDB |
//! | `surreal_embedded` | `surreal` | Surreal mem engine |
//! | `quickstart_postgres` | `postgres` | Requires `DATABASE_URL` |
//! | `quickstart_mongodb` | `mongodb` | Requires `VALENCE_MONGODB_URI` |
//! | `quickstart_redis` | `redis` | Requires `VALENCE_REDIS_URL` |
//! | `quickstart_telemetry` | `mem,telemetry-console` | Console telemetry sink |
//!
//! ```bash
//! cargo run -p valence --example quickstart --features mem
//! cargo run -p valence --example quickstart_sqlite --features sqlite
//! cargo run -p valence --example quickstart_indradb --features indradb
//! cargo run -p valence --example surreal_embedded --features surreal
//! ```

extern crate self as valence;

mod include_generated;

pub use valence_core::*;
pub use valence_macros::*;

#[cfg(feature = "telemetry-console")]
pub use valence_telemetry::*;

#[cfg(feature = "mem")]
pub use valence_backend_mem::{
    install_default_mem_router, InMemoryBackend, ENGINE_ID as MEM_ENGINE_ID,
};

#[cfg(feature = "sqlite")]
pub use valence_backend_sqlite::{
    SqliteBackend, ENGINE_ID as SQLITE_ENGINE_ID, PRIMARY as SQLITE_PRIMARY,
};

#[cfg(feature = "postgres")]
pub use valence_backend_postgres::{
    PostgresBackend, ENGINE_ID as POSTGRES_ENGINE_ID, PRIMARY as POSTGRES_PRIMARY,
};

#[cfg(feature = "mongodb")]
pub use valence_backend_mongodb::{
    MongoBackend, ENGINE_ID as MONGODB_ENGINE_ID, PRIMARY as MONGODB_PRIMARY,
};

#[cfg(feature = "indradb")]
pub use valence_backend_indradb::{
    IndradbBackend, ENGINE_ID as INDRADB_ENGINE_ID, PRIMARY as INDRADB_PRIMARY,
};

#[cfg(feature = "hybrid")]
pub use valence_backend_hybrid::{
    CachePolicy, CacheRules, HybridBackend, HybridBackendBuilder, ENGINE_ID as HYBRID_ENGINE_ID,
    PRIMARY as HYBRID_PRIMARY,
};

#[cfg(feature = "redis")]
pub use valence_backend_redis::{
    RedisBackend, ENGINE_ID as REDIS_ENGINE_ID, PRIMARY as REDIS_PRIMARY,
};

#[cfg(feature = "surreal")]
pub use valence_backend_surreal::{
    bootstrap_embedded_router, connect_embedded_at_path, extract_id_from_record_display,
    extract_id_from_select_value, register_embedded_logical_names,
    register_embedded_logical_names_slices, shared_router_with_embedded_logical_names,
    surreal_record_id_for, EmbeddedEngine, RegisterEmbeddedLogicalNamesOptions, SDb,
    SurrealEmbeddedBackend, SurrealMemBackend, ENGINE_ID as SURREAL_ENGINE_ID,
};

#[cfg(all(feature = "surreal", feature = "surreal-inventory"))]
pub use valence_backend_surreal::{
    bootstrap_embedded_router_from_inventory, collect_distinct_embedded_surreal_logical_names,
    register_embedded_logical_names_from_inventory, DEFAULT_EMBEDDED_SURREAL_LOGICAL_NAMES,
};

#[cfg(all(feature = "surreal", feature = "surreal-connect-env"))]
pub use valence_backend_surreal::{
    connect_embedded_from_env, database_from_env, embedded_engine_from_env, embedded_path_from_env,
    namespace_from_env,
};

#[cfg(all(feature = "surreal", feature = "surreal-remote"))]
pub use valence_backend_surreal::SurrealRemoteBackend;

/// Hidden re-exports for generated model code and platform migrations.
#[doc(hidden)]
pub mod __internal {
    pub use valence_core::__internal::{CompiledQuery, QueryCompiler};
}

/// Ergonomic imports for schema authoring and generated models.
pub mod prelude {
    pub use crate::{
        valence_schema, valence_trait_schema, Cardinality, Currency, CurrencyCode, Database,
        DatabaseEvaluator, DatabaseFromEngine, FieldChange, FieldOperation, FieldType, IdHolder,
        JsonAsSerdeError, Model, Mutation, MutationKind, OnDelete, RecordId, Reference, Role,
        SideEffect, Validator, WithReference, DEFAULT_IN_MEMORY, DEFAULT_IN_MEMORY_ROUTER_KEY,
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn public_crate_reexports_core() {
        let _ = router_key("default", KnownEngines::INMEMORY_MEM);
    }

    #[cfg(feature = "mem")]
    #[tokio::test]
    async fn mem_feature_wires_backend() {
        let valence = Valence::builder()
            .add_backend("default", Arc::new(InMemoryBackend::new()))
            .build()
            .expect("build");
        assert_eq!(valence.active_backend().unwrap().engine_id(), MEM_ENGINE_ID);
    }

    #[cfg(feature = "surreal")]
    #[tokio::test]
    async fn surreal_feature_wires_backend() {
        use surrealdb::engine::local::Mem;

        let db = valence_backend_surreal::SDb::init();
        db.connect::<Mem>(()).await.expect("connect");
        db.use_ns("test").use_db("test").await.expect("ns");
        let valence = Valence::builder()
            .add_backend("default", Arc::new(SurrealEmbeddedBackend::new(db)))
            .build()
            .expect("build");
        assert_eq!(
            valence.active_backend().unwrap().engine_id(),
            SURREAL_ENGINE_ID
        );
    }
}
