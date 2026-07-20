//! Valence testkit — matrix bootstrap, backend contract, and scenario catalog.
//!
//! **Audience:** internal — CI maintainers and integration test authors.
//!
//! Internal test harness; not a stable public API.

#![allow(missing_docs)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::print_stderr
)]
// Step dispatch keeps a uniform `async fn` signature across modules; some arms
// are sync-only today.
#![allow(clippy::unused_async)]
// Harness code favors explicit early returns and `unwrap_or_else(|| …)` for
// readable skip/error paths over pedantic rewrites.
#![allow(clippy::needless_return)]
#![allow(clippy::unwrap_or_default)]
#![allow(clippy::implicit_clone)]
#![allow(clippy::enum_glob_use)]
// Internal test harness: Result APIs and fluent builders are not a published surface.
#![allow(
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::return_self_not_must_use,
    clippy::unnecessary_wraps
)]

mod admin_contract;
mod backend_contract;
mod bootstrap;
mod catalog;
mod deletion_capture;
mod deletion_contract;
mod fixtures;
mod harness_lock;
mod hop_contract;
mod hops;
mod matrix;
mod model_contract;
mod runner;
mod scenario;

#[cfg(feature = "surreal-mem")]
pub use admin_contract::run_admin_contract_surreal_mem;
pub use admin_contract::{run_admin_contract, run_admin_contract_for, run_admin_contract_mem};
pub use backend_contract::run_backend_contract;
pub use bootstrap::{BootstrapMode, BootstrapSession, WireBackendOptions};
pub use catalog::{
    catalog_for_storage, e2e_storage_backends, embedded_catalog, run_catalog_entry, CatalogEntry,
    PathKind,
};
#[cfg(feature = "surreal-mem")]
pub use deletion_contract::run_deletion_contract_surreal_mem;
pub use deletion_contract::{
    run_deletion_contract, run_deletion_contract_for, run_deletion_contract_mem,
};
pub use hop_contract::run_cross_backend_hop_contract;
pub use hops::{
    directed_pairs, hop_quads_representative, hop_storage_engines, hop_triples_representative,
    run_hop_chain_contract, run_hop_pair_contract, run_hop_quad_contract, HopPair, HopQuad,
    HopTriple,
};
pub use matrix::{
    all_storage_adapters, extended_store_available, extended_store_available_with_wire,
    extended_store_skip_reason, extended_store_skip_reason_with_wire, topology_available,
    topology_skip_reason, wire_backend_configured, CrossBackendLayout, MatrixSpec, StorageAdapter,
    TelemetryAdapter, Topology,
};
#[cfg(feature = "acme-stub")]
pub use model_contract::run_model_contract_acme_stub;
#[cfg(feature = "surreal-mem")]
pub use model_contract::run_model_contract_surreal_mem;
pub use model_contract::{
    backend_for_storage, run_model_contract, run_model_contract_for, run_model_contract_mem,
};
pub use runner::{RunMode, ScenarioResult, ScenarioRunner, StepTiming};
pub use scenario::{ScenarioSpec, ScenarioStep};

pub use valence_backend_mem::{
    install_default_mem_router, InMemoryBackend, ENGINE_ID as MEM_ENGINE_ID,
};

#[cfg(feature = "surreal-mem")]
pub use valence_backend_surreal::{
    bootstrap_embedded_router, connect_embedded_at_path, EmbeddedEngine,
    ENGINE_ID as SURREAL_ENGINE_ID,
};

#[cfg(feature = "acme-stub")]
pub use acme_valence_backend_stub::{AcmeStubBackend, ENGINE_ID as ACME_ENGINE_ID};
