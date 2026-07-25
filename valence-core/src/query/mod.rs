//! Composable query builder — filters, sorts, compiled execution.
//!
//! Entry type: [`QueryCore`]. [`QueryCore::execute`] post-filters rows by entity read privacy.
//! [`QueryCore::get_record_json`] is a raw storage fetch (no privacy).
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
