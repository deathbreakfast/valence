//! Identifier validation and platform table exclusions for deletion graphs.

use crate::error::{Error, Result};

/// Tables excluded from automatic cascade expansion (platform internals).
pub const SKIP_DELETION_GRAPH_TABLES: &[&str] = &[
    "valence_data_ownership",
    "valence_ownership_transfer",
    "valence_deletion_run",
    "valence_deletion_step",
    "valence_deletion_error",
    "valence_iter_run",
    "valence_iter_batch",
    "valence_iter_row_error",
];

pub fn skip_graph_table(name: &str) -> bool {
    SKIP_DELETION_GRAPH_TABLES.contains(&name)
}

/// When `true`, [`QueryCore::execute`](crate::query::QueryCore::execute) should not post-filter rows by ownership `pending_deletion`.
#[inline]
pub fn table_skips_pending_deletion_filter(table: &str) -> bool {
    skip_graph_table(table)
}

/// # Errors
///
/// Returns an error when the requested operation cannot be completed.
pub fn assert_safe_ident(s: &str) -> Result<()> {
    if s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        Ok(())
    } else {
        Err(Error::Validation(format!(
            "unsafe identifier for deletion graph query: {s:?}"
        )))
    }
}

/// Record id string passed to `type::record($tb, $rid)` (bound parameter).
/// # Errors
///
/// Returns an error when the requested operation cannot be completed.
pub fn assert_safe_bare_thing_id(s: &str) -> Result<()> {
    if s.is_empty() {
        return Err(Error::Validation(
            "empty bare record id for deletion graph".to_string(),
        ));
    }
    if s.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == ':')
    {
        Ok(())
    } else {
        Err(Error::Validation(format!(
            "unsafe bare record id for deletion graph query: {s:?}"
        )))
    }
}

#[cfg(test)]
mod safe_id_tests {
    use super::{assert_safe_bare_thing_id, assert_safe_ident};

    #[test]
    fn bare_thing_id_accepts_uuid_with_hyphens() {
        assert_safe_bare_thing_id("17510ba7-3fdf-4d29-b3af-c27be5340acd").unwrap();
    }

    #[test]
    fn bare_thing_id_accepts_composite_segment_with_colons() {
        assert_safe_bare_thing_id("default-deployment:wiztop:0").unwrap();
    }

    #[test]
    fn strict_ident_rejects_uuid_in_table_slot() {
        assert!(assert_safe_ident("17510ba7-3fdf-4d29-b3af-c27be5340acd").is_err());
    }
}
