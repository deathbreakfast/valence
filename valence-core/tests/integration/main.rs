//! Integration tests for `uf-valence-core`.
//!
//! Each file under this directory is a module of one test binary, so the
//! crate's integration tests link once instead of once per file.
//! `data_use_catalog_double_install` stays a binary of its own: it installs
//! the process-wide data-use catalog, and `data_use_catalog_install` here
//! needs a process where nothing has installed one yet.

mod dag_privacy;
mod data_use_catalog_install;
mod defer_to_edge;
mod delete_now;
mod delete_side_effects;
mod deletion_set_null_apply;
mod port_extensions;
mod query_stats;
mod register_logical;
mod typed_field_roundtrip_mem;
