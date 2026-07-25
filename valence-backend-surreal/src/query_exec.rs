//! Shared compiled-query execution for embedded and remote Surreal clients.

use surrealdb::{Connection, Surreal};

use crate::error::db_err;
use crate::row_json::{
    decode_query_response_rows_to_json, decode_query_value_projection_rows_to_json,
    is_select_value_projection_query,
};
use valence_core::error::Result;

pub fn query_err_is_missing_table(msg: &str) -> bool {
    let m = msg.to_lowercase();
    m.contains("does not exist") && m.contains("table")
}

pub fn is_read_only_query(query: &str) -> bool {
    matches!(
        query.split_whitespace().next().map(|w| w.to_ascii_uppercase()),
        Some(ref w) if w == "SELECT" || w == "SHOW"
    )
}

/// Reject mutating SurrealQL before it reaches the engine (mirrors SQL `ensure_read_only`).
///
/// # Errors
///
/// Returns [`valence_core::error::Error::Internal`] when the statement is not `SELECT`/`SHOW`.
pub fn ensure_read_only(query: &str) -> Result<()> {
    if is_read_only_query(query) {
        Ok(())
    } else {
        Err(valence_core::error::Error::Internal(format!(
            "unsupported SurrealQL in execute_compiled_query (read-only SELECT/SHOW only): {query}"
        )))
    }
}

#[cfg(test)]
mod read_only_tests {
    use super::{ensure_read_only, is_read_only_query};

    #[test]
    fn classifies_select_and_show() {
        assert!(is_read_only_query("SELECT * FROM t"));
        assert!(is_read_only_query("show tables"));
        assert!(!is_read_only_query("DELETE FROM t"));
        assert!(!is_read_only_query("UPDATE t SET x = 1"));
    }

    #[test]
    fn ensure_read_only_rejects_writes() {
        assert!(ensure_read_only("SELECT * FROM t").is_ok());
        let err = ensure_read_only("DELETE FROM t").expect_err("reject");
        assert!(err.to_string().contains("read-only"));
    }
}

pub async fn execute_compiled_query_inner<C>(
    db: &Surreal<C>,
    query: &str,
    params: &[(String, serde_json::Value)],
) -> Result<Vec<serde_json::Value>>
where
    C: Connection,
{
    use surrealdb::types::Value as SurrealValueType;

    ensure_read_only(query)?;

    let mut response = if params.is_empty() {
        match db.query(query).await {
            Ok(r) => r,
            Err(e) if is_read_only_query(query) && query_err_is_missing_table(&e.to_string()) => {
                return Ok(vec![]);
            }
            Err(e) => return Err(db_err(e)),
        }
    } else {
        let mut bindings = std::collections::HashMap::new();
        for (key, value) in params {
            bindings.insert(key.clone(), value.clone());
        }
        match db.query(query).bind(bindings).await {
            Ok(r) => r,
            Err(e) if is_read_only_query(query) && query_err_is_missing_table(&e.to_string()) => {
                return Ok(vec![]);
            }
            Err(e) => return Err(db_err(e)),
        }
    };
    let result: SurrealValueType = match response.take(0) {
        Ok(r) => r,
        Err(e) if is_read_only_query(query) && query_err_is_missing_table(&e.to_string()) => {
            return Ok(vec![]);
        }
        Err(e) => return Err(db_err(e)),
    };
    if is_select_value_projection_query(query) {
        decode_query_value_projection_rows_to_json(result)
    } else {
        decode_query_response_rows_to_json(db, result).await
    }
}
