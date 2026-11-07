//! Integration tests for `uf-valence-backend-surreal` (needs `embedded-mem`).
//!
//! Each file under this directory is a module of one test binary, so the
//! crate's integration tests link once instead of once per file.

mod backend_contract;
#[cfg(feature = "inventory")]
mod bootstrap_inventory;
mod compiled_query;
mod embedded_crud;
mod graph_edges;
#[cfg(feature = "embedded-rocksdb")]
mod rocksdb_persistence;
mod typed_field_roundtrip;
