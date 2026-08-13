//! Physical schema-version stamp helpers (desired version from the in-process registry).

use crate::error::{Error, Result};
use crate::schema::SchemaRegistry;

/// SQL catalog table that stores last-applied DSL schema versions.
pub const SCHEMA_META_TABLE: &str = "valence_schema_meta";

/// Desired schema version string for `table` from [`SchemaRegistry`].
///
/// # Errors
///
/// Returns [`Error::Validation`] when the table is not registered.
pub fn desired_schema_version(table: &str) -> Result<&'static str> {
    SchemaRegistry::global()
        .get_schema(table)
        .map(|m| m.version)
        .ok_or_else(|| Error::Validation(format!("no registry schema for table {table}")))
}

/// Whether a physical stamp matches the registry desired version (exact string equality).
#[must_use]
pub fn version_stamp_matches(stamp: Option<&str>, desired: &str) -> bool {
    stamp == Some(desired)
}
