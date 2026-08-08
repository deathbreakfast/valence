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
#[allow(missing_docs, clippy::missing_errors_doc)]
mod typed_table;

pub use document::{ensure_table, row_from_body, upsert_body_fields, EDGES_TABLE, ID_COLUMN};
pub use typed_table::{
    ensure_typed_table_postgres, ensure_typed_table_sqlite, inspect_typed_layout_postgres,
    inspect_typed_layout_sqlite, sync_typed_table_postgres, sync_typed_table_sqlite,
};
pub use edges::{ensure_edges_table, get_edge_targets, relate_edge, unrelate_edge};
pub use merge::json_merge;
pub use postgres_ops::{
    apply_ttl_policy_postgres, create_record_postgres, define_unique_index_postgres,
    delete_record_postgres, ensure_edges_postgres, ensure_table_postgres, execute_select_postgres,
    get_edge_sources_postgres, get_edge_targets_postgres, get_record_postgres,
    merge_record_postgres, relate_edge_postgres, unrelate_edge_postgres, update_record_postgres,
};
pub use query::{
    decode_select_rows, extract_ids, first_count, prepare_compiled, prepare_compiled_postgres,
    rewrite_value_id_unique_probe_for_document_sql, row_to_json,
};
pub use sqlite_ops::{
    apply_ttl_policy_sqlite, assert_safe_table, create_record_sqlite, define_unique_index_sqlite,
    delete_record_sqlite, ensure_edges_sqlite, ensure_table_sqlite, execute_select_sqlite,
    get_edge_sources_sqlite, get_edge_targets_sqlite, get_record_sqlite, merge_record_sqlite,
    relate_edge_sqlite, sql_capabilities, storage_id, ttl_deferred, unrelate_edge_sqlite,
    update_record_sqlite,
};
