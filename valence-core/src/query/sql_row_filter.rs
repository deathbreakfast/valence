//! SELECT WHERE / ORDER / LIMIT / OFFSET support for document-row adapters (mem, IndraDB,
//! Redis, MongoDB, hybrid).
//!
//! Filtering works on the text of the compiled SQL, not on a parsed query. In clauses without
//! `OR`, parameter names are matched as substrings, so `$param_1` can be confused with
//! `$param_10`.

mod clause;

use crate::compiled_query::CompiledQuery;

/// Apply `field {=,>,>=,<,<=} $param` and `LIKE` filters from compiled SQL.
///
/// A clause containing `OR` is matched term by term with `OR` looser than `AND`: a row is kept
/// when every top-level `AND` conjunct matches, and each conjunct matches when any of its `OR`
/// terms does (two record predicates joined by `AND`, for example). A term the filter cannot
/// read excludes the row. A clause without `OR` applies each comparison in turn and ignores
/// fragments it does not recognise.
///
/// Matching is string-level on the compiled SQL. In clauses without `OR`, parameter-name prefix
/// collisions such as `$param_1` and `$param_10` are not told apart.
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

    if clause.to_ascii_uppercase().contains(" OR ") {
        return rows
            .into_iter()
            .filter(|row| clause::clause_matches(row, clause, &compiled.params))
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
            let field = extract_field_ref(&lhs_segment(before));
            if !field.is_empty() {
                rows.retain(|row| row_field_equals(row, &field, value));
            }
            continue;
        }

        let like_needle = format!("LIKE ${key}");
        if clause.to_uppercase().contains(&like_needle.to_uppercase()) {
            if let Some(pattern) = value.as_str() {
                if let Some(like_at) = clause.to_uppercase().find(" LIKE ") {
                    let before = clause[..like_at].trim_end();
                    let field = extract_field_ref(&lhs_segment(before));
                    if !field.is_empty() {
                        rows.retain(|row| {
                            row_string_field(row, &field)
                                .is_some_and(|s| sql_like_matches(s, pattern))
                        });
                    }
                }
            }
        }
    }
    rows
}

/// Match a SQL `LIKE` pattern with `%` wildcards (`%term%`, `term%`, `%term`, exact).
fn sql_like_matches(value: &str, pattern: &str) -> bool {
    let starts = pattern.starts_with('%');
    let ends = pattern.ends_with('%');
    match (starts, ends) {
        (true, true) => {
            let inner = &pattern[1..pattern.len().saturating_sub(1)];
            value.contains(inner)
        }
        (false, true) => value.starts_with(&pattern[..pattern.len().saturating_sub(1)]),
        (true, false) => value.ends_with(&pattern[1..]),
        (false, false) => value == pattern,
    }
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
    let field = extract_field_ref(&lhs_segment(before));
    if field.is_empty() {
        return true;
    }
    rows.retain(|row| row_field_cmp(row, &field, value, op));
    true
}

/// Last AND-conjunct before the comparison op (multi-param WHERE clauses).
fn lhs_segment(before: &str) -> String {
    let upper = before.to_uppercase();
    if let Some(idx) = upper.rfind(" AND ") {
        before[idx + 5..].trim().to_string()
    } else {
        before.trim().to_string()
    }
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
            let as_ = row_string_field(a, &field).unwrap_or("");
            let bs = row_string_field(b, &field).unwrap_or("");
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

/// Parse a SQL LHS into a Valence field path (`name`, `body` path, or `price.code`).
fn extract_field_ref(before: &str) -> String {
    let mut trimmed = before.trim();
    // Strip CAST(… AS INTEGER) wrappers from int JSON subpath compares.
    if let Some(inner) = trimmed
        .strip_prefix("CAST(")
        .or_else(|| trimmed.strip_prefix("cast("))
    {
        if let Some(as_at) = inner.to_uppercase().rfind(" AS ") {
            trimmed = inner[..as_at].trim();
        }
    }
    if let Some(start) = trimmed.find("json_extract(") {
        let rest = &trimmed[start + "json_extract(".len()..];
        if let Some(comma) = rest.find(',') {
            let col = rest[..comma].trim();
            let after = rest[comma + 1..].trim_start();
            if let Some(path_rest) = after.strip_prefix("'$.") {
                if let Some(end) = path_rest.find('\'') {
                    let path = &path_rest[..end];
                    if col == "body" {
                        return path.to_string();
                    }
                    if !col.is_empty() && !path.is_empty() {
                        return format!("{col}.{path}");
                    }
                }
            }
        }
    }
    trimmed
        .rsplit(|c: char| c.is_whitespace() || c == '(')
        .next()
        .unwrap_or("")
        .trim_matches(|c: char| c == '"' || c == '`')
        .to_string()
}

fn order_by_field(order_clause: &str) -> String {
    if order_clause.contains("json_extract(") {
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
mod tests;
