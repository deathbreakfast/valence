//! SELECT WHERE / ORDER / LIMIT / OFFSET support for document-row adapters (mem, IndraDB).

use crate::compiled_query::CompiledQuery;

/// Apply `field {=,>,>=,<,<=} $param` and `LIKE` filters from compiled SQL.
pub fn apply_equality_where(
    mut rows: Vec<serde_json::Value>,
    compiled: &CompiledQuery,
) -> Vec<serde_json::Value> {
    let upper = compiled.query_string.to_uppercase();
    let Some(where_idx) = upper.find(" WHERE ") else {
        return rows;
    };
    let where_part = &compiled.query_string[where_idx + 7..];
    let where_upper = where_part.to_uppercase();
    let end = where_upper
        .find(" ORDER BY ")
        .or_else(|| where_upper.find(" LIMIT "))
        .or_else(|| where_upper.find(" OFFSET "))
        .or_else(|| where_upper.find(" GROUP BY "))
        .unwrap_or(where_part.len());
    let clause = where_part[..end].trim();

    if clause.to_uppercase().contains(" OR ") {
        return rows
            .into_iter()
            .filter(|row| or_equality_matches(row, clause, &compiled.params))
            .collect();
    }

    for (key, value) in &compiled.params {
        // Check longer ops first so `>=` / `<=` are not mistaken for `>` / `<`.
        if apply_cmp_param(&mut rows, clause, key, value, ">=") {
            continue;
        }
        if apply_cmp_param(&mut rows, clause, key, value, "<=") {
            continue;
        }
        if apply_cmp_param(&mut rows, clause, key, value, ">") {
            continue;
        }
        if apply_cmp_param(&mut rows, clause, key, value, "<") {
            continue;
        }

        let needle = format!("= ${key}");
        if let Some(eq_at) = clause.find(&needle) {
            let before = clause[..eq_at].trim_end();
            let field = extract_field_ref(before);
            if !field.is_empty() {
                rows.retain(|row| row_field_equals(row, field, value));
            }
            continue;
        }

        let like_needle = format!("LIKE ${key}");
        if clause.to_uppercase().contains(&like_needle.to_uppercase()) {
            if let Some(prefix) = value.as_str() {
                let prefix = prefix.trim_end_matches('%').to_string();
                if let Some(like_at) = clause.to_uppercase().find(" LIKE ") {
                    let before = clause[..like_at].trim_end();
                    let field = extract_field_ref(before);
                    if !field.is_empty() {
                        rows.retain(|row| {
                            row_string_field(row, field).is_some_and(|s| s.starts_with(&prefix))
                        });
                    }
                }
            }
        }
    }
    rows
}

/// Apply `field {op} $key` when present; returns true if this param was a comparison op.
fn apply_cmp_param(
    rows: &mut Vec<serde_json::Value>,
    clause: &str,
    key: &str,
    value: &serde_json::Value,
    op: &str,
) -> bool {
    let needle = format!("{op} ${key}");
    let Some(at) = clause.find(&needle) else {
        return false;
    };
    // Reject `>` matching inside `>=` (and `<` inside `<=`) when scanning shorter ops.
    if (op == ">" || op == "<") && at > 0 && clause.as_bytes().get(at - 1) == Some(&b'=') {
        return false;
    }
    let before = clause[..at].trim_end();
    let field = extract_field_ref(before);
    if field.is_empty() {
        return true;
    }
    rows.retain(|row| row_field_cmp(row, field, value, op));
    true
}

fn row_field_cmp(
    row: &serde_json::Value,
    field: &str,
    threshold: &serde_json::Value,
    op: &str,
) -> bool {
    let Some(actual) = nested_field(row, field) else {
        return false;
    };
    let Some(ord) = compare_json_values(actual, threshold) else {
        return false;
    };
    match op {
        ">" => ord.is_gt(),
        ">=" => ord.is_ge(),
        "<" => ord.is_lt(),
        "<=" => ord.is_le(),
        _ => false,
    }
}

fn compare_json_values(
    actual: &serde_json::Value,
    expected: &serde_json::Value,
) -> Option<std::cmp::Ordering> {
    if let (Some(a), Some(b)) = (json_as_i64(actual), json_as_i64(expected)) {
        return Some(a.cmp(&b));
    }
    if let (Some(a), Some(b)) = (json_as_f64(actual), json_as_f64(expected)) {
        return a.partial_cmp(&b);
    }
    match (actual.as_str(), expected.as_str()) {
        (Some(a), Some(b)) => Some(a.cmp(b)),
        _ => None,
    }
}

