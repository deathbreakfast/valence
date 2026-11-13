//! Integration tests for `uf-valence-backend-sqlite`.
//!
//! Each file under this directory is a module of one test binary, so the
//! crate's integration tests link once instead of once per file.

mod backend_contract;
mod large_scan;
mod memory_connection_recycle;
mod query_stats;
mod scan_enumeration;
mod schema_version_sync;
mod transfer;
mod typed_sync_add_field;
mod typed_sync_retired_field;
mod unique_violation_mapping;
