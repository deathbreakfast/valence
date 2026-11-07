//! Integration tests for `uf-valence-backend-mem`.
//!
//! Each file under this directory is a module of one test binary, so the
//! crate's integration tests link once instead of once per file.

mod backend_contract;
mod count_where_thing_eq;
mod retired_fields_ignored;
mod ttl_deferred_stamp;
mod two_record_predicate;
