//! Integration tests for `uf-valence-backend-postgres`.
//!
//! Each file under this directory is a module of one test binary, so the
//! crate's integration tests link once instead of once per file.

mod backend_contract;
mod pending_lookup_large;
mod safe_tweak_sync;
mod scan_enumeration;
mod transfer_from_sqlite;
mod typed_null_binding;
mod typed_sync_add_field;
mod unique_violation_mapping;
