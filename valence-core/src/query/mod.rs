//! Composable query builder — filters, sorts, compiled execution.
//!
//! Entry type: [`QueryCore`]. [`QueryCore::execute`] post-filters rows by entity read privacy
//! and applies field-level [`PrivacyEvaluator::filter_entity_fields`](crate::privacy::PrivacyEvaluator::filter_entity_fields).
//! [`QueryCore::get_record_json`] is a raw storage fetch (no privacy).
//!
//! Query `limit` / `offset` are clamped to [`MAX_QUERY_LIMIT`] / [`MAX_QUERY_OFFSET`].

/// Maximum rows a single [`QueryCore::execute`] may return.
pub const MAX_QUERY_LIMIT: u32 = 1000;

/// Maximum query offset accepted by [`QueryCore`].
pub const MAX_QUERY_OFFSET: u32 = 1_000_000;

mod predicates;
#[cfg(any(
    feature = "compiler-sql",
    feature = "compiler-mongodb",
    feature = "compiler-redis",
    feature = "compiler-indradb",
))]
mod sql_document;
mod sql_helpers;
mod sql_row_filter;
mod types;

#[cfg(all(test, feature = "compiler-surreal"))]
mod sql_emit_tests;

pub use predicates::{
    DateTimePredicate, IdOnlyRecord, IntPredicate, NullPredicate, OrderBy, RecordPredicate,
    SortDirection, StringPredicate,
};
pub use sql_row_filter::{apply_equality_where, apply_order_limit_offset};
pub use types::{HopSource, HopType, QueryCore, WhereClause};

#[cfg(test)]
mod clamp_tests {
    use super::*;

    #[test]
    fn limit_and_offset_clamp_on_builder() {
        let q = QueryCore::new("t".into()).limit(u32::MAX).offset(u32::MAX);
        assert_eq!(q.limit, Some(MAX_QUERY_LIMIT));
        assert_eq!(q.offset, Some(MAX_QUERY_OFFSET));
    }
}
