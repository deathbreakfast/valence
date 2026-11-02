//! Graph edge junction table operations (SQL strings + param helpers).

use valence_core::record_id::RecordId;

/// Insert edge row SQL.
pub fn relate_edge_sql() -> &'static str {
    "INSERT OR IGNORE INTO valence_edges (from_table, from_id, edge_type, to_table, to_id) \
     VALUES (?, ?, ?, ?, ?)"
}

/// Delete edge row SQL.
pub fn unrelate_edge_sql() -> &'static str {
    "DELETE FROM valence_edges WHERE from_table = ? AND from_id = ? AND edge_type = ? \
     AND to_table = ? AND to_id = ?"
}

/// List edge targets SQL.
pub fn get_edge_targets_sql() -> &'static str {
    "SELECT to_table, to_id FROM valence_edges \
     WHERE from_table = ? AND from_id = ? AND edge_type = ?"
}

pub fn ensure_edges_table() -> String {
    super::document::ensure_edges_table_ddl()
}

pub fn relate_edge(from: &RecordId, edge_table: &str, to: &RecordId) -> (String, Vec<String>) {
    (
        relate_edge_sql().to_string(),
        vec![
            from.table().to_string(),
            from.id().to_string(),
            edge_table.to_string(),
            to.table().to_string(),
            to.id().to_string(),
        ],
    )
}

pub fn unrelate_edge(from: &RecordId, edge_table: &str, to: &RecordId) -> (String, Vec<String>) {
    (
        unrelate_edge_sql().to_string(),
        vec![
            from.table().to_string(),
            from.id().to_string(),
            edge_table.to_string(),
            to.table().to_string(),
            to.id().to_string(),
        ],
    )
}

pub fn get_edge_targets(from: &RecordId, edge_table: &str) -> (String, Vec<String>) {
    (
        get_edge_targets_sql().to_string(),
        vec![
            from.table().to_string(),
            from.id().to_string(),
            edge_table.to_string(),
        ],
    )
}

/// Keyset page over `valence_edges` (SQLite `?` placeholders).
///
/// Binds, in order: edge type filter (twice), cursor flag, then the five cursor columns, then limit.
pub fn scan_edges_sqlite_sql() -> &'static str {
    "SELECT from_table, from_id, edge_type, to_table, to_id FROM valence_edges \
     WHERE (? IS NULL OR edge_type = ?) \
     AND (? = 0 OR (from_table, from_id, edge_type, to_table, to_id) > (?, ?, ?, ?, ?)) \
     ORDER BY from_table, from_id, edge_type, to_table, to_id LIMIT ?"
}

/// Keyset page over `valence_edges` (Postgres `$n` placeholders).
pub fn scan_edges_postgres_sql() -> &'static str {
    "SELECT from_table, from_id, edge_type, to_table, to_id FROM valence_edges \
     WHERE ($1::text IS NULL OR edge_type = $1) \
     AND (NOT $2 OR (from_table, from_id, edge_type, to_table, to_id) > ($3, $4, $5, $6, $7)) \
     ORDER BY from_table, from_id, edge_type, to_table, to_id LIMIT $8"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_sql_orders_by_full_primary_key() {
        for sql in [scan_edges_sqlite_sql(), scan_edges_postgres_sql()] {
            assert!(sql.contains("ORDER BY from_table, from_id, edge_type, to_table, to_id"));
        }
    }
}