fn json_as_i64(v: &serde_json::Value) -> Option<i64> {
    if let Some(n) = v.as_i64() {
        return Some(n);
    }
    if let Some(n) = v.as_u64() {
        return i64::try_from(n).ok();
    }
    v.as_str().and_then(|s| s.trim().parse().ok())
}

fn json_as_f64(v: &serde_json::Value) -> Option<f64> {
    v.as_f64()
        .or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
}

/// Apply ORDER BY field ASC/DESC plus LIMIT/OFFSET windowing.
pub fn apply_order_limit_offset(
    mut rows: Vec<serde_json::Value>,
    sql: &str,
) -> Vec<serde_json::Value> {
    let upper = sql.to_uppercase();
    if let Some(order_idx) = upper.find(" ORDER BY ") {
        let rest = &sql[order_idx + 10..];
        let rest_upper = rest.to_uppercase();
        let end = rest_upper
            .find(" LIMIT ")
            .or_else(|| rest_upper.find(" OFFSET "))
            .unwrap_or(rest.len());
        let order_clause = rest[..end].trim();
        let desc = order_clause.to_uppercase().contains(" DESC");
        let field = order_by_field(order_clause);
        rows.sort_by(|a, b| {
            let as_ = row_string_field(a, field).unwrap_or("");
            let bs = row_string_field(b, field).unwrap_or("");
            if desc {
                bs.cmp(as_)
            } else {
                as_.cmp(bs)
            }
        });
    }

    let offset = if let Some(off_idx) = upper.rfind(" OFFSET ") {
        let tail = sql[off_idx + 8..].trim();
        let num = tail.split_whitespace().next().unwrap_or("0");
        num.parse().unwrap_or(0)
    } else {
        0usize
    };
    if offset > 0 {
        rows = rows.into_iter().skip(offset).collect();
    }

    if let Some(limit_idx) = upper.rfind(" LIMIT ") {
        let tail = sql[limit_idx + 7..].trim();
        let num = tail.split_whitespace().next().unwrap_or("0");
        if let Ok(limit) = num.parse::<usize>() {
            rows.truncate(limit);
        }
    }
    rows
}

fn or_equality_matches(
    row: &serde_json::Value,
    clause: &str,
    params: &[(String, serde_json::Value)],
) -> bool {
    let stripped = clause.trim().trim_start_matches('(').trim_end_matches(')');
    let normalized = stripped.replace(" or ", " OR ");
    for part in normalized.split(" OR ") {
        let part = part.trim();
        if let Some(eq_at) = part.find(" = $") {
            let before = part[..eq_at].trim_end();
            let key = part[eq_at + 4..]
                .trim()
                .trim_end_matches(|c: char| !c.is_ascii_alphanumeric() && c != '_');
            let field = extract_field_ref(before);
            if let Some((_, value)) = params.iter().find(|(k, _)| k == key) {
                if !field.is_empty() && row_field_equals(row, field, value) {
                    return true;
                }
            }
        }
    }
    false
}

fn extract_field_ref(before: &str) -> &str {
    let trimmed = before.trim();
    if let Some(start) = trimmed.find("json_extract(body, '$.") {
        let rest = &trimmed[start + "json_extract(body, '$.".len()..];
        if let Some(end) = rest.find("')") {
            return &rest[..end];
        }
    }
    if let Some(inner) = trimmed.strip_prefix("json_extract(body, '$.") {
        return inner.trim_end_matches("')");
    }
    trimmed
        .rsplit(|c: char| c.is_whitespace() || c == '(')
        .next()
        .unwrap_or("")
        .trim_matches(|c: char| c == '"' || c == '`')
}

fn order_by_field(order_clause: &str) -> &str {
    if order_clause.contains("json_extract(body, '$.") {
        return extract_field_ref(order_clause);
    }
    let token = order_clause.split_whitespace().next().unwrap_or("id");
    extract_field_ref(token)
}

fn row_string_field<'a>(row: &'a serde_json::Value, field: &str) -> Option<&'a str> {
    if let Some(v) = nested_field(row, field) {
        if let Some(s) = v.as_str() {
            return Some(s);
        }
    }
    None
}

fn nested_field<'a>(row: &'a serde_json::Value, field: &str) -> Option<&'a serde_json::Value> {
    let mut cur = row;
    for part in field.split('.') {
        cur = cur
            .get(part)
            .or_else(|| cur.get("body").and_then(|b| b.get(part)))?;
    }
    Some(cur)
}

