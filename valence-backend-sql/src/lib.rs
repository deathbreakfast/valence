//! Shared SQL document storage helpers for Valence SQL backends.

#![deny(missing_docs)]

mod document;
#[allow(missing_docs)]
mod edges;
mod merge;
#[allow(missing_docs, clippy::missing_errors_doc)]
// internal ops; errors are Error::Database/Validation
mod postgres_ops;
mod query;
#[allow(missing_docs, clippy::missing_errors_doc)]
// internal ops; errors are Error::Database/Validation
mod sqlite_ops;

pub use document::{ensure_table, row_from_body, upsert_body_fields, EDGES_TABLE, ID_COLUMN};
pub use edges::{ensure_edges_table, get_edge_targets, relate_edge, unrelate_edge};
pub use merge::json_merge;
pub use postgres_ops::{
    create_record_postgres, define_unique_index_postgres, delete_record_postgres,
    ensure_edges_postgres, ensure_table_postgres, execute_select_postgres,
    get_edge_targets_postgres, get_record_postgres, merge_record_postgres, relate_edge_postgres,
    unrelate_edge_postgres, update_record_postgres,
};
pub use query::{
    decode_select_rows, extract_ids, first_count, prepare_compiled, prepare_compiled_postgres,
    row_to_json,
};
pub use sqlite_ops::{
    assert_safe_table, create_record_sqlite, define_unique_index_sqlite, delete_record_sqlite,
    ensure_edges_sqlite, ensure_table_sqlite, execute_select_sqlite, get_edge_targets_sqlite,
    get_record_sqlite, merge_record_sqlite, relate_edge_sqlite, sql_capabilities, storage_id,
    ttl_deferred, unrelate_edge_sqlite, update_record_sqlite,
};
