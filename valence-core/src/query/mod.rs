//! Admin/query builder — composable filters, sorts, and compiled SQL execution.
//!
//! Entry type: [`QueryCore`]. Predicate helpers include [`StringPredicate`] and [`IntPredicate`].
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