fn row_field_equals(row: &serde_json::Value, field: &str, expected: &serde_json::Value) -> bool {
    if let Some(actual) = nested_field(row, field) {
        return values_equal(actual, expected);
    }
    false
}

fn values_equal(actual: &serde_json::Value, expected: &serde_json::Value) -> bool {
    match (actual, expected) {
        (serde_json::Value::String(a), serde_json::Value::String(b)) => a == b,
        (serde_json::Value::Object(obj), serde_json::Value::String(expected_s)) => {
            let id = obj.get("id").and_then(|v| v.as_str()).unwrap_or("");
            let table = obj.get("table").and_then(|v| v.as_str()).unwrap_or("");
            expected_s == id || expected_s == &format!("{table}:{id}")
        }
        (a, b) => a == b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_json_extract_equality_param() {
        let compiled = CompiledQuery::new(
            "SELECT id, body FROM project WHERE json_extract(body, '$.name') = $param_0".into(),
            vec![("param_0".into(), serde_json::json!("alpha"))],
        );
        let rows = vec![
            serde_json::json!({"id": "1", "name": "alpha"}),
            serde_json::json!({"id": "2", "name": "beta"}),
        ];
        let out = apply_equality_where(rows, &compiled);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0]["name"], "alpha");
    }

    #[test]
    fn filters_bare_column_equality_param() {
        let compiled = CompiledQuery::new(
            "SELECT * FROM project WHERE name = $param_0".into(),
            vec![("param_0".into(), serde_json::json!("alpha"))],
        );
        let rows = vec![
            serde_json::json!({"id": {"table": "project", "id": "1"}, "name": "alpha"}),
            serde_json::json!({"id": {"table": "project", "id": "2"}, "name": "beta"}),
        ];
        let out = apply_equality_where(rows, &compiled);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0]["name"], "alpha");
    }

    #[test]
    fn filters_record_or_clause_against_object_fk() {
        let compiled = CompiledQuery::new(
            "SELECT id, body FROM task WHERE (json_extract(body, '$.project') = $param_0 OR json_extract(body, '$.project') = $param_1 OR json_extract(body, '$.project.id') = $param_1)".into(),
            vec![
                ("param_0".into(), serde_json::json!("project:p1")),
                ("param_1".into(), serde_json::json!("p1")),
            ],
        );
        let rows = vec![
            serde_json::json!({"id": "t1", "project": {"table": "project", "id": "p1"}}),
            serde_json::json!({"id": "t2", "project": {"table": "project", "id": "other"}}),
        ];
        let out = apply_equality_where(rows, &compiled);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0]["id"], "t1");
    }

    #[test]
    fn filters_datetime_after_numeric_param() {
        let compiled = CompiledQuery::new(
            "SELECT id, body FROM typed_probe WHERE json_extract(body, '$.at') > $param_0".into(),
            vec![("param_0".into(), serde_json::json!(2_000_000_000_i64))],
        );
        let rows = vec![
            serde_json::json!({"id": "1", "at": 1_700_000_000_i64}),
            serde_json::json!({"id": "2", "at": 2_100_000_000_i64}),
        ];
        let out = apply_equality_where(rows, &compiled);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0]["id"], "2");
    }

    #[test]
    fn filters_datetime_after_miss_when_all_before_threshold() {
        let compiled = CompiledQuery::new(
            "SELECT id, body FROM typed_probe WHERE json_extract(body, '$.at') > $param_0".into(),
            vec![("param_0".into(), serde_json::json!(3_000_000_000_i64))],
        );
        let rows = vec![serde_json::json!({"id": "1", "at": 1_700_000_000_i64})];
        let out = apply_equality_where(rows, &compiled);
        assert!(out.is_empty());
    }

    #[test]
    fn count_where_thing_eq_or_clause_filters_record_fk() {
        // Exact shape emitted by `compiled_query_factory::count_where_thing_eq` for SQL/mem.
        let compiled = CompiledQuery::new(
            "SELECT COUNT(*) AS count FROM account \
             WHERE json_extract(body, '$.user') = $bare_id \
                OR json_extract(body, '$.user.id') = $bare_id \
                OR json_extract(body, '$.user') = $parent_rid"
                .into(),
            vec![
                ("bare_id".into(), serde_json::json!("persona")),
                ("parent_rid".into(), serde_json::json!("user:persona")),
            ],
        );
        let rows = vec![
            serde_json::json!({"id": "a1", "user": {"table": "user", "id": "owner"}}),
            serde_json::json!({"id": "a2", "user": {"table": "user", "id": "persona"}}),
        ];
        let out = apply_equality_where(rows, &compiled);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0]["id"], "a2");
    }
}
