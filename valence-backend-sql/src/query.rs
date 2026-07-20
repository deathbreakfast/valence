//! Execute compiled SQL queries and map rows to JSON.

use serde_json::{Map, Value};
use valence_core::compiled_query::CompiledQuery;
use valence_core::error::{Error, Result};

/// Bind `$param_key` placeholders in SQL to `?` for SQLite positional binding.
pub fn sql_with_positional_placeholders(
    query: &str,
    params: &[(String, Value)],
) -> (String, Vec<Value>) {
    let mut out = String::with_capacity(query.len());
    let mut values = Vec::new();
    let mut rest = query;
    while let Some(dollar) = rest.find('$') {
        out.push_str(&rest[..dollar]);
        rest = &rest[dollar + 1..];
        let key_len = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .count();
        let key = &rest[..key_len];
        rest = &rest[key_len..];
        if let Some((_, value)) = params.iter().find(|(k, _)| k == key) {
            out.push('?');
            values.push(value.clone());
        } else {
            out.push('$');
            out.push_str(key);
        }
    }
    out.push_str(rest);
    (out, values)
}

/// Decode a SQL row `(id, body)` into Valence JSON record shape.
///
/// # Errors
///
/// Currently infallible; returns `Ok` with an empty object body if JSON parse fails.
pub fn row_to_json(table: &str, id: &str, body_text: &str) -> Result<Value> {
    let body: Value = serde_json::from_str(body_text).unwrap_or_else(|_| Value::Object(Map::new()));
    Ok(super::document::row_from_body(table, id, body))
}

/// Parse SELECT results from generic JSON rows returned by driver layer.
///
/// # Errors
///
/// Currently infallible; malformed row shapes are skipped or passed through.
pub fn decode_select_rows(rows: Vec<Value>, default_table: &str) -> Result<Vec<Value>> {
    let mut out = Vec::new();
    for row in rows {
        if let Some(obj) = row.as_object() {
            if let (Some(id), Some(body)) = (obj.get("id"), obj.get("body")) {
                let id_str = id.as_str().unwrap_or_default();
                let body_val = if let Some(s) = body.as_str() {
                    serde_json::from_str(s).unwrap_or_else(|_| Value::Object(Map::new()))
                } else {
                    body.clone()
                };
                out.push(super::document::row_from_body(
                    default_table,
                    id_str,
                    body_val,
                ));
                continue;
            }
        }
        out.push(row);
    }
    Ok(out)
}

/// Extract count from first row.
pub fn first_count(rows: &[Value]) -> i64 {
    rows.first()
        .and_then(|v| {
            v.as_i64()
                .or_else(|| v.get("count").and_then(|c| c.as_i64()))
                .or_else(|| {
                    v.as_f64().map(|f| {
                        #[allow(clippy::cast_possible_truncation)] // count aggregation
                        {
                            f as i64
                        }
                    })
                })
        })
        .unwrap_or(0)
}

/// Extract id strings from SELECT id queries.
pub fn extract_ids(rows: &[Value]) -> Vec<String> {
    rows.iter()
        .filter_map(|v| {
            v.as_str()
                .map(str::to_string)
                .or_else(|| v.get("id").and_then(|id| id.as_str().map(str::to_string)))
        })
        .collect()
}

/// Validate compiled query is read-only SELECT.
///
/// # Errors
///
/// Returns [`Error::Internal`] when the query is not a `SELECT`.
pub fn ensure_read_only(query: &str) -> Result<()> {
    let upper = query.trim().to_uppercase();
    if upper.starts_with("SELECT ") {
        Ok(())
    } else {
        Err(Error::Internal(format!(
            "unsupported SQL in execute_compiled_query: {query}"
        )))
    }
}

/// Normalize compiled query for SQLite execution (`?` placeholders).
///
/// # Errors
///
/// Returns [`Error::Internal`] when the compiled query is not read-only.
pub fn prepare_compiled(compiled: &CompiledQuery) -> Result<(String, Vec<Value>)> {
    ensure_read_only(&compiled.query_string)?;
    Ok(sql_with_positional_placeholders(
        &compiled.query_string,
        &compiled.params,
    ))
}

