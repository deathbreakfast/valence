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

/// Rewrite Surreal-shaped `SELECT VALUE id FROM …` probes to plain SQL `SELECT id`.
///
/// Typed SQL stores fields as real columns, so `WHERE {field} = $value` is valid as-is.
pub fn rewrite_value_id_unique_probe_for_document_sql(sql: &str) -> String {
    const PREFIX_LEN: usize = "SELECT VALUE id FROM ".len();
    let trimmed = sql.trim();
    if trimmed.len() < PREFIX_LEN
        || !trimmed[..PREFIX_LEN].eq_ignore_ascii_case("SELECT VALUE id FROM ")
    {
        return sql.to_string();
    }
    let rest = trimmed[PREFIX_LEN..].trim_start();
    format!("SELECT id FROM {rest}")
}

/// Normalize compiled query for SQLite execution (`?` placeholders).
///
/// # Errors
///
/// Returns [`Error::Internal`] when the compiled query is not read-only.
pub fn prepare_compiled(compiled: &CompiledQuery) -> Result<(String, Vec<Value>)> {
    ensure_read_only(&compiled.query_string)?;
    let rewritten = rewrite_value_id_unique_probe_for_document_sql(&compiled.query_string);
    Ok(sql_with_positional_placeholders(
        &rewritten,
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

/// Postgres reserved words that a Valence table could be named after. Unquoted,
/// `FROM user` reads `current_user` and `FROM order` is a syntax error.
const POSTGRES_RESERVED_TABLE_WORDS: &[&str] = &[
    "all",
    "analyse",
    "analyze",
    "and",
    "any",
    "array",
    "as",
    "asc",
    "asymmetric",
    "authorization",
    "binary",
    "both",
    "case",
    "cast",
    "check",
    "collate",
    "collation",
    "column",
    "concurrently",
    "constraint",
    "create",
    "cross",
    "current_catalog",
    "current_date",
    "current_role",
    "current_schema",
    "current_time",
    "current_timestamp",
    "current_user",
    "default",
    "deferrable",
    "desc",
    "distinct",
    "do",
    "else",
    "end",
    "except",
    "false",
    "fetch",
    "for",
    "foreign",
    "freeze",
    "from",
    "full",
    "grant",
    "group",
    "having",
    "ilike",
    "in",
    "initially",
    "inner",
    "intersect",
    "into",
    "is",
    "isnull",
    "join",
    "lateral",
    "leading",
    "left",
    "like",
    "limit",
    "localtime",
    "localtimestamp",
    "natural",
    "not",
    "notnull",
    "null",
    "offset",
    "on",
    "only",
    "or",
    "order",
    "outer",
    "overlaps",
    "placing",
    "primary",
    "references",
    "returning",
    "right",
    "select",
    "session_user",
    "similar",
    "some",
    "symmetric",
    "system_user",
    "table",
    "tablesample",
    "then",
    "to",
    "trailing",
    "true",
    "union",
    "unique",
    "user",
    "using",
    "variadic",
    "verbose",
    "when",
    "where",
    "window",
    "with",
];

/// Reserved words Postgres evaluates as values (`user` is `CURRENT_USER`). A bare
/// column with one of these names compares against the session value instead of the
/// column and silently matches nothing, so they are quoted anywhere they appear.
const POSTGRES_VALUE_KEYWORDS: &[&str] = &[
    "current_catalog",
    "current_date",
    "current_role",
    "current_schema",
    "current_time",
    "current_timestamp",
    "current_user",
    "localtime",
    "localtimestamp",
    "session_user",
    "system_user",
    "user",
];

/// Reserved words that are literals rather than names, so never quoted as columns.
const POSTGRES_LITERAL_WORDS: &[&str] = &["null", "true", "false", "default"];

/// Keywords that follow a column in a predicate (`group IS NULL`, `group IN (...)`).
const POSTGRES_PREDICATE_WORDS: &[&str] = &["is", "in", "like", "ilike", "between"];

/// Double-quote reserved-word identifiers the compiled-query emitter writes bare.
///
/// The emitter writes table and column names bare, which is fine for SQLite but not
/// for Postgres when the table is `user`, `order`, `group`, and so on, or when a
/// column has one of those names. Reserved words that follow `FROM` or `JOIN` are
/// quoted as tables. Value keywords such as `user` are quoted in any position.
/// Other reserved words are quoted where a column goes: before a comparison
/// operator or `IS`/`IN`/`LIKE`/`ILIKE`/`BETWEEN`, or after `BY`. Words already
/// qualified (`.`, `"`, `$`) or called as a function are left alone. Only bare
/// lowercase words are touched, and nothing inside a single-quoted literal is
/// rewritten.
#[must_use]
pub fn quote_reserved_tables_for_postgres(sql: &str) -> String {
    fn is_ident_byte(b: u8) -> bool {
        b.is_ascii_alphanumeric() || b == b'_'
    }
    fn is_qualified(bytes: &[u8], start: usize) -> bool {
        start > 0 && matches!(bytes[start - 1], b'.' | b'"' | b'$')
    }
    fn next_token(bytes: &[u8], end: usize) -> &[u8] {
        let rest = &bytes[end..];
        let skip = rest
            .iter()
            .position(|b| !b.is_ascii_whitespace())
            .unwrap_or(rest.len());
        &rest[skip..]
    }
    fn is_bare_value_keyword(bytes: &[u8], start: usize, end: usize, word: &str) -> bool {
        POSTGRES_VALUE_KEYWORDS.contains(&word)
            && !is_qualified(bytes, start)
            && next_token(bytes, end).first() != Some(&b'(')
    }
    fn is_bare_reserved_column(
        bytes: &[u8],
        start: usize,
        end: usize,
        word: &str,
        after_by: bool,
    ) -> bool {
        if !POSTGRES_RESERVED_TABLE_WORDS.contains(&word)
            || POSTGRES_LITERAL_WORDS.contains(&word)
            || is_qualified(bytes, start)
        {
            return false;
        }
        let next = next_token(bytes, end);
        let next_word_len = next
            .iter()
            .position(|b| !is_ident_byte(*b))
            .unwrap_or(next.len());
        let next_word = std::str::from_utf8(&next[..next_word_len]).unwrap_or("");
        after_by
            || matches!(next.first(), Some(b'=' | b'<' | b'>' | b'!'))
            || POSTGRES_PREDICATE_WORDS
                .iter()
                .any(|w| next_word.eq_ignore_ascii_case(w))
    }
    let bytes = sql.as_bytes();
    let mut out = String::with_capacity(sql.len() + 8);
    let mut i = 0;
    let mut in_literal = false;
    let mut after_table_keyword = false;
    // Inside an `ORDER BY` / `GROUP BY` list, each item after `BY` or `,` is a column.
    let mut in_by_list = false;
    let mut expect_by_column = false;
    while i < bytes.len() {
        let b = bytes[i];
        if in_literal {
            out.push(b as char);
            if b == b'\'' {
                in_literal = false;
            }
            i += 1;
            continue;
        }
        if b == b'\'' {
            in_literal = true;
            after_table_keyword = false;
            in_by_list = false;
            expect_by_column = false;
            out.push('\'');
            i += 1;
            continue;
        }
        if !is_ident_byte(b) {
            if !b.is_ascii_whitespace() {
                after_table_keyword = false;
                expect_by_column = b == b',' && in_by_list;
                in_by_list = expect_by_column;
            }
            let ch_len = sql[i..].chars().next().map_or(1, char::len_utf8);
            out.push_str(&sql[i..i + ch_len]);
            i += ch_len;
            continue;
        }
        let start = i;
        while i < bytes.len() && is_ident_byte(bytes[i]) {
            i += 1;
        }
        let word = &sql[start..i];
        if (after_table_keyword && POSTGRES_RESERVED_TABLE_WORDS.contains(&word))
            || is_bare_value_keyword(bytes, start, i, word)
            || is_bare_reserved_column(bytes, start, i, word, expect_by_column)
        {
            out.push('"');
            out.push_str(word);
            out.push('"');
        } else {
            out.push_str(word);
        }
        after_table_keyword =
            word.eq_ignore_ascii_case("from") || word.eq_ignore_ascii_case("join");
        expect_by_column = word.eq_ignore_ascii_case("by");
        in_by_list = in_by_list || expect_by_column;
    }
    out
}

/// Normalize compiled query for Postgres execution (`$1`, `$2`, … placeholders).
///
/// # Errors
///
/// Returns [`Error::Internal`] when the compiled query is not read-only.
pub fn prepare_compiled_postgres(compiled: &CompiledQuery) -> Result<(String, Vec<Value>)> {
    ensure_read_only(&compiled.query_string)?;
    let for_sql = rewrite_value_id_unique_probe_for_document_sql(&compiled.query_string);
    let rewritten =
        quote_reserved_tables_for_postgres(&rewrite_json_extract_for_postgres(&for_sql));
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
    fn postgres_quotes_reserved_table_after_from() {
        assert_eq!(
            quote_reserved_tables_for_postgres(
                "SELECT * FROM user WHERE (primary_email = $param_0 OR primary_email = $param_1) LIMIT 1"
            ),
            "SELECT * FROM \"user\" WHERE (primary_email = $param_0 OR primary_email = $param_1) LIMIT 1"
        );
        assert_eq!(
            quote_reserved_tables_for_postgres(
                "SELECT id FROM task WHERE owner IN (SELECT id FROM user WHERE x = 1) AND g IN (SELECT 1 FROM group AS g JOIN order o ON o.id = g.id)"
            ),
            "SELECT id FROM task WHERE owner IN (SELECT id FROM \"user\" WHERE x = 1) AND g IN (SELECT 1 FROM \"group\" AS g JOIN \"order\" o ON o.id = g.id)"
        );
    }

    #[test]
    fn postgres_quote_leaves_columns_literals_and_plain_tables() {
        let unchanged = [
            "SELECT * FROM account_email WHERE address = $value LIMIT 2",
            "SELECT * FROM task WHERE owner = $param_0 ORDER BY created_at DESC",
            "SELECT * FROM task WHERE note = 'from user' AND ('task:' || id) = $param_0",
            "SELECT * FROM \"user\" WHERE id = $1",
            "SELECT * FROM User WHERE id = $1",
            "SELECT COUNT(*) FROM users",
            "SELECT * FROM task WHERE t.user = $1 AND \"user\" = $2 AND user_id = $user",
            "SELECT current_timestamp() AS now",
        ];
        for sql in unchanged {
            assert_eq!(quote_reserved_tables_for_postgres(sql), sql, "{sql}");
        }
    }

    #[test]
    fn postgres_quotes_value_keyword_columns_anywhere() {
        assert_eq!(
            quote_reserved_tables_for_postgres(
                "SELECT * FROM notification WHERE (user = $param_0) AND read_at IS NULL LIMIT 100"
            ),
            "SELECT * FROM notification WHERE (\"user\" = $param_0) AND read_at IS NULL LIMIT 100"
        );
        assert_eq!(
            quote_reserved_tables_for_postgres(
                "SELECT user, current_date FROM user WHERE note = 'user' ORDER BY user DESC"
            ),
            "SELECT \"user\", \"current_date\" FROM \"user\" WHERE note = 'user' ORDER BY \"user\" DESC"
        );
    }

    #[test]
    fn postgres_quotes_reserved_word_columns_in_predicates_and_by_lists() {
        assert_eq!(
            quote_reserved_tables_for_postgres(
                "SELECT * FROM permission_group_principal WHERE (group = $param_0 OR group = $param_1) LIMIT 1000"
            ),
            "SELECT * FROM permission_group_principal WHERE (\"group\" = $param_0 OR \"group\" = $param_1) LIMIT 1000"
        );
        assert_eq!(
            quote_reserved_tables_for_postgres(
                "SELECT COUNT(*) AS count FROM t WHERE order<>$a AND group IS NOT NULL AND check IN ($b) AND select LIKE $c"
            ),
            "SELECT COUNT(*) AS count FROM t WHERE \"order\"<>$a AND \"group\" IS NOT NULL AND \"check\" IN ($b) AND \"select\" LIKE $c"
        );
        assert_eq!(
            quote_reserved_tables_for_postgres(
                "SELECT * FROM t ORDER BY created_at DESC, group ASC, order LIMIT 5"
            ),
            "SELECT * FROM t ORDER BY created_at DESC, \"group\" ASC, \"order\" LIMIT 5"
        );
    }

    #[test]
    fn postgres_reserved_column_quoting_leaves_keywords_and_literals_sad() {
        let unchanged = [
            "SELECT * FROM t WHERE a = $1 GROUP BY a ORDER BY b DESC",
            "SELECT * FROM t WHERE a IS NULL AND b = true AND c IN (1, 2)",
            "SELECT * FROM t WHERE t.group = $1 AND \"order\" = $2 AND group_id = $3",
            "SELECT * FROM t WHERE note = 'group = 1'",
            "SELECT * FROM t WHERE x IN (SELECT a FROM u ORDER BY a), group_id = 1",
        ];
        for sql in unchanged {
            assert_eq!(quote_reserved_tables_for_postgres(sql), sql, "{sql}");
        }
    }

    #[test]
    fn prepare_compiled_postgres_quotes_user_table() {
        let compiled = CompiledQuery::new(
            "SELECT * FROM user WHERE primary_email = $param_0 LIMIT 1".into(),
            vec![("param_0".into(), json!("account_email:1"))],
        );
        let (sql, values) = prepare_compiled_postgres(&compiled).unwrap();
        assert_eq!(
            sql,
            "SELECT * FROM \"user\" WHERE primary_email = $1 LIMIT 1"
        );
        assert_eq!(values, vec![json!("account_email:1")]);
    }

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
    fn rewrite_json_extract_currency_code() {
        let q = "SELECT id FROM typed_probe WHERE json_extract(price, '$.code') = $param_0";
        let out = rewrite_json_extract_for_postgres(q);
        assert_eq!(
            out,
            "SELECT id FROM typed_probe WHERE (price->>'code') = $param_0"
        );
    }

    #[test]
    fn rewrite_json_extract_amount_minor_int_compare() {
        let q =
            "SELECT id FROM typed_probe WHERE CAST(json_extract(price, '$.amount_minor') AS INTEGER) = $param_0";
        let out = rewrite_json_extract_for_postgres(q);
        assert_eq!(
            out,
            "SELECT id FROM typed_probe WHERE CAST((price->>'amount_minor') AS INTEGER) = $param_0"
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

    #[test]
    fn rewrite_unique_probe_value_id_keeps_typed_column_predicate() {
        let in_sql = "SELECT VALUE id FROM account_email WHERE address = $value LIMIT 2";
        let out = rewrite_value_id_unique_probe_for_document_sql(in_sql);
        assert_eq!(
            out,
            "SELECT id FROM account_email WHERE address = $value LIMIT 2"
        );
    }

    #[test]
    fn prepare_compiled_rewrites_unique_probe() {
        let compiled = CompiledQuery {
            query_string: "SELECT VALUE id FROM account_email WHERE address = $value LIMIT 2"
                .into(),
            params: vec![("value".into(), json!("a@example.com"))],
        };
        let (sql, values) = prepare_compiled(&compiled).expect("prepare");
        assert!(
            !sql.to_ascii_uppercase().contains("SELECT VALUE"),
            "VALUE id projection must be rewritten for typed SQL: {sql}"
        );
        assert!(
            sql.contains("address = ?"),
            "typed column predicate must survive rewrite: {sql}"
        );
        assert!(!sql.contains("json_extract(body"));
        assert_eq!(values, vec![json!("a@example.com")]);
    }

    #[test]
    fn rewrite_leaves_non_probe_queries_alone() {
        let q = "SELECT id FROM account_email WHERE address = $value";
        assert_eq!(rewrite_value_id_unique_probe_for_document_sql(q), q);
    }
}
