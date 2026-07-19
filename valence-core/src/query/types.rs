//! [`QueryCore`], [`WhereClause`], and hop metadata — the structured IR behind generated query builders.
//!
//! Inherent methods live in `impl_query_core.rs` via [`include!`] so this file remains the defining
//! module for [`QueryCore`] (Rust requires inherent `impl` in the same module as the type).

use crate::entity::ValenceEntity;
use crate::error::{Error, Result};
use crate::row_json::thing_to_id_only;
use crate::runtime::Valence;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::collections::BTreeMap;

use super::predicates::*;
use super::sql_helpers::*;

/// Internal representation of a WHERE clause.
#[derive(Debug, Clone)]
pub enum WhereClause {
    Int(String, IntPredicate),
    String(String, StringPredicate),
    DateTime(String, DateTimePredicate),
    Record(String, RecordPredicate),
    Null(String, NullPredicate),
    /// Subquery-exists: filter where a connected record matches the subquery.
    ConnectionExists {
        from_field: String,
        to_table: String,
        subquery: Box<QueryCore>,
    },
    /// Subquery-exists (reverse): for HasMany, filter where target table has rows with reverse_field = self.id.
    ConnectionExistsReverse {
        to_table: String,
        reverse_field: String,
        subquery: Box<QueryCore>,
    },
    /// Subquery-exists (ManyToMany): filter where edge table links self to target rows matching subquery.
    ConnectionExistsManyToMany {
        edge_table: String,
        to_table: String,
        subquery: Box<QueryCore>,
    },
    /// Direct edge membership check for ManyToMany: `self -> edge_table -> target`.
    ConnectionContainsManyToMany {
        edge_table: String,
        target: crate::RecordId,
    },
    /// A hop condition folded into where_clauses by union_with/join_with.
    Hop(HopSource),
}

/// Describes how a hop connects a source query to a target query.
#[derive(Debug, Clone)]
pub enum HopType {
    /// HasOne hop: source is the FK side; outer `SELECT` is the target.
    HasOneForward { fk_field: String },
    /// HasMany hop: source is the parent; outer `SELECT` is the child.
    HasManyForward { reverse_field: String },
    /// ManyToMany via edge table.
    ManyToManyForward { edge_table: String },
}

/// A previous query in the hop chain. Compiled at execution time.
#[derive(Debug, Clone)]
pub struct HopSource {
    /// The source query that produced IDs for this hop.
    pub source_query: Box<QueryCore>,
    /// How the source connects to the target.
    pub hop_type: HopType,
}

/// Core query builder that compiles per registered engine.
#[derive(Debug, Clone)]
pub struct QueryCore {
    pub table: String,
    pub model_type: Option<String>,
    pub projection: Option<Vec<String>>,
    pub where_clauses: Vec<WhereClause>,
    pub order_by: Vec<OrderBy>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
    pub search_fields: Vec<String>,
    pub search_term: Option<String>,
    /// Previous hop in the chain. None for root queries, Some for hopped queries.
    pub hop_source: Option<HopSource>,
    /// OR-combined clause groups for union queries.
    pub or_groups: Vec<Vec<WhereClause>>,
    /// GROUP BY fields for distinct/aggregation queries.
    pub group_by: Vec<String>,
}

include!("impl_query_core_builder_construct.rs");
include!("impl_query_core_builder_filter.rs");
include!("impl_query_core_builder_window.rs");
include!("impl_query_core_builder_compose.rs");
include!("impl_query_core_builder_execute.rs");

// Query compilers share parameter-key allocation; dialect emitters are feature-gated so
// default (SQL-only) and Surreal-only builds stay free of dead_code warnings.
#[cfg(any(
    feature = "compiler-surreal",
    feature = "compiler-sql",
    feature = "compiler-mongodb",
    feature = "compiler-redis",
    feature = "compiler-indradb",
))]
include!("impl_query_core_sql_shared.rs");

#[cfg(feature = "compiler-surreal")]
include!("impl_query_core_sql_predicates.rs");
#[cfg(feature = "compiler-surreal")]
include!("impl_query_core_sql_connections.rs");
#[cfg(feature = "compiler-surreal")]
include!("impl_query_core_sql_clauses.rs");
#[cfg(feature = "compiler-surreal")]
include!("impl_query_core_sql_emit_surreal.rs");

#[cfg(any(
    feature = "compiler-sql",
    feature = "compiler-mongodb",
    feature = "compiler-redis",
    feature = "compiler-indradb",
))]
include!("impl_query_core_sql_connections_sql.rs");
#[cfg(any(
    feature = "compiler-sql",
    feature = "compiler-mongodb",
    feature = "compiler-redis",
    feature = "compiler-indradb",
))]
include!("impl_query_core_sql_emit.rs");
#[cfg(any(
    feature = "compiler-sql",
    feature = "compiler-mongodb",
    feature = "compiler-redis",
    feature = "compiler-indradb",
))]
include!("impl_query_core_sql_emit_sql.rs");

include!("impl_query_core_fetch.rs");