/// Translate SQLite-style `json_extract(expr, '$.a.b')` into Postgres jsonb operators.
pub fn rewrite_json_extract_for_postgres(sql: &str) -> String {
    let mut out = String::with_capacity(sql.len());
    let mut rest = sql;
    while let Some(start) = rest.find("json_extract(") {
        out.push_str(&rest[..start]);
        rest = &rest[start + "json_extract(".len()..];
        let Some(comma) = rest.find(',') else {
            out.push_str("json_extract(");
            break;
        };
        let expr = rest[..comma].trim();
        rest = rest[comma + 1..].trim_start();
        let path = if let Some(stripped) = rest.strip_prefix("'$.") {
            let Some(end_q) = stripped.find('\'') else {
                out.push_str("json_extract(");
                out.push_str(expr);
                out.push_str(", ");
                break;
            };
            let path = &stripped[..end_q];
            rest = stripped[end_q + 1..].trim_start();
            if let Some(r) = rest.strip_prefix(')') {
                rest = r;
            }
            path
        } else {
            out.push_str("json_extract(");
            out.push_str(expr);
            out.push_str(", ");
            continue;
        };
        let parts: Vec<&str> = path.split('.').filter(|p| !p.is_empty()).collect();
        if parts.len() <= 1 {
            let seg = parts.first().copied().unwrap_or("");
            out.push_str(&format!("({expr}->>'{seg}')"));
        } else {
            out.push_str(&format!("({expr}#>>'{{{}}}')", parts.join(",")));
        }
    }
    out.push_str(rest);
    out
}

/// Normalize compiled query for Postgres execution (`$1`, `$2`, … placeholders).
///
/// # Errors
///
/// Returns [`Error::Internal`] when the compiled query is not read-only.
pub fn prepare_compiled_postgres(compiled: &CompiledQuery) -> Result<(String, Vec<Value>)> {
    ensure_read_only(&compiled.query_string)?;
    let rewritten = rewrite_json_extract_for_postgres(&compiled.query_string);
    Ok(sql_with_postgres_placeholders(&rewritten, &compiled.params))
}

/// Bind `$param_key` placeholders to Postgres numbered params.
pub fn sql_with_postgres_placeholders(
    query: &str,
    params: &[(String, Value)],
) -> (String, Vec<Value>) {
    let mut out = String::with_capacity(query.len());
    let mut values = Vec::new();
    let mut rest = query;
    let mut idx = 1usize;
    while let Some(dollar) = rest.find('$') {
        out.push_str(&rest[..dollar]);
        rest = &rest[dollar + 1..];
        let key_len = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .count();
        let key = &rest[..key_len];
        rest = &rest[key_len..];
        if let Some((_, value)) = params.iter().find(|(k, _)| k == key) {
            out.push_str(&format!("${idx}"));
            values.push(value.clone());
            idx += 1;
        } else if key.chars().all(|c| c.is_ascii_digit()) {
            // Already positional ($1) — keep as-is.
            out.push('$');
            out.push_str(key);
        } else {
            out.push('$');
            out.push_str(key);
        }
    }
    out.push_str(rest);
    (out, values)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]

    use super::*;
    use serde_json::json;

    #[test]
    fn positional_preserves_json_path_and_binds_params() {
        let q = "SELECT id, body FROM task WHERE (json_extract(body, '$.project') = $param_0 OR json_extract(body, '$.project') = $param_1 OR json_extract(body, '$.project.id') = $param_1)";
        let params = vec![
            ("param_0".into(), json!("project:mem-1")),
            ("param_1".into(), json!("mem-1")),
        ];
        let (sql, values) = sql_with_positional_placeholders(q, &params);
        assert_eq!(
            sql,
            "SELECT id, body FROM task WHERE (json_extract(body, '$.project') = ? OR json_extract(body, '$.project') = ? OR json_extract(body, '$.project.id') = ?)"
        );
        assert_eq!(values.len(), 3);
        assert_eq!(values[0], json!("project:mem-1"));
        assert_eq!(values[1], json!("mem-1"));
        assert_eq!(values[2], json!("mem-1"));
    }

    #[test]
    fn postgres_rewrites_json_extract_paths() {
        let q = "SELECT id FROM task WHERE json_extract(body, '$.project') = $p OR json_extract(t.body, '$.project.id') = $p";
        let out = rewrite_json_extract_for_postgres(q);
        assert_eq!(
            out,
            "SELECT id FROM task WHERE (body->>'project') = $p OR (t.body#>>'{project,id}') = $p"
        );
    }

    #[test]
    fn prepare_compiled_postgres_rewrites_json_extract() {
        let compiled = CompiledQuery {
            query_string: "SELECT id, body FROM project WHERE json_extract(body, '$.name') = $param_0 LIMIT 10".into(),
            params: vec![("param_0".into(), json!("alpha"))],
        };
        let (sql, values) = prepare_compiled_postgres(&compiled).expect("prepare");
        assert!(
            !sql.contains("json_extract"),
            "raw json_extract must be rewritten: {sql}"
        );
        assert!(sql.contains("body->>'name'") || sql.contains("(body->>'name')"));
        assert_eq!(values, vec![json!("alpha")]);
    }
}
