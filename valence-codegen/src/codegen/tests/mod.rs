//! Codegen integration tests: DSL fixtures, snapshot assertions, connection/trait/composite coverage.

#![allow(clippy::unwrap_used, clippy::expect_used)]
mod composite_key;
mod connections;
mod emit_parity;
mod ownership_hooks;
mod record_history_source_codegen;
mod schema_generation;
mod support;
mod traits;
mod validation;
